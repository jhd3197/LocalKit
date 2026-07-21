// Headless runtime verification for plan 20 (site clone + reusable blueprints).
//
// Spins up the mock Vite build (no Tauri runtime, no Docker) and walks the two
// creation flows: the New Site dialog's "From blueprint" section lists the
// sample blueprints with plugin/theme chips, selecting one switches the dialog
// into create-from mode and stamps a new site out of it; a site's Clone button
// opens the copy under a new name; and "Save as blueprint" from a site records
// a new template that then shows up in the dialog.
//
//   node scripts/verify-blueprints.mjs
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
    // Focus a field and type into it for real (one React onChange per key), so
    // a subsequent submit reliably reads the new value — the direct value-setter
    // trick races the controlled-input re-render.
    const typeInto = async (predicateSrc, value) => {
      const focused = await page.evaluate((src) => {
        // eslint-disable-next-line no-new-func
        const match = new Function("i", `return (${src})(i)`);
        const input = [...document.querySelectorAll("input, textarea")].find(match);
        if (!input) return false;
        input.focus();
        return true;
      }, predicateSrc);
      if (!focused) return false;
      await page.keyboard.down("Control");
      await page.keyboard.press("KeyA");
      await page.keyboard.up("Control");
      await page.keyboard.press("Backspace");
      await page.keyboard.type(value, { delay: 5 });
      return true;
    };

    // Open a specific site's detail page by clicking the Details button inside
    // that site's card (matching by name on the nearest card, not the first
    // Details on the page).
    const openSite = (siteName) =>
      page.evaluate((name) => {
        const btn = [...document.querySelectorAll("button")]
          .filter((b) => b.textContent.trim() === "Details")
          .find((b) => {
            const card = b.closest("div.rounded-xl");
            return card && card.textContent.includes(name);
          });
        if (!btn) return false;
        btn.click();
        return true;
      }, siteName);

    // Click the "Use" button inside a specific blueprint row (matching by name
    // on the nearest row, not the first Use on the page).
    const useBlueprint = (bpName) =>
      page.evaluate((name) => {
        const btn = [...document.querySelectorAll("button")]
          .filter((b) => b.textContent.trim() === "Use")
          .find((b) => {
            const row = b.closest("div.rounded-lg");
            return row && row.textContent.includes(name);
          });
        if (!btn) return false;
        btn.click();
        return true;
      }, bpName);

    await page.waitForFunction(() => document.body.innerText.includes("Pixel Bakery"));
    console.log("› dashboard loaded");

    // --- 1) New Site dialog: the "From blueprint" section ------------------
    await clickByText("button", "New Site");
    await sleep(500);
    let text = await bodyText();
    ok("New Site dialog opens", text.includes("New WordPress site"));
    ok("blueprint section is present", /or start from a blueprint/i.test(text));
    ok("sample blueprints are listed", text.includes("Starter Shop") && text.includes("Agency Base"));
    ok("plugin/theme chips render", text.includes("woocommerce") && text.includes("storefront"));
    ok(
      "blueprint names its source site",
      text.includes("from Pixel Bakery") || text.includes("from Acme Corporate")
    );

    // Select the Starter Shop blueprint (the Use button inside its row).
    ok("a blueprint can be selected", await useBlueprint("Starter Shop"));
    await sleep(300);
    text = await bodyText();
    ok("selection shows the based-on summary", text.includes("Based on") && text.includes("Starter Shop"));
    ok(
      "the create button switches to blueprint mode",
      await page.evaluate(() =>
        [...document.querySelectorAll("button")].some(
          (b) => b.textContent.trim() === "Create from blueprint"
        )
      )
    );
    ok(
      "the name is prefilled from the blueprint",
      await page.evaluate(() => {
        const input = [...document.querySelectorAll("input")].find((i) => i.value === "Starter Shop");
        return !!input;
      })
    );

    // Back to a blank site, then forward again — the mode toggles cleanly.
    await clickByText("button", "Use a blank site");
    await sleep(200);
    ok(
      "can return to a blank site",
      (await bodyText()).match(/or start from a blueprint/i) &&
        (await page.evaluate(() =>
          [...document.querySelectorAll("button")].some((b) => b.textContent.trim() === "Create site")
        ))
    );

    // --- 2) Create a site from a blueprint end to end ----------------------
    ok("selected the Agency Base blueprint", await useBlueprint("Agency Base"));
    await sleep(200);
    await typeInto("(i) => i.value === 'Agency Base'", "Agency Copy");
    await sleep(150);
    await clickByText("button", "Create from blueprint");
    // The staged progress toast should appear as the create runs.
    const sawProgress = await page
      .waitForFunction(
        () =>
          /writing project files|downloading wordpress|starting docker|waiting for wordpress|laying down|rewriting urls|created from blueprint/i.test(
            document.body.innerText
          ),
        { timeout: 4000 }
      )
      .then(() => true)
      .catch(() => false);
    ok("a progress toast tracks the blueprint create", sawProgress);
    await sleep(600);
    text = await bodyText();
    ok("creating from a blueprint navigates to the new site", text.includes("Back to sites"));
    ok("the new site carries the given name", text.includes("Agency Copy"));

    // --- 3) Clone a site under a new name ----------------------------------
    await clickByText("button", "Back to sites");
    await sleep(500);
    // Open Pixel Bakery detail and clone it.
    ok("opened Pixel Bakery", await openSite("Pixel Bakery"));
    await sleep(700);
    await clickByText("button", "Clone");
    await sleep(400);
    text = await bodyText();
    ok("Clone dialog opens", /clone .+pixel bakery/i.test(text));
    ok(
      "clone name defaults to a copy",
      await page.evaluate(() => {
        const input = [...document.querySelectorAll("input")].find((i) =>
          i.value.toLowerCase().includes("copy")
        );
        return !!input;
      })
    );
    await typeInto("(i) => i.value.toLowerCase().includes('copy')", "Bakery Clone");
    await sleep(150);
    await clickByText("button", "Clone site");
    await sleep(900);
    text = await bodyText();
    ok("cloning navigates to the new site", text.includes("Back to sites") && text.includes("Bakery Clone"));

    // --- 4) Save an existing site as a blueprint ---------------------------
    await clickByText("button", "Back to sites");
    await sleep(500);
    ok("opened Hiking Blog", await openSite("Hiking Blog"));
    await sleep(700);
    await clickByText("button", "Save as blueprint");
    await sleep(400);
    text = await bodyText();
    ok(
      "Save-as-blueprint dialog opens",
      text.includes("Hiking Blog") && /as a blueprint/i.test(text)
    );
    await typeInto("(i) => i.value === 'Hiking Blog blueprint'", "Hiking Starter");
    await sleep(150);
    await clickByText("button", "Save blueprint");
    await sleep(1200);
    ok("saving a blueprint is toasted", (await bodyText()).includes("Saved"));

    // The new blueprint shows up in the New Site dialog.
    await clickByText("button", "Back to sites");
    await sleep(400);
    await clickByText("button", "New Site");
    await sleep(500);
    ok("the saved blueprint appears in the dialog", (await bodyText()).includes("Hiking Starter"));

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
