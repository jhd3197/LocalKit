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
    const scrollTop = () => page.evaluate(() => window.scrollTo(0, 0));
    // Dismiss any toasts before a shot — the mock raises an "update available"
    // launch toast that would otherwise sit in every capture.
    const clearToasts = async () => {
      await page.evaluate(() => {
        document
          .querySelectorAll('.fixed.bottom-4.right-4 button[aria-label="Dismiss"]')
          .forEach((b) => b.click());
      });
      await sleep(150);
    };
    const shot = async (name) => {
      await clearToasts();
      await page.screenshot({ path: path.join(OUT, `${name}.png`) });
      console.log("  ✓", `${name}.png`);
    };
    // Crop a single element (e.g. one panel/card) instead of the viewport —
    // puppeteer scrolls it into view and clips to its bounding box.
    const shotElement = async (name, selectorText, tag = "section", heading = "h2") => {
      const handle = await page.evaluateHandle(
        (t, sel, h) => {
          const el = [...document.querySelectorAll(h)].find(
            (x) => x.textContent.trim() === t
          );
          return el ? el.closest(sel) : null;
        },
        selectorText,
        tag,
        heading
      );
      const el = handle.asElement();
      if (!el) throw new Error(`could not find ${tag} for ${heading} "${selectorText}"`);
      await clearToasts();
      await el.screenshot({ path: path.join(OUT, `${name}.png`) });
      console.log("  ✓", `${name}.png (element)`);
    };
    // Wait until some text appears anywhere in the *visible* document
    // (innerText skips display:none, so hidden tabs don't count).
    const waitForText = (text, timeout = 15_000) =>
      page.waitForFunction(
        (t) => document.body.innerText.toLowerCase().includes(t.toLowerCase()),
        { timeout },
        text
      );
    // Click the first element matching `selector` whose trimmed text is `text`.
    // Uses a direct .click() dispatch, so it works even when the element is
    // visually covered by another overlay (no pointer hit-testing).
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
    // Direct-dispatch click on a button by aria-label (survives overlays).
    const clickAria = async (label) => {
      const ok = await page.evaluate((l) => {
        const el = document.querySelector(`button[aria-label="${l}"]`);
        if (!el) return false;
        el.click();
        return true;
      }, label);
      if (!ok) throw new Error(`no button[aria-label="${label}"]`);
    };
    // Click the "Import" button in the remote-sites table row named `name`.
    const clickImportInRow = async (name) => {
      const result = await page.evaluate((n) => {
        const row = [...document.querySelectorAll("tr")].find(
          (tr) => tr.querySelector("td")?.textContent.trim() === n
        );
        if (!row) return "no-row";
        const btn = [...row.querySelectorAll("button")].find(
          (b) => b.textContent.trim() === "Import"
        );
        if (!btn) return "no-button";
        if (btn.disabled) return "disabled";
        btn.click();
        return "ok";
      }, name);
      if (result !== "ok") throw new Error(`Import for "${name}": ${result}`);
    };

    // 1) Dashboard — grid view: kind badges, status badges (running / degraded /
    //    stopped / creating), a half-created site, plus PHP and Docker sites.
    await waitForText("Pixel Bakery");
    await waitForText("Analytics API"); // the docker site — kinds have loaded
    await settle(600);
    await shot("dashboard");

    // 1b) Dashboard — dense list view via the toolbar toggle.
    await clickAria("List view");
    await settle(400);
    await shot("dashboard-list");
    await clickAria("Grid view");
    await settle(300);

    // 2) Site detail — open a running WordPress site (overview tab: credentials,
    //    database, wp-cli info, snapshots, sync).
    await clickText("button", "Pixel Bakery");
    await waitForText("Core version:");
    await waitForText("Snapshots");
    await settle(600);
    await scrollTop();
    await shot("site-detail");

    // 2b) Snapshots panel on its own — the manual + before-push/pull history
    //     that makes every destructive action reversible (plan 17).
    await shotElement("snapshots", "Snapshots");
    await scrollTop();

    // 2c) Tools tab (plan 24) — Adminer DB browser, search-replace, WP_DEBUG,
    //     config editor. Capability-gated, so it only shows on kinds that have it.
    await clickText("button", "tools");
    await waitForText("Adminer");
    await settle(500);
    await scrollTop();
    await shot("site-tools");

    // 3) New site dialog — back to the dashboard, open it, type a name. The
    //    WordPress tab shows the version selects and the blueprint picker.
    await clickText("button", "← Back to sites");
    await waitForText("Pixel Bakery");
    await clickText("button", "New Site");
    await waitForText("New site");
    await page.type('input[placeholder="My Blog"]', "Portfolio Redesign");
    await settle(400);
    await shot("new-site");
    await clickText("button", "Cancel");
    await settle(300);

    // 4) Settings modal — opened via the sidebar gear icon; sectioned rail.
    await clickAria("Settings");
    await waitForText("Docker is running");
    await settle(600);
    await shot("settings");
    await clickText("nav button", "Local domains");
    await waitForText("Trust HTTPS certificate");
    await settle(400);
    await shot("settings-domains");
    await clickText("nav button", "ServerKit");
    await waitForText("Production");
    await settle(300);
    // Expand the connection's remote WordPress sites — the table with the
    // per-site Import buttons (plan 18) — then scroll the modal's content pane
    // so that table is in frame (it starts below the fold).
    await clickText("button", "View WP sites");
    await waitForText("legacy-shop");
    await page.evaluate(() => {
      const h = [...document.querySelectorAll("h3")].find(
        (x) => x.textContent.trim().toLowerCase() === "remote wordpress sites"
      );
      let el = h?.parentElement;
      while (el && el.scrollHeight <= el.clientHeight) el = el.parentElement;
      if (el) el.scrollTop = el.scrollHeight;
    });
    await settle(500);
    await shot("settings-serverkit");

    // 5) Import a remote site — the confirm dialog, showing the version readout
    //    and the closest-image fallback warning (legacy-shop: 6.2→6.7, 7.4→8.3).
    //    The dialog is app-level, so closing Settings leaves it over the
    //    dashboard for a clean shot.
    await clickImportInRow("legacy-shop");
    await waitForText("rewriting URLs to the local address");
    await clickAria("Close settings");
    await settle(500);
    await shot("import-site");
    await clickText("button", "Cancel");

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
