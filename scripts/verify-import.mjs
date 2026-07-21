// Headless runtime verification for plan 18 (import a remote site as a new
// local site).
//
// Spins up the mock Vite build (no Tauri runtime, no Docker) and walks the
// import UX: Settings → ServerKit lists the remote sites with per-row Import
// buttons, multisite rows are refused up front, the dialog reports the version
// match (and warns when there is no exact image), importing streams the same
// progress stages the backend emits, and the new site lands on the dashboard
// carrying its origin badge.
//
//   node scripts/verify-import.mjs
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

/**
 * Open Settings (sidebar gear) and select the ServerKit section from the
 * left rail — the modal opens on whatever section nav last deep-linked to.
 */
async function openServerKitSettings(page) {
  await page.evaluate(() => {
    const gear = [...document.querySelectorAll("button")].find(
      (b) => b.getAttribute("aria-label") === "Settings"
    );
    gear?.click();
  });
  await sleep(600);
  await page.evaluate(() => {
    const rail = document.querySelector('[aria-label="Settings"] nav');
    const btn = [...(rail?.querySelectorAll("button") ?? [])].find((b) =>
      b.textContent.trim().toLowerCase().includes("serverkit")
    );
    btn?.click();
  });
  await sleep(600);
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
    /** The remote-site table as {name, wp, importable, tooltip} records. */
    const remoteRows = () =>
      page.evaluate(() => {
        const table = [...document.querySelectorAll("table")].find((t) =>
          t.textContent.includes("acme-corporate")
        );
        if (!table) return [];
        return [...table.querySelectorAll("tbody tr")].map((tr) => {
          const cells = [...tr.querySelectorAll("td")].map((td) => td.innerText.trim());
          const btn = tr.querySelector("button");
          return {
            name: cells[0],
            wp: cells[3],
            importable: btn ? !btn.disabled : false,
            tooltip: btn?.title ?? "",
          };
        });
      });

    await page.waitForFunction(() => document.body.innerText.includes("Pixel Bakery"));
    console.log("› dashboard loaded");

    // 0) The dashboard already marks the seeded imported site.
    let text = await bodyText();
    ok("imported site shows its origin connection", text.includes("Production"));

    // 1) Settings → ServerKit → expand the connection.
    await openServerKitSettings(page);
    // Panel headings are CSS-uppercased, and innerText applies text-transform.
    ok("settings opened on the ServerKit section", /serverkit connections/i.test(await bodyText()));

    await clickByText("button", "View WP sites");
    await page.waitForFunction(
      () => document.body.innerText.includes("agency-network"),
      { timeout: 15_000 }
    );
    await sleep(600);
    console.log("› remote sites listed");

    // 2) Import buttons: present per row, refused for multisite.
    const rows = await remoteRows();
    ok("every remote site row has an Import control", rows.length === 5);
    const network = rows.find((r) => r.name === "agency-network");
    const bakery = rows.find((r) => r.name === "pixel-bakery");
    ok("importable sites offer Import", bakery?.importable === true);
    ok("multisite rows are refused", network?.importable === false);
    ok(
      "the refusal explains itself in the tooltip",
      /multisite/i.test(network?.tooltip ?? "")
    );

    // 3) Version mismatch warning — legacy-shop is WP 6.2 / PHP 7.4, neither
    //    of which LocalKit has an image for.
    await page.evaluate(() => {
      const table = [...document.querySelectorAll("table")].find((t) =>
        t.textContent.includes("legacy-shop")
      );
      const row = [...table.querySelectorAll("tbody tr")].find((tr) =>
        tr.textContent.includes("legacy-shop")
      );
      row.querySelector("button").click();
    });
    await sleep(600);

    text = await bodyText();
    ok("the import dialog opens", text.includes("Import “legacy-shop”"));
    ok("it reports the WordPress version match", text.includes("6.2 → 6.7"));
    ok("it reports the PHP version match", text.includes("7.4 → 8.3"));
    ok(
      "it warns when there is no exact image",
      text.includes("does not have an exact image match")
    );
    ok("it promises not to touch the remote", text.includes("remote site is not modified"));

    // Cancel — then import a site that matches exactly, so the warning's
    // absence is also verified.
    await clickByText("button", "Cancel");
    await sleep(400);
    ok("cancel closes the dialog", !(await bodyText()).includes("Import “legacy-shop”"));

    // 4) Import pixel-bakery (WP 6.7 / PHP 8.3 — both exact) under a new name.
    await page.evaluate(() => {
      const table = [...document.querySelectorAll("table")].find((t) =>
        t.textContent.includes("pixel-bakery")
      );
      const row = [...table.querySelectorAll("tbody tr")].find((tr) =>
        tr.textContent.includes("pixel-bakery")
      );
      row.querySelector("button").click();
    });
    await sleep(600);

    text = await bodyText();
    ok("exact version matches raise no warning", !text.includes("does not have an exact image match"));
    ok("the name defaults to the remote site's", await page.evaluate(() => {
      const input = [...document.querySelectorAll("input")].find(
        (i) => i.value === "pixel-bakery"
      );
      return Boolean(input);
    }));

    await page.evaluate(() => {
      const input = [...document.querySelectorAll("input")].find((i) => i.value === "pixel-bakery");
      const setter = Object.getOwnPropertyDescriptor(
        window.HTMLInputElement.prototype,
        "value"
      ).set;
      setter.call(input, "Bakery Copy");
      input.dispatchEvent(new Event("input", { bubbles: true }));
    });
    await sleep(300);
    await clickByText("button", "Import site");

    // 5) Progress streams the backend's stages, then lands on the new site.
    await page.waitForFunction(
      () => document.body.innerText.includes("Downloading remote wp-content"),
      { timeout: 15_000 }
    );
    ok("progress reports the code download stage", true);
    await page.waitForFunction(
      () => document.body.innerText.includes("Rewriting URLs remote -> local"),
      { timeout: 20_000 }
    );
    ok("progress reports the URL rewrite stage", true);
    await page.waitForFunction(
      () => document.body.innerText.includes("Bakery Copy imported from Production"),
      { timeout: 20_000 }
    );
    ok("the import resolves with a success message", true);

    // 6) Back on the dashboard, the new site carries its origin.
    await clickByText("button", "Back to sites").catch(() => {});
    await page.evaluate(() => {
      const link = [...document.querySelectorAll("button, a")].find(
        (b) => b.textContent.trim() === "Sites"
      );
      link?.click();
    });
    await sleep(900);

    const dash = await page.evaluate(() => {
      const cards = [...document.querySelectorAll("div")].filter((d) =>
        d.textContent.includes("Bakery Copy")
      );
      const card = cards[cards.length - 1];
      return {
        present: Boolean(card),
        badge: Boolean(
          [...document.querySelectorAll("span")].find(
            (s) =>
              s.title?.includes("Imported from Production") &&
              s.textContent.includes("Production")
          )
        ),
      };
    });
    ok("the imported site appears on the dashboard", dash.present);
    ok("it carries the imported-from badge", dash.badge);

    // 7) Re-importing the same remote site is refused.
    await openServerKitSettings(page);
    await clickByText("button", "View WP sites");
    await page.waitForFunction(() => document.body.innerText.includes("pixel-bakery"), {
      timeout: 15_000,
    });
    await sleep(500);
    await page.evaluate(() => {
      const table = [...document.querySelectorAll("table")].find((t) =>
        t.textContent.includes("pixel-bakery")
      );
      const row = [...table.querySelectorAll("tbody tr")].find((tr) =>
        tr.textContent.includes("pixel-bakery")
      );
      row.querySelector("button").click();
    });
    await sleep(500);
    await clickByText("button", "Import site");
    await sleep(1200);
    ok(
      "a second import of the same remote site is refused",
      (await bodyText()).includes("already imported")
    );

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
