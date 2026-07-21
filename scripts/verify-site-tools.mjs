// Headless runtime verification for plan 24 (site tools). Spins up the mock
// Vite build (no Tauri, no Docker) and checks the Tools tab on SiteDetail:
//   - a WordPress site has a Tools tab; switching to it shows the tool sections;
//   - Search & Replace previews per-column change counts, then Apply appears;
//   - a code-only docker site has no Tools tab at all.
//
//   node scripts/verify-site-tools.mjs
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
  if (isWin) spawn("taskkill", ["/pid", String(child.pid), "/T", "/F"], { stdio: "ignore" });
  else {
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
      defaultViewport: { width: 1440, height: 1200 },
    });
    const page = await browser.newPage();
    page.on("pageerror", (e) => console.warn("  page error:", e.message));
    page.on("dialog", (d) => d.accept());
    await page.goto(URL, { waitUntil: "networkidle0" });

    const bodyText = () => page.evaluate(() => document.body.innerText);
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
    const clickExact = (selector, text) =>
      page.evaluate(
        (sel, t) => {
          const el = [...document.querySelectorAll(sel)].find((e) => e.textContent.trim() === t);
          if (!el) return false;
          el.click();
          return true;
        },
        selector,
        text
      );
    const setInputByPlaceholder = (needle, value) =>
      page.evaluate(
        (n, v) => {
          const input = [...document.querySelectorAll("input")].find(
            (i) => i.placeholder && i.placeholder.includes(n)
          );
          if (!input) return false;
          const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, "value").set;
          setter.call(input, v);
          input.dispatchEvent(new Event("input", { bubbles: true }));
          return true;
        },
        needle,
        value
      );
    const findCard = `(n) => {
      const title = [...document.querySelectorAll('button')].find((b) => b.textContent.trim() === n);
      if (!title) return null;
      let card = title.parentElement;
      while (card && ![...card.querySelectorAll(':scope button')].some((b) => b.textContent.trim() === 'Details')) {
        card = card.parentElement;
      }
      return card;
    }`;
    const openDetail = (siteName) =>
      page.evaluate(
        (n, find) => {
          const card = new Function("return " + find)()(n);
          [...card.querySelectorAll("button")].find((b) => b.textContent.trim() === "Details").click();
        },
        siteName,
        findCard
      );
    const buttonLabels = () =>
      page.evaluate(() => [...document.querySelectorAll("button")].map((b) => b.textContent.trim()));

    await page.waitForFunction(() => document.body.innerText.includes("Pixel Bakery"));
    console.log("› dashboard loaded");

    // 1) WordPress site → Tools tab exists and switches.
    await openDetail("Pixel Bakery");
    await sleep(900);
    ok("WP detail shows a Tools tab", (await buttonLabels()).includes("tools"));
    ok("WP detail defaults to the overview (logs visible)", /Container logs/i.test(await bodyText()));

    await clickExact("button", "tools");
    await sleep(400);
    let text = await bodyText();
    ok("Tools tab shows Search & Replace", /Search & Replace/i.test(text));
    ok("Tools tab hides the overview logs panel", !/Container logs/i.test(text));

    // 2) Search & Replace: preview shows per-column counts + Apply appears.
    ok("filled the 'replace this' field", await setInputByPlaceholder("old.test", "https://old.test"));
    ok("filled the 'with this' field", await setInputByPlaceholder("new.test", "https://new.test"));
    await clickByText("button", "Preview changes");
    await sleep(500);
    text = await bodyText();
    ok("preview reports a total", /19 occurrences in 3 columns would change/i.test(text));
    ok("preview lists a table/column row", /wp_options/i.test(text) && /option_value/i.test(text));
    const canApply = await page.evaluate(() =>
      [...document.querySelectorAll("button")].some(
        (b) => /^Apply — 19 changes$/.test(b.textContent.trim()) && !b.disabled
      )
    );
    ok("Apply button appears with the change count", canApply);

    // Apply → snapshot-first replace → success line + snapshot link.
    await clickByText("button", "Apply — 19");
    await sleep(1600);
    text = await bodyText();
    ok("apply reports success", /Replaced 19 occurrences/i.test(text));
    ok("apply offers the snapshot shortcut", /view snapshots/i.test(text));

    // 3) Debug: the section shows, and toggling on seeds the log viewer.
    ok("Tools tab shows Debug", /Debug/i.test(await bodyText()));
    const debugSwitch = () =>
      page.evaluate(() => {
        const btn = [...document.querySelectorAll('button[role="switch"]')][0];
        return btn ? btn.getAttribute("aria-checked") : null;
      });
    ok("debug starts off", (await debugSwitch()) === "false");
    await page.evaluate(() => {
      [...document.querySelectorAll('button[role="switch"]')][0].click();
    });
    await sleep(500);
    ok("debug toggles on", (await debugSwitch()) === "true");
    text = await bodyText();
    ok("debug log viewer shows seeded output", /PHP Fatal error/i.test(text));
    // Clear log empties the viewer.
    await clickByText("button", "Clear log");
    await sleep(400);
    ok("clear empties the log viewer", /No debug output yet/i.test(await bodyText()));

    await clickByText("button", "Back to sites");
    await sleep(600);

    // 3) Docker site → no Tools tab.
    await openDetail("Analytics API");
    await sleep(900);
    ok("docker detail has no Tools tab", !(await buttonLabels()).includes("tools"));

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
