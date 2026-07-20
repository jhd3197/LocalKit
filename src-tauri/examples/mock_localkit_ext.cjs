// Mock of the serverkit-localkit extension for LocalKit M4 E2E testing.
// Mimics builtin-extensions/serverkit-localkit/backend/localkit.py contract.
// - Validates X-API-Key (good-key); invalid key -> 401 {'error': ...} on ALL routes.
// - Stores the SQL uploaded via POST /push/db; GET /pull/db returns it gzipped
//   with the local URL rewritten to the remote URL (simulating a remote DB).
// - Implements sync v2 (plan 19): chunked resumable push with an in-memory
//   chunk store, and Range/session downloads on the pull side.
//
// Two mock-only routes exist so m4_smoke can make assertions the real
// extension has no reason to expose:
//   GET  /__stats    — request counters (how many chunks actually got sent)
//   POST /__control  — fault injection: refuse chunk PUTs after N succeed,
//                      which is how the smoke simulates a client dying
//                      mid-upload deterministically instead of racing a kill.
const http = require("http");
const zlib = require("zlib");
const crypto = require("crypto");

const GOOD_KEY = "good-key";
const LOCAL_URL = "http://localhost:8081";
const REMOTE_URL = "https://blog.example.com";
// Capabilities of the real extension; LocalKit gates Import on pull-code and
// the chunked transfer path on sync-v2.
const FEATURES = ["sites", "push-code", "push-db", "pull-db", "pull-code", "sync-v2"];
// Canary file the import E2E looks for after extracting the remote wp-content.
const CANARY_PATH = "wp-content/themes/remote-theme/style.css";
const CANARY_BODY = "/* pulled from the remote site */\n";
let storedSql = null;
let receivedTgz = null;

// --- v2 transfer state -----------------------------------------------------
/** transfer_id -> {siteId, kind, total, chunkSize, sha256, localUrl, buf, received:Map} */
const transfers = new Map();
/** `${session}:${kind}:${siteId}` -> Buffer — a download pinned for resuming. */
const downloadSessions = new Map();

const stats = newStats();
function newStats() {
  return {
    inits: 0,
    resumedInits: 0,
    chunkPuts: 0,
    chunkBytes: 0,
    duplicates: 0,
    finishes: 0,
    rangeGets: 0,
    // Chunks the most recently finished transfer needed in total. The resume
    // assertion is `chunkPuts === totalChunks - <what landed before the kill>`,
    // and that needs the total from the server's own arithmetic.
    lastTotalChunks: 0,
    v1Pushes: 0,
  };
}
function resetStats() {
  for (const k of Object.keys(stats)) stats[k] = 0;
}
/**
 * Test knobs:
 * - failChunksAfter: once N chunks have landed, refuse the rest (stands in for
 *   the client's connection dying mid-upload).
 * - syncV2: drop "sync-v2" from /pair, so a v2-capable client is forced down
 *   the v1 path — that is how the fallback gets exercised.
 */
const control = { failChunksAfter: null, chunksSinceControl: 0, syncV2: true };

const sha256 = (buf) => crypto.createHash("sha256").update(buf).digest("hex");

// --- minimal tar writer ----------------------------------------------------
// Node ships zlib but no tar, and the archive shape is the contract under
// test, so the 512-byte ustar blocks are written out by hand.
function tarEntry(name, body) {
  const header = Buffer.alloc(512);
  const write = (text, offset, len) => header.write(text.slice(0, len), offset, "ascii");
  const octal = (n, offset, len) => write(n.toString(8).padStart(len - 1, "0") + "\0", offset, len);
  write(name, 0, 100);
  octal(0o644, 100, 8); // mode
  octal(0, 108, 8); // uid
  octal(0, 116, 8); // gid
  octal(body.length, 124, 12);
  octal(0, 136, 12); // mtime
  header.write("        ", 148, 8, "ascii"); // checksum placeholder (spaces)
  write("0", 156, 1); // typeflag: regular file
  write("ustar\0", 257, 6);
  write("00", 263, 2);
  let sum = 0;
  for (const b of header) sum += b;
  // Checksum is the odd one out: 6 octal digits then NUL then space, not the
  // (len-1)-digits-then-NUL every other numeric field uses.
  header.write(sum.toString(8).padStart(6, "0") + "\0 ", 148, 8, "ascii");
  const pad = Buffer.alloc((512 - (body.length % 512)) % 512);
  return Buffer.concat([header, body, pad]);
}

function remoteWpContentTgz() {
  const tar = Buffer.concat([
    tarEntry(CANARY_PATH, Buffer.from(CANARY_BODY)),
    tarEntry("wp-content/plugins/remote-plugin/remote-plugin.php", Buffer.from("<?php // remote\n")),
    Buffer.alloc(1024), // end of archive
  ]);
  return zlib.gzipSync(tar);
}

function parseMultipart(body, boundary) {
  // Minimal parser (latin1 = byte-preserving): { fields: {name: value}, file: {filename, data} }
  const text = body.toString("latin1");
  const parts = text.split(`--${boundary}`);
  const out = { fields: {}, file: null };
  for (const part of parts) {
    if (part.length < 10) continue;
    const headerEnd = part.indexOf("\r\n\r\n");
    if (headerEnd === -1) continue;
    const header = part.slice(0, headerEnd);
    let data = part.slice(headerEnd + 4);
    if (data.endsWith("\r\n")) data = data.slice(0, -2);
    const nameMatch = /name="([^"]+)"/.exec(header);
    const fileMatch = /filename="([^"]*)"/.exec(header);
    if (fileMatch && fileMatch[1]) out.file = { filename: fileMatch[1], data: Buffer.from(data, "latin1") };
    else if (nameMatch) out.fields[nameMatch[1]] = data;
  }
  return out;
}

function readBody(req) {
  return new Promise((resolve) => {
    const chunks = [];
    req.on("data", (c) => chunks.push(c));
    req.on("end", () => resolve(Buffer.concat(chunks)));
  });
}

// The panel's MAX_CONTENT_LENGTH. Mirrored here because it is the wall sync
// v2 exists to get over: a v1 multipart push of a real site hits it, while a
// v2 chunk request is 8 MiB no matter how large the payload is.
const MAX_BODY = 100 * 1024 * 1024;

/** Apply the v1 processing rules to an assembled code payload. */
function acceptCodeArchive(buf) {
  if (buf[0] !== 0x1f || buf[1] !== 0x8b) return { error: "not gzip" };
  let tar;
  try {
    tar = zlib.gunzipSync(buf);
  } catch (e) {
    return { error: `not gzip: ${e.message}` };
  }
  if (!tar.includes(Buffer.from("wp-content"))) return { error: "No wp-content found in the archive" };
  receivedTgz = buf.length;
  return null;
}

/** Send binary with ETag + Range support, mirroring Flask's conditional=True. */
function sendBinary(req, res, buf) {
  const etag = `"${sha256(buf).slice(0, 32)}"`;
  const range = req.headers.range;
  const ifRange = req.headers["if-range"];
  const base = { "Content-Type": "application/gzip", ETag: etag, "Accept-Ranges": "bytes" };

  // If-Range that no longer matches means the body changed under the client;
  // per RFC 9110 the correct answer is the whole thing, not a partial one.
  if (range && (!ifRange || ifRange === etag)) {
    const m = /^bytes=(\d+)-(\d*)$/.exec(range);
    if (m) {
      const start = parseInt(m[1], 10);
      const end = m[2] ? Math.min(parseInt(m[2], 10), buf.length - 1) : buf.length - 1;
      if (start >= buf.length || start > end) {
        res.writeHead(416, { ...base, "Content-Range": `bytes */${buf.length}` });
        return res.end();
      }
      stats.rangeGets += 1;
      const slice = buf.subarray(start, end + 1);
      res.writeHead(206, {
        ...base,
        "Content-Length": slice.length,
        "Content-Range": `bytes ${start}-${end}/${buf.length}`,
      });
      return res.end(slice);
    }
  }
  res.writeHead(200, { ...base, "Content-Length": buf.length });
  res.end(buf);
}

/** The bytes a pull should serve, pinned per session so ranges stay coherent. */
function pinnedExport(session, kind, siteId, build) {
  if (!session) return build();
  const key = `${session}:${kind}:${siteId}`;
  if (!downloadSessions.has(key)) downloadSessions.set(key, build());
  return downloadSessions.get(key);
}

const server = http.createServer(async (req, res) => {
  const json = (code, obj) => {
    res.writeHead(code, { "Content-Type": "application/json" });
    res.end(JSON.stringify(obj));
  };
  if (req.headers["x-api-key"] !== GOOD_KEY) {
    return json(401, { error: "Invalid or expired API key" });
  }
  const url = new URL(req.url, "http://x");

  // --- mock-only test hooks ------------------------------------------------
  if (url.pathname === "/api/v1/localkit/__stats") {
    return json(200, { ...stats, transfers: transfers.size });
  }
  if (url.pathname === "/api/v1/localkit/__control" && req.method === "POST") {
    const body = await readBody(req);
    const cfg = body.length ? JSON.parse(body.toString()) : {};
    control.failChunksAfter = cfg.failChunksAfter ?? null;
    control.chunksSinceControl = 0;
    if (cfg.syncV2 !== undefined) control.syncV2 = cfg.syncV2;
    if (cfg.resetStats) resetStats();
    if (cfg.forgetTransfers) transfers.clear();
    return json(200, { ok: true, ...control });
  }

  if (url.pathname === "/api/v1/localkit/pair") {
    const features = control.syncV2 ? FEATURES : FEATURES.filter((f) => f !== "sync-v2");
    return json(200, { status: "ok", service: "serverkit-localkit", panel: "ServerKit", version: "1.7.0", user: "admin", canonical_domain: "panel.example.com", canonical_origin: "https://panel.example.com", features });
  }

  if (url.pathname === "/api/v1/localkit/sites" && req.method === "GET") {
    return json(200, { sites: [
      { id: 1, name: "client-blog", url: REMOTE_URL, site_url: REMOTE_URL, status: "running", wp_version: "6.7.2", php_version: "8.3", multisite: false, environment_count: 0 },
      { id: 2, name: "woo-store", url: null, site_url: null, status: "stopped", wp_version: "6.6.4", php_version: "8.1", multisite: false, environment_count: 1 },
      // Refused by the import flow — one compose project cannot be a network.
      { id: 3, name: "network-hq", url: "https://network.example.com", status: "running", wp_version: "6.7.2", php_version: "8.2", multisite: true, environment_count: 0 },
    ]});
  }

  if (url.pathname === "/api/v1/localkit/sites" && req.method === "POST") {
    const body = await readBody(req);
    return json(201, { success: true, site: { id: 3, name: JSON.parse(body.toString()).name.toLowerCase().replace(/ /g, "-") }, http_port: 8090 });
  }

  // --- sync v2: chunked push ----------------------------------------------

  const initMatch = /^\/api\/v1\/localkit\/push\/(code|db)\/init$/.exec(url.pathname);
  if (initMatch && req.method === "POST") {
    const kind = initMatch[1];
    const body = await readBody(req);
    const data = body.length ? JSON.parse(body.toString()) : {};
    if (!data.site_id) return json(400, { error: "site_id is required" });
    if (!/^[0-9a-f]{64}$/.test(data.sha256 || "")) {
      return json(400, { error: "sha256 must be a hex-encoded SHA-256 digest" });
    }
    stats.inits += 1;

    // Resume: an existing transfer of the identical payload keeps its chunks.
    for (const [id, t] of transfers) {
      if (t.kind === kind && t.siteId === data.site_id && t.sha256 === data.sha256
          && t.total === data.total_bytes && t.chunkSize === data.chunk_size) {
        stats.resumedInits += 1;
        return json(200, {
          transfer_id: id,
          chunk_size: t.chunkSize,
          received: [...t.received.keys()].sort((a, b) => a - b),
          resumed: true,
        });
      }
    }

    const id = crypto.randomBytes(16).toString("hex");
    transfers.set(id, {
      kind,
      siteId: data.site_id,
      total: data.total_bytes,
      chunkSize: data.chunk_size,
      sha256: data.sha256,
      localUrl: data.local_url || "",
      buf: Buffer.alloc(data.total_bytes),
      received: new Map(),
    });
    return json(201, { transfer_id: id, chunk_size: data.chunk_size, received: [], resumed: false });
  }

  const chunkMatch = /^\/api\/v1\/localkit\/push\/(code|db)\/chunk$/.exec(url.pathname);
  if (chunkMatch && req.method === "PUT") {
    const kind = chunkMatch[1];
    const t = transfers.get(url.searchParams.get("transfer_id"));
    if (!t || t.kind !== kind) return json(404, { error: "Unknown or expired transfer" });

    const offset = Number(url.searchParams.get("offset"));
    const chunkSha = url.searchParams.get("sha256");
    const expected = Math.min(t.chunkSize, t.total - offset);
    if (!Number.isInteger(offset) || offset < 0 || offset >= t.total || offset % t.chunkSize !== 0) {
      return json(400, { error: `offset ${offset} is not a chunk boundary of this transfer` });
    }

    if (t.received.get(offset) === chunkSha) {
      stats.duplicates += 1;
      return json(200, { received: [...t.received.keys()].sort((a, b) => a - b), duplicate: true });
    }

    const body = await readBody(req);

    // Fault injection stands in for "the client's connection died here".
    if (control.failChunksAfter != null && control.chunksSinceControl >= control.failChunksAfter) {
      return json(503, { error: "mock: injected chunk failure" });
    }

    if (body.length !== expected) {
      return json(400, { error: `chunk at offset ${offset} must be ${expected} bytes, got ${body.length}` });
    }
    if (sha256(body) !== chunkSha) {
      return json(400, { error: `chunk at offset ${offset} failed its checksum` });
    }
    body.copy(t.buf, offset);
    t.received.set(offset, chunkSha);
    stats.chunkPuts += 1;
    stats.chunkBytes += body.length;
    control.chunksSinceControl += 1;
    return json(200, { received: [...t.received.keys()].sort((a, b) => a - b), duplicate: false });
  }

  const finishMatch = /^\/api\/v1\/localkit\/push\/(code|db)\/finish$/.exec(url.pathname);
  if (finishMatch && req.method === "POST") {
    const kind = finishMatch[1];
    const body = await readBody(req);
    const data = body.length ? JSON.parse(body.toString()) : {};
    const id = data.transfer_id;
    const t = transfers.get(id);
    if (!t || t.kind !== kind) return json(404, { error: "Unknown or expired transfer" });

    const missing = [];
    for (let o = 0; o < t.total; o += t.chunkSize) if (!t.received.has(o)) missing.push(o);
    if (missing.length) {
      return json(409, {
        error: `${missing.length} chunk(s) are still missing`,
        received: [...t.received.keys()].sort((a, b) => a - b),
        missing,
      });
    }
    if (sha256(t.buf) !== t.sha256) {
      transfers.delete(id);
      return json(400, { error: "The assembled upload failed its checksum — nothing was applied." });
    }

    stats.finishes += 1;
    stats.lastTotalChunks = Math.ceil(t.total / t.chunkSize);
    if (kind === "code") {
      const bad = acceptCodeArchive(t.buf);
      if (bad) return json(400, bad);
      transfers.delete(id);
      return json(200, { success: true, message: "wp-content pushed to the site" });
    }
    storedSql = t.buf.toString();
    transfers.delete(id);
    return json(200, { success: true, message: "Database imported", remote_url: REMOTE_URL, search_replace: true });
  }

  // --- v1 push (still exercised: it is the fallback for old servers) -------

  if (url.pathname === "/api/v1/localkit/push/code" && req.method === "POST") {
    const body = await readBody(req);
    if (body.length > MAX_BODY) return json(413, { error: "Request Entity Too Large" });
    const boundary = /boundary=(.+)$/.exec(req.headers["content-type"])[1];
    const { fields, file } = parseMultipart(body, boundary);
    if (!fields.site_id || !file) return json(400, { error: "site_id and file required" });
    stats.v1Pushes += 1;
    const bad = acceptCodeArchive(file.data);
    if (bad) return json(400, bad);
    return json(200, { success: true, message: "wp-content pushed to the site" });
  }

  if (url.pathname === "/api/v1/localkit/push/db" && req.method === "POST") {
    const body = await readBody(req);
    if (body.length > MAX_BODY) return json(413, { error: "Request Entity Too Large" });
    const boundary = /boundary=(.+)$/.exec(req.headers["content-type"])[1];
    const { fields, file } = parseMultipart(body, boundary);
    if (!fields.site_id || !file) return json(400, { error: "site_id and file required" });
    stats.v1Pushes += 1;
    storedSql = file.data.toString();
    return json(200, { success: true, message: "Database imported", remote_url: REMOTE_URL, search_replace: true });
  }

  // --- pull (Range + session, plan 19 phase 3) -----------------------------

  if (url.pathname === "/api/v1/localkit/pull/code" && req.method === "GET") {
    const siteId = url.searchParams.get("site_id");
    if (!siteId) return json(400, { error: "site_id is required" });
    return sendBinary(req, res, pinnedExport(url.searchParams.get("session"), "code", siteId, remoteWpContentTgz));
  }

  if (url.pathname === "/api/v1/localkit/pull/db" && req.method === "GET") {
    if (!storedSql) return json(404, { error: "Site not found" });
    const siteId = url.searchParams.get("site_id");
    return sendBinary(req, res, pinnedExport(url.searchParams.get("session"), "db", siteId, () =>
      zlib.gzipSync(Buffer.from(storedSql.split(LOCAL_URL).join(REMOTE_URL)))
    ));
  }

  json(404, { error: "Not found" });
});

server.listen(9872, "127.0.0.1", () => console.log("mock localkit extension on :9872"));
