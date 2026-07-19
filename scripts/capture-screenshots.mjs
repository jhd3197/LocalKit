// Headless screenshot capture for LocalKit's README.
//
// Spins up the mock Vite build (no Tauri runtime, no Docker), drives the UI by
// clicking real buttons, and writes PNGs into docs/screenshots/. Uses an
// already-installed Chrome/Edge via puppeteer-core — no browser download.
//
//   npm run shots
//
import { spawn } from "node:child_process";
import { setTimeout as sleep } from "node:timers/promises";
import { existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";
import puppeteer from "puppeteer-core";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(__dirname, "..");
const OUT = path.join(ROOT, "docs", "screenshots");
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
  console.error("No Chrome/Edge found. Install one or edit CHROME_CANDIDATES.");
  process.exit(1);
}

const isWin = process.platform === "win32";

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
  const server = spawn(
    "npm",
    ["run", "dev:mock"],
    { cwd: ROOT, shell: true, stdio: "ignore", detached: !isWin }
  );

  let browser;
  try {
    await waitForServer(URL);
    console.log("› server up, launching browser…");

    browser = await puppeteer.launch({
      executablePath: chrome,
      headless: true,
      defaultViewport: { width: 1440, height: 900, deviceScaleFactor: 2 },
      args: ["--force-color-profile=srgb", "--hide-scrollbars"],
    });
    const page = await browser.newPage();
    page.on("pageerror", (e) => console.warn("  page error:", e.message));
    await page.goto(URL, { waitUntil: "networkidle0" });

    const settle = (ms = 450) => sleep(ms);
    const shot = async (name) => {
      const file = path.join(OUT, `${name}.png`);
      await page.screenshot({ path: file });
      console.log("  ✓", `${name}.png`);
    };
    // Wait until some text appears anywhere in the document (case-insensitive;
    // CSS `uppercase` classes change what innerText returns).
    const waitForText = (text, timeout = 15_000) =>
      page.waitForFunction(
        (t) => document.body.innerText.toLowerCase().includes(t.toLowerCase()),
        { timeout },
        text
      );
    // Click the first element matching `selector` whose trimmed text is `text`.
    const clickText = async (selector, text) => {
      const ok = await page.evaluate(
        (sel, t) => {
          const el = [...document.querySelectorAll(sel)].find(
            (x) => x.textContent.trim() === t
          );
          if (!el) return false;
          el.click();
          return true;
        },
        selector,
        text
      );
      if (!ok) throw new Error(`could not click ${selector} "${text}"`);
    };

    // 1) Dashboard — grid view with status badges.
    await waitForText("Pixel Bakery");
    await waitForText("Client Demo");
    await settle(600);
    await shot("dashboard");

    // 1b) Dashboard — dense list view via the toolbar toggle.
    await page.click('button[aria-label="List view"]');
    await settle(400);
    await shot("dashboard-list");
    await page.click('button[aria-label="Grid view"]');
    await settle(300);

    // 2) Site detail — open a running site (credentials, wp-cli info, sync).
    await clickText("button", "Pixel Bakery");
    await waitForText("Core version:");
    await waitForText("Sync history");
    await settle(600);
    await shot("site-detail");

    // 3) New site dialog — back to the dashboard, open it, type a name.
    await clickText("button", "← Back to sites");
    await waitForText("Pixel Bakery");
    await clickText("button", "New Site");
    await waitForText("New WordPress site");
    await page.type('input[placeholder="My Blog"]', "Portfolio Redesign");
    await settle(400);
    await shot("new-site");
    await clickText("button", "Cancel");
    await settle(300);

    // 4) Settings modal — opened via the sidebar gear icon; sectioned rail.
    await page.click('button[aria-label="Settings"]');
    await waitForText("Docker is running");
    await settle(600);
    await shot("settings");
    await clickText("nav button", "Local domains");
    await waitForText("Trust HTTPS certificate");
    await settle(400);
    await shot("settings-domains");
    await clickText("nav button", "ServerKit");
    await waitForText("Production");
    await settle(400);
    await shot("settings-serverkit");

    console.log("› done. Wrote PNGs to docs/screenshots/");
  } finally {
    if (browser) await browser.close();
    killTree(server);
  }
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
