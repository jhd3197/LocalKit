// Mock of the serverkit-localkit extension for LocalKit M4 E2E testing.
// Mimics builtin-extensions/serverkit-localkit/backend/localkit.py contract.
// - Validates X-API-Key (good-key); invalid key -> 401 {'error': ...} on ALL routes.
// - Stores the SQL uploaded via POST /push/db; GET /pull/db returns it gzipped
//   with the local URL rewritten to the remote URL (simulating a remote DB).
const http = require("http");
const zlib = require("zlib");

const GOOD_KEY = "good-key";
const LOCAL_URL = "http://localhost:8081";
const REMOTE_URL = "https://blog.example.com";
// Capabilities of the real extension (plan 18); LocalKit gates Import on this.
const FEATURES = ["sites", "push-code", "push-db", "pull-db", "pull-code"];
// Canary file the import E2E looks for after extracting the remote wp-content.
const CANARY_PATH = "wp-content/themes/remote-theme/style.css";
const CANARY_BODY = "/* pulled from the remote site */\n";
let storedSql = null;
let receivedTgz = null;

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

const server = http.createServer((req, res) => {
  const json = (code, obj) => {
    res.writeHead(code, { "Content-Type": "application/json" });
    res.end(JSON.stringify(obj));
  };
  if (req.headers["x-api-key"] !== GOOD_KEY) {
    return json(401, { error: "Invalid or expired API key" });
  }
  const url = new URL(req.url, "http://x");

  if (url.pathname === "/api/v1/localkit/pair") {
    return json(200, { status: "ok", service: "serverkit-localkit", panel: "ServerKit", version: "1.7.0", user: "admin", canonical_domain: "panel.example.com", canonical_origin: "https://panel.example.com", features: FEATURES });
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
    let body = "";
    req.on("data", (c) => (body += c));
    req.on("end", () => json(201, { success: true, site: { id: 3, name: JSON.parse(body).name.toLowerCase().replace(/ /g, "-") }, http_port: 8090 }));
    return;
  }

  if (url.pathname === "/api/v1/localkit/push/code" && req.method === "POST") {
    const chunks = [];
    req.on("data", (c) => chunks.push(c));
    req.on("end", () => {
      const body = Buffer.concat(chunks);
      const boundary = /boundary=(.+)$/.exec(req.headers["content-type"])[1];
      const { fields, file } = parseMultipart(body, boundary);
      if (!fields.site_id || !file) return json(400, { error: "site_id and file required" });
      if (file.data[0] !== 0x1f || file.data[1] !== 0x8b) return json(400, { error: "not gzip" });
      const tar = zlib.gunzipSync(file.data);
      if (!tar.includes(Buffer.from("wp-content"))) return json(400, { error: "No wp-content found in the archive" });
      receivedTgz = file.data.length;
      json(200, { success: true, message: "wp-content pushed to the site" });
    });
    return;
  }

  if (url.pathname === "/api/v1/localkit/push/db" && req.method === "POST") {
    const chunks = [];
    req.on("data", (c) => chunks.push(c));
    req.on("end", () => {
      const body = Buffer.concat(chunks);
      const boundary = /boundary=(.+)$/.exec(req.headers["content-type"])[1];
      const { fields, file } = parseMultipart(body, boundary);
      if (!fields.site_id || !file) return json(400, { error: "site_id and file required" });
      storedSql = file.data.toString();
      json(200, { success: true, message: "Database imported", remote_url: REMOTE_URL, search_replace: true });
    });
    return;
  }

  if (url.pathname === "/api/v1/localkit/pull/code" && req.method === "GET") {
    if (!url.searchParams.get("site_id")) return json(400, { error: "site_id is required" });
    const gz = remoteWpContentTgz();
    res.writeHead(200, { "Content-Type": "application/gzip" });
    res.end(gz);
    return;
  }

  if (url.pathname === "/api/v1/localkit/pull/db" && req.method === "GET") {
    if (!storedSql) return json(404, { error: "Site not found" });
    const remoteSql = storedSql.split(LOCAL_URL).join(REMOTE_URL);
    const gz = zlib.gzipSync(Buffer.from(remoteSql));
    res.writeHead(200, { "Content-Type": "application/gzip" });
    res.end(gz);
    return;
  }

  json(404, { error: "Not found" });
});

server.listen(9872, "127.0.0.1", () => console.log("mock localkit extension on :9872"));
