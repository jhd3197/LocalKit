// Headless runtime verification for plan 22 (multi-stack: kind + capability
// gating). Spins up the mock Vite build (no Tauri, no Docker) and checks that
// a docker-kind site is gated correctly against a WordPress one:
//   - both dashboard cards carry a kind badge (WP / Docker);
//   - the docker card offers no Clone; the WP card does;
//   - the docker SiteDetail hides WP Admin, the credentials + database panels,
//     clone/blueprint and ServerKit push, but still shows Snapshots + Logs;
//   - the WordPress SiteDetail still shows all of those;
//   - the New Site dialog's "Docker project" tab drives an inspect → import
//     flow (path + Inspect → app service/port fields appear).
//
//   node scripts/verify-multistack.mjs
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
    // The specific card for `siteName`: climb from its exact-match title button
    // to the nearest ancestor that owns a Details button (the card itself, not
    // the whole grid — which would match every card's buttons at once).
    const findCard = `(n) => {
      const title = [...document.querySelectorAll('button')].find((b) => b.textContent.trim() === n);
      if (!title) return null;
      let card = title.parentElement;
      while (card && ![...card.querySelectorAll(':scope button')].some((b) => b.textContent.trim() === 'Details')) {
        card = card.parentElement;
      }
      return card;
    }`;
    const cardButtons = (siteName) =>
      page.evaluate(
        (n, find) => {
          const card = new Function('return ' + find)()(n);
          return card ? [...card.querySelectorAll("button")].map((b) => b.textContent.trim()) : null;
        },
        siteName,
        findCard
      );
    const openDetail = (siteName) =>
      page.evaluate(
        (n, find) => {
          const card = new Function('return ' + find)()(n);
          [...card.querySelectorAll("button")].find((b) => b.textContent.trim() === "Details").click();
        },
        siteName,
        findCard
      );

    await page.waitForFunction(() => document.body.innerText.includes("Analytics API"));
    console.log("› dashboard loaded");

    // 1) Dashboard: both kinds carry a kind badge; docker offers no Clone.
    const dockerBtns = await cardButtons("Analytics API");
    const wpBtns = await cardButtons("Pixel Bakery");
    ok("docker card renders", dockerBtns !== null);
    ok("wordpress card renders", wpBtns !== null);
    ok("docker card has no Clone", dockerBtns && !dockerBtns.includes("Clone"));
    ok("wordpress card has a Clone", wpBtns && wpBtns.includes("Clone"));
    const badges = await page.evaluate(() =>
      [...document.querySelectorAll("span")]
        .map((s) => s.textContent.trim())
        .filter((t) => t === "WP" || t === "Docker")
    );
    ok("a Docker kind badge is shown", badges.includes("Docker"));
    ok("a WP kind badge is shown", badges.includes("WP"));

    // 2) Docker SiteDetail: WP-only sections are gone, generic ones remain.
    await openDetail("Analytics API");
    await sleep(900);
    let text = await bodyText();
    ok("navigated to the docker site", text.includes("Back to sites"));
    ok("docker detail hides WP Admin", !/WP Admin/i.test(text));
    ok("docker detail hides the credentials panel", !/WP Admin credentials/i.test(text));
    ok("docker detail hides the database panel", !/Database \(MariaDB\)/i.test(text));
    ok("docker detail hides wp-cli info", !/WordPress info/i.test(text));
    ok("docker detail keeps the Snapshots panel", /snapshots/i.test(text));
    ok("docker detail keeps Container logs", /Container logs/i.test(text));
    ok("docker detail shows the app service", /app service/i.test(text));
    const detailButtons = await page.evaluate(() =>
      [...document.querySelectorAll("button")].map((b) => b.textContent.trim())
    );
    ok("docker detail hides Clone", !detailButtons.includes("Clone"));
    ok("docker detail hides Save as blueprint", !detailButtons.includes("Save as blueprint"));
    ok("docker detail keeps Terminal", detailButtons.includes("Terminal"));

    // Back to the dashboard.
    await clickByText("button", "Back to sites");
    await sleep(600);

    // 3) WordPress SiteDetail still shows everything.
    await openDetail("Pixel Bakery");
    await sleep(900);
    text = await bodyText();
    ok("wordpress detail shows WP Admin", /WP Admin/i.test(text));
    ok("wordpress detail shows the database panel", /Database \(MariaDB\)/i.test(text));
    ok("wordpress detail shows wp-cli info", /WordPress info/i.test(text));
    const wpDetailButtons = await page.evaluate(() =>
      [...document.querySelectorAll("button")].map((b) => b.textContent.trim())
    );
    ok("wordpress detail shows Clone", wpDetailButtons.includes("Clone"));
    ok("wordpress detail shows Save as blueprint", wpDetailButtons.includes("Save as blueprint"));

    await clickByText("button", "Back to sites");
    await sleep(600);

    // 4) New Site dialog → Docker project tab → inspect → import fields.
    await clickByText("button", "New Site");
    await sleep(500);
    ok("dialog opens on the WordPress tab", /install WordPress automatically/i.test(await bodyText()));
    await clickByText("button", "Docker project");
    await sleep(400);
    text = await bodyText();
    ok("docker tab explains the copy", /copies an existing Docker Compose project/i.test(text));
    const hasPathInput = await page.evaluate(() =>
      [...document.querySelectorAll("input")].some(
        (i) => i.placeholder && i.placeholder.includes("docker-compose.yml")
      )
    );
    ok("docker tab shows a project-folder input", hasPathInput);

    // Type a path and inspect (the mock returns a fictional two-service project).
    await page.evaluate(() => {
      const input = [...document.querySelectorAll("input")].find(
        (i) => i.placeholder && i.placeholder.includes("docker-compose.yml")
      );
      const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, "value").set;
      setter.call(input, "C:/dev/analytics-api");
      input.dispatchEvent(new Event("input", { bubbles: true }));
    });
    await clickByText("button", "Inspect");
    await sleep(600);
    text = await bodyText();
    ok("inspect reveals the app service/port fields", /App service/i.test(text) && /App port/i.test(text));
    ok("inspect reports the detected database", /database/i.test(text));
    ok("inspect names the default excludes", /node_modules/i.test(text));
    const canImport = await page.evaluate(() =>
      [...document.querySelectorAll("button")].some(
        (b) => b.textContent.trim() === "Import project" && !b.disabled
      )
    );
    ok("Import project is enabled once inspected", canImport);

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
