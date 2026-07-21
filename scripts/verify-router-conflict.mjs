// Headless runtime verification for plan 16 (router coexistence).
//
// Spins up the mock Vite build (no Tauri runtime, no Docker) and walks the
// port-conflict matrix from the plan: a fictional LocalWP holds 80/443, so
// enabling local domains must surface a NAMED conflict (not a silent
// failure), "Use fallback ports" must recover to 8080/8443, site URLs must
// gain the port, and the SiteDetail banner must appear while blocked.
//
//   node scripts/verify-router-conflict.mjs
//
import { spawn } from "node:child_process";
import { setTimeout as sleep } from "node:timers/promises";
import { existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";
import puppeteer from "puppeteer-core";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(__dirname, "..");
const PORT = 1426;
const URL = `http://localhost:${PORT}/`;

const CHROME_CANDIDATES = [
  "C:/Program Files/Google/Chrome/Application/chrome.exe",
  "C:/Program Files (x86)/Google/Chrome/Application/chrome.exe",
  "C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe",
  "C:/Program Files/Microsoft/Edge/Application/msedge.exe",
];
const chrome = CHROME_CANDIDATES.find((p) => existsSync(p));
if (!chrome) {
  console.error("No Chrome/Edge found.");
  process.exit(1);
}

const isWin = process.platform === "win32";
let failures = 0;

function ok(name, cond) {
  if (cond) console.log("  ✓", name);
  else {
    failures++;
    console.error("  ✗ FAIL:", name);
  }
}

async function waitForServer(url, ms = 60_000) {
  const deadline = Date.now() + ms;
  while (Date.now() < deadline) {
    try {
      const r = await fetch(url);
      if (r.ok) return;
    } catch {}
    await sleep(500);
  }
  throw new Error(`Vite mock server never came up at ${url}`);
}

function killTree(child) {
  if (!child || child.killed) return;
  if (isWin) {
    spawn("taskkill", ["/pid", String(child.pid), "/T", "/F"], { stdio: "ignore" });
  } else {
    try {
      process.kill(-child.pid, "SIGKILL");
    } catch {}
  }
}

async function main() {
  console.log("› starting mock Vite server…");
  const server = spawn("npm", ["run", "dev:mock"], {
    cwd: ROOT,
    shell: true,
    stdio: "ignore",
    detached: !isWin,
  });

  let browser;
  try {
    await waitForServer(URL);
    browser = await puppeteer.launch({
      executablePath: chrome,
      headless: true,
      defaultViewport: { width: 1440, height: 1000 },
    });
    const page = await browser.newPage();
    page.on("pageerror", (e) => console.warn("  page error:", e.message));
    await page.goto(URL, { waitUntil: "networkidle0" });

    const bodyText = () => page.evaluate(() => document.body.innerText);
    /** Click the first element whose trimmed text matches `text`. */
    const clickByText = (selector, text) =>
      page.evaluate(
        (sel, t) => {
          const el = [...document.querySelectorAll(sel)].find((e) =>
            e.textContent.trim().toLowerCase().includes(t.toLowerCase())
          );
          if (!el) return false;
          el.click();
          return true;
        },
        selector,
        text
      );

    await page.waitForFunction(() => document.body.innerText.includes("Pixel Bakery"));
    console.log("› dashboard loaded");

    // Open Settings → Local domains.
    await page.evaluate(() => {
      const gear = [...document.querySelectorAll("button")].find(
        (b) => (b.getAttribute("aria-label") || "").toLowerCase().includes("setting")
      );
      gear?.click();
    });
    await sleep(400);
    await clickByText("button", "Local domains");
    await sleep(500);
    ok("Domains settings shows the default ports", (await bodyText()).includes("80/443"));

    // 1) Toggle domains OFF then ON — the mock LocalWP owns 80/443, so
    //    re-enabling must hit the pre-flight and report a NAMED conflict.
    const toggle = 'button[aria-label="Enable local domains"]';
    await page.click(toggle);
    await sleep(500);
    await page.click(toggle);
    await sleep(800);

    let text = await bodyText();
    ok("conflict names the holding process", text.includes("httpd.exe"));
    ok("conflict names both ports", text.includes("port 80") && text.includes("port 443"));
    ok("status reads as blocked, not a bare failure", text.includes("blocked by another program"));
    ok("offers the fallback-ports action", text.includes("Use fallback ports"));
    ok("offers Retry", /\bRetry\b/.test(text));

    // 2) SiteDetail banner — the *persistent* hazard, which is a different
    //    state from the failed enable above: domains are ON (hosts entries
    //    written, WordPress URLs already rewritten to <slug>.test) and the
    //    router later lost its ports. That's when the user is actually
    //    staring at the other program's 404, so that's when the banner fires.
    //    A failed enable changes nothing, so it deliberately has no banner.
    await page.evaluate(() => {
      const mock = window.__LOCALKIT_MOCK__;
      mock.routerStatus.enabled = true;
      mock.routerStatus.running = false;
    });
    await page.evaluate(() => {
      document.querySelector('button[aria-label="Close settings"]')?.click();
    });
    await sleep(400);
    // Click the "Details" button inside the Pixel Bakery card — matching on
    // card text alone hits a wrapping div and silently stays on the dashboard.
    await page.evaluate(() => {
      const card = [...document.querySelectorAll("div")].find(
        (d) =>
          d.textContent.includes("Pixel Bakery") &&
          [...d.querySelectorAll("button")].some((b) => b.textContent.trim() === "Details")
      );
      [...card.querySelectorAll("button")].find((b) => b.textContent.trim() === "Details").click();
    });
    await sleep(900);
    text = await bodyText();
    ok("navigated to SiteDetail", text.includes("Back to sites"));
    ok("SiteDetail warns local domains are blocked", text.includes("Local domains are blocked"));
    ok("SiteDetail names the holder", text.includes("httpd.exe"));
    ok("SiteDetail still offers the working localhost URL", /localhost:\d+/.test(text));

    // Dismiss is sticky for that conflict.
    await clickByText("button", "Dismiss");
    await sleep(400);
    ok("banner dismisses", !(await bodyText()).includes("Local domains are blocked"));

    // 3) One-click recovery: fallback ports resolve the conflict.
    await page.evaluate(() => {
      const gear = [...document.querySelectorAll("button")].find(
        (b) => (b.getAttribute("aria-label") || "").toLowerCase().includes("setting")
      );
      gear?.click();
    });
    await sleep(400);
    await clickByText("button", "Local domains");
    await sleep(400);
    await clickByText("button", "Use fallback ports");
    await sleep(1000);

    text = await bodyText();
    ok("router recovers onto the fallback ports", text.includes("8080/8443"));
    ok("fallback mode is labelled", text.toLowerCase().includes("fallback"));
    ok("conflict callout is gone", !text.includes("Another program is using"));

    // 4) Site URLs must now carry the port (the whole point of phase 2).
    await page.evaluate(() => {
      document.querySelector('button[aria-label="Close settings"]')?.click();
    });
    await sleep(500);
    text = await bodyText();
    ok("dashboard site URLs carry the fallback port", /\.test:8080/.test(text));

    console.log(failures === 0 ? "\n✓ all checks passed" : `\n✗ ${failures} check(s) failed`);
  } finally {
    if (browser) await browser.close();
    killTree(server);
  }
  process.exit(failures === 0 ? 0 : 1);
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
