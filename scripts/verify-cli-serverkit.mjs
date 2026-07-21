// Headless runtime check of the plan-21 `lk` ServerKit surface against the mock
// serverkit-localkit extension (examples/mock_localkit_ext.cjs). It shells out
// to the compiled `lk` binary with a throwaway --data-dir, so it exercises the
// real CLI (arg parsing, resolution, exit codes, JSON shapes) — not a stand-in.
//
// Covered: connection add (env key) → list --json (key redacted) → test →
// sites --remote --json → add-with-bad-key refusal → push/pull argument errors
// → completions for all shells → remove. The Docker-backed push/pull path is
// exercised by `cargo run --example m4_smoke` and the arg resolution by the
// `lk` unit tests; this script covers everything that talks to a live server
// without needing Docker.
//
// Prereq: build the binary first — `cd src-tauri && cargo build -p lk`.
// Run:    node scripts/verify-cli-serverkit.mjs
import { spawn, spawnSync } from "node:child_process";
import { existsSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const isWin = process.platform === "win32";
const binName = isWin ? "lk.exe" : "lk";
const LK = join(root, "src-tauri", "target", "debug", binName);
const MOCK = join(root, "src-tauri", "examples", "mock_localkit_ext.cjs");
const MOCK_URL = "http://127.0.0.1:9872";
const API_KEY = "good-key";

if (!existsSync(LK)) {
  console.error(`lk binary not found at ${LK}\n  build it first: cd src-tauri && cargo build -p lk`);
  process.exit(1);
}

const dataDir = mkdtempSync(join(tmpdir(), "lk-cli-verify-"));
let failures = 0;
let mock;

/** Run `lk` with the scratch data dir; returns {code, stdout, stderr}. */
function lk(args, { key } = {}) {
  const env = { ...process.env };
  if (key !== undefined) env.LOCALKIT_API_KEY = key;
  else delete env.LOCALKIT_API_KEY;
  const r = spawnSync(LK, ["--no-color", "--data-dir", dataDir, ...args], {
    encoding: "utf8",
    env,
  });
  return { code: r.status, stdout: r.stdout ?? "", stderr: r.stderr ?? "" };
}

function check(label, cond, detail = "") {
  if (cond) {
    console.log(`  ✓ ${label}`);
  } else {
    failures += 1;
    console.error(`  ✗ ${label}${detail ? ` — ${detail}` : ""}`);
  }
}

function waitForMock(timeoutMs = 8000) {
  const started = Date.now();
  return new Promise((resolve, reject) => {
    const tick = async () => {
      try {
        const res = await fetch(`${MOCK_URL}/api/v1/system/health`);
        if (res.ok) return resolve();
      } catch {
        /* not up yet */
      }
      if (Date.now() - started > timeoutMs) return reject(new Error("mock did not start"));
      setTimeout(tick, 150);
    };
    tick();
  });
}

async function main() {
  mock = spawn("node", [MOCK], { stdio: "ignore" });
  await waitForMock();

  console.log("connection add (env key):");
  let r = lk(["connection", "add", "mock", MOCK_URL], { key: API_KEY });
  check("exit 0", r.code === 0, `code=${r.code} ${r.stderr.trim()}`);
  check("id on stdout", r.stdout.trim().length > 0);
  check("extension features on stderr", /features:/.test(r.stderr));

  console.log("connection list --json:");
  r = lk(["connection", "list", "--json"]);
  let list;
  try {
    list = JSON.parse(r.stdout);
  } catch {
    list = null;
  }
  check("valid JSON array of 1", Array.isArray(list) && list.length === 1, r.stdout.trim());
  check("api key redacted", r.stdout.includes("mock") && !/api_key|good-key/.test(r.stdout));

  console.log("connection test:");
  r = lk(["connection", "test", "mock"]);
  check("exit 0", r.code === 0, r.stderr.trim());
  check("reports extension installed", /extension: installed/.test(r.stdout));

  console.log("sites --remote mock --json:");
  r = lk(["sites", "--remote", "mock", "--json"]);
  let sites;
  try {
    sites = JSON.parse(r.stdout);
  } catch {
    sites = null;
  }
  check("valid JSON array of 3", Array.isArray(sites) && sites.length === 3, r.stdout.trim());
  check("multisite flag present", Array.isArray(sites) && sites.some((s) => s.multisite === true));

  console.log("connection add with a bad key is refused:");
  r = lk(["connection", "add", "badconn", MOCK_URL, "--key", "wrong-key"]);
  check("exit 1", r.code === 1, `code=${r.code}`);
  check("not stored", (() => {
    const l = lk(["connection", "list", "--json"]);
    try {
      return JSON.parse(l.stdout).length === 1;
    } catch {
      return false;
    }
  })());

  console.log("push/pull argument errors:");
  r = lk(["push", "nope", "--code", "--connection", "mock", "--remote-site", "1"]);
  check("push on missing site → exit 1", r.code === 1, `code=${r.code}`);
  r = lk(["pull", "nope"]);
  check("pull without --db → exit 1", r.code === 1, `code=${r.code}`);
  check("pull guidance points at lk import", /lk import/.test(r.stderr));

  console.log("completions for every shell:");
  for (const shell of ["bash", "zsh", "fish", "powershell"]) {
    const c = lk(["completions", shell]);
    check(`${shell} non-empty & mentions connection`, c.code === 0 && c.stdout.includes("connection"));
  }

  console.log("connection remove:");
  r = lk(["connection", "remove", "mock", "--yes"]);
  check("exit 0", r.code === 0, r.stderr.trim());
  r = lk(["connection", "list", "--json"]);
  check("list now empty", r.stdout.trim() === "[]", r.stdout.trim());
}

main()
  .catch((e) => {
    console.error(`fatal: ${e.message}`);
    failures += 1;
  })
  .finally(() => {
    if (mock) mock.kill();
    rmSync(dataDir, { recursive: true, force: true });
    if (failures > 0) {
      console.error(`\n${failures} check(s) failed`);
      process.exit(1);
    }
    console.log("\nlk ServerKit CLI verified OK");
  });
