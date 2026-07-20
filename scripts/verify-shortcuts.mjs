// Headless runtime verification for plan 15 (command palette + shortcuts).
//
// Spins up the mock Vite build (no Tauri runtime, no Docker) and drives the
// keyboard system end-to-end: palette open/filter/run, global shortcuts, the
// editable-target guard, rebinding with conflict detection, persistence.
//
//   node scripts/verify-shortcuts.mjs
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
      defaultViewport: { width: 1440, height: 900 },
    });
    const page = await browser.newPage();
    page.on("pageerror", (e) => console.warn("  page error:", e.message));
    await page.goto(URL, { waitUntil: "networkidle0" });

    const bodyText = () => page.evaluate(() => document.body.innerText);
    const waitForText = (text, timeout = 15_000) =>
      page.waitForFunction(
        (t) => document.body.innerText.toLowerCase().includes(t.toLowerCase()),
        { timeout },
        text
      );
    const dialogOpen = (label) =>
      page.evaluate(
        (l) => !!document.querySelector(`[role="dialog"][aria-label="${l}"]`),
        label
      );

    await waitForText("Pixel Bakery");
    console.log("› dashboard loaded");

    // 1) mod+K opens the palette; static commands are listed.
    await page.keyboard.down("Control");
    await page.keyboard.press("k");
    await page.keyboard.up("Control");
    await sleep(300);
    ok("mod+K opens palette", await dialogOpen("Command palette"));
    ok("palette lists static commands", (await bodyText()).includes("Go to Sites"));

    // 2) Fuzzy filter finds per-site commands; Enter runs "Open".
    await page.keyboard.type("pixel bakery open");
    await sleep(300);
    ok("fuzzy finds per-site command", (await bodyText()).includes("Pixel Bakery"));
    await page.keyboard.press("Enter");
    await sleep(500);
    ok("Enter runs command (navigates to site)", (await bodyText()).includes("Back to sites"));
    ok("palette closed after run", !(await dialogOpen("Command palette")));

    // 3) Global shortcut mod+1 returns to the dashboard from any page.
    await page.keyboard.down("Control");
    await page.keyboard.press("1");
    await page.keyboard.up("Control");
    await sleep(400);
    ok("mod+1 goes to Sites", (await bodyText()).includes("New Site"));

    // 4) mod+N opens the new-site dialog globally.
    await page.keyboard.down("Control");
    await page.keyboard.press("n");
    await page.keyboard.up("Control");
    await sleep(300);
    ok("mod+N opens new-site dialog", await dialogOpen("New WordPress site"));

    // 5) Editable-target guard: typing "?" in the input must NOT open the
    //    cheat-sheet, and the text must land in the field.
    await page.type('input[placeholder="My Blog"]', "what?");
    await sleep(200);
    ok("'?' does not fire while typing", !(await dialogOpen("Keyboard shortcuts")));
    const typed = await page.$eval('input[placeholder="My Blog"]', (el) => el.value);
    ok("input received the text", typed === "what?");

    // 6) Escape closes the dialog (shared useDialog).
    await page.keyboard.press("Escape");
    await sleep(300);
    ok("Escape closes dialog", !(await dialogOpen("New WordPress site")));

    // 7) "?" opens the cheat-sheet with effective bindings.
    await page.keyboard.press("?");
    await sleep(300);
    ok("? opens cheat-sheet", await dialogOpen("Keyboard shortcuts"));
    ok("cheat-sheet shows bindings", (await bodyText()).includes("Ctrl+K"));
    await page.keyboard.press("Escape");
    await sleep(300);

    // 8) Settings → Keyboard: rebind "Command palette" to ctrl+shift+p.
    await page.keyboard.down("Control");
    await page.keyboard.press(",");
    await page.keyboard.up("Control");
    await sleep(400);
    ok("mod+, opens settings", await dialogOpen("Settings"));
    await page.evaluate(() => {
      [...document.querySelectorAll("nav button")].find((b) => b.textContent.trim() === "Keyboard")?.click();
    });
    await sleep(300);
    ok("Keyboard section renders", (await bodyText()).includes("Reset all to defaults"));

    // Click the binding button on the "Command palette" row, record a combo.
    await page.evaluate(() => {
      const rows = [...document.querySelectorAll("div")].filter(
        (d) => d.firstElementChild?.textContent === "Command palette" && d.querySelector("button")
      );
      const btn = rows[0]?.querySelector("button:last-child");
      btn?.click();
      btn?.focus(); // el.click() doesn't move focus; keydown must reach the button
    });
    await sleep(200);
    ok("capture field is recording", (await bodyText()).includes("Press keys…"));
    await page.keyboard.down("Control");
    await page.keyboard.down("Shift");
    await page.keyboard.press("p");
    await page.keyboard.up("Shift");
    await page.keyboard.up("Control");
    await sleep(300);
    const stored = await page.evaluate(() => localStorage.getItem("localkit.settings.shortcut.toggle-palette"));
    ok("override persisted (shortcut.toggle-palette)", stored === "mod+shift+p");

    // 9) Conflict detection: bind the same combo to "Open settings".
    await page.evaluate(() => {
      const rows = [...document.querySelectorAll("div")].filter(
        (d) => d.firstElementChild?.textContent === "Open settings" && d.querySelector("button")
      );
      const btn = rows[0]?.querySelector("button:last-child");
      btn?.click();
      btn?.focus(); // el.click() doesn't move focus; keydown must reach the button
    });
    await sleep(200);
    await page.keyboard.down("Control");
    await page.keyboard.down("Shift");
    await page.keyboard.press("p");
    await page.keyboard.up("Shift");
    await page.keyboard.up("Control");
    await sleep(300);
    ok("conflict is caught", (await bodyText()).includes("already used by"));
    await page.evaluate(() => {
      [...document.querySelectorAll("button")].find((b) => b.textContent.trim() === "Overwrite")?.click();
    });
    await sleep(300);
    const unbound = await page.evaluate(() => localStorage.getItem("localkit.settings.shortcut.toggle-palette"));
    ok("overwrite unbinds the loser", unbound === "none");

    // 10) Reset all restores defaults.
    await page.evaluate(() => {
      [...document.querySelectorAll("button")].find((b) => b.textContent.trim() === "Reset all to defaults")?.click();
    });
    await sleep(300);
    const cleared = await page.evaluate(() => localStorage.getItem("localkit.settings.shortcut.toggle-palette"));
    ok("reset all clears overrides", cleared === null);

    // 11) New binding still fires after reload (persistence across restarts).
    await page.keyboard.press("Escape"); // close settings
    await sleep(300);
    await page.evaluate(() => localStorage.setItem("localkit.settings.shortcut.toggle-palette", "mod+shift+p"));
    await page.reload({ waitUntil: "networkidle0" });
    await waitForText("Pixel Bakery");
    await page.keyboard.down("Control");
    await page.keyboard.down("Shift");
    await page.keyboard.press("p");
    await page.keyboard.up("Shift");
    await page.keyboard.up("Control");
    await sleep(300);
    ok("rebound shortcut fires after reload", await dialogOpen("Command palette"));

    console.log(failures === 0 ? "› all checks passed" : `› ${failures} check(s) FAILED`);
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
