// Headless runtime verification for plan 19 (chunked sync: byte progress + cancel).
//
// Spins up the mock Vite build (no Tauri runtime, no Docker, no ServerKit) and
// walks the transfer UX a chunked sync is supposed to produce: the progress
// toast counts real bytes instead of sitting on one static line, it offers a
// Cancel button only while bytes are actually moving, cancelling stops the
// transfer and resolves neutrally rather than as a red failure, and the sync
// history records it as `cancelled` rather than `error`.
//
//   node scripts/verify-sync-progress.mjs
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
    await page.goto(URL, { waitUntil: "networkidle0" });

    const bodyText = () => page.evaluate(() => document.body.innerText);
    // Clicking a *disabled* button silently does nothing, which would turn a
    // real regression into a confusing timeout further down. Report it.
    const clickByText = (selector, text) =>
      page.evaluate(
        (sel, t) => {
          const el = [...document.querySelectorAll(sel)].find((e) =>
            e.textContent.trim().toLowerCase().includes(t.toLowerCase())
          );
          if (!el) return "missing";
          if (el.disabled) return "disabled";
          el.click();
          return "clicked";
        },
        selector,
        text
      );
    const click = async (selector, text) => (await clickByText(selector, text)) === "clicked";
    /** Text of the pinned progress toast, or "" when none is up. */
    const toastText = () =>
      page.evaluate(() => {
        const el = document.querySelector(".fixed.bottom-4.right-4 > div");
        return el ? el.innerText.trim() : "";
      });
    /** Sync-history rows as [when, op, result, message] tuples. */
    const historyRows = () =>
      page.evaluate(() => {
        const heading = [...document.querySelectorAll("h3")].find((h) =>
          h.textContent.trim().toLowerCase().startsWith("sync history")
        );
        const table = heading?.parentElement?.querySelector("table");
        if (!table) return [];
        return [...table.querySelectorAll("tbody tr")].map((tr) =>
          [...tr.querySelectorAll("td")].map((td) => td.innerText.trim())
        );
      });
    /** Tailwind classes on the result cell of the newest history row. */
    const newestResultClass = () =>
      page.evaluate(() => {
        const heading = [...document.querySelectorAll("h3")].find((h) =>
          h.textContent.trim().toLowerCase().startsWith("sync history")
        );
        const tr = heading?.parentElement?.querySelector("table tbody tr");
        return tr ? tr.querySelectorAll("td")[2].className : "";
      });

    await page.waitForFunction(() => document.body.innerText.includes("Pixel Bakery"));
    console.log("› dashboard loaded");

    await page.evaluate(() => {
      const card = [...document.querySelectorAll("div")].find(
        (d) =>
          d.textContent.includes("Pixel Bakery") &&
          [...d.querySelectorAll("button")].some((b) => b.textContent.trim() === "Details")
      );
      [...card.querySelectorAll("button")].find((b) => b.textContent.trim() === "Details").click();
    });
    await sleep(900);
    ok("navigated to SiteDetail", (await bodyText()).includes("Back to sites"));

    // Pick a connection + remote site so the push buttons enable.
    await page.evaluate(() => {
      const setValue = (el, value) => {
        const setter = Object.getOwnPropertyDescriptor(
          window.HTMLSelectElement.prototype,
          "value"
        ).set;
        setter.call(el, value);
        el.dispatchEvent(new Event("change", { bubbles: true }));
      };
      const selects = [...document.querySelectorAll("select")];
      for (const sel of selects) {
        const real = [...sel.options].find((o) => o.value && o.value !== "");
        if (real) setValue(sel, real.value);
      }
    });
    await sleep(800);
    // The remote-site select only populates after the connection is chosen.
    await page.evaluate(() => {
      const setter = Object.getOwnPropertyDescriptor(
        window.HTMLSelectElement.prototype,
        "value"
      ).set;
      for (const sel of document.querySelectorAll("select")) {
        const real = [...sel.options].find((o) => o.value && o.value !== "");
        if (real && !sel.value) {
          setter.call(sel, real.value);
          sel.dispatchEvent(new Event("change", { bubbles: true }));
        }
      }
    });
    await sleep(400);

    // --- 1. byte progress -------------------------------------------------
    console.log("› push code (byte progress)");
    ok("Push code is clickable", await click("button", "Push code"));
    await sleep(700);

    let first = await toastText();
    ok("a progress toast appeared", first.length > 0);
    ok(
      "the transfer reports bytes, not just a stage",
      /\d+(\.\d+)?\s?(B|KB|MB|GB)\s*\/\s*\d+(\.\d+)?\s?(B|KB|MB|GB)/.test(first)
    );
    ok("the byte readout names the payload", /wp-content/i.test(first));
    ok("a running transfer offers Cancel", first.includes("Cancel"));

    await sleep(600);
    const second = await toastText();
    ok("the byte count actually advances", second !== first);

    const bytesOf = (t) => {
      const m = /([\d.]+)\s?(B|KB|MB|GB)\s*\/\s*([\d.]+)\s?(B|KB|MB|GB)/.exec(t);
      if (!m) return null;
      const scale = { B: 1, KB: 1024, MB: 1024 ** 2, GB: 1024 ** 3 };
      return [parseFloat(m[1]) * scale[m[2]], parseFloat(m[3]) * scale[m[4]]];
    };
    const a = bytesOf(first);
    const b = bytesOf(second);
    ok("progress moves forward, never backward", a && b && b[0] > a[0]);
    ok("the total stays fixed across updates", a && b && a[1] === b[1]);
    ok("done never exceeds total", b && b[0] <= b[1]);

    // --- 2. cancel --------------------------------------------------------
    console.log("› cancel mid-transfer");
    ok("clicked Cancel", await click("button", "Cancel"));
    await sleep(900);

    const resolved = await toastText();
    ok("the toast resolves on cancel", /cancelled/i.test(resolved));
    ok("the spinner is gone once cancelled", !resolved.includes("Cancel\n"));
    ok(
      "a cancel is not styled as an error",
      await page.evaluate(() => {
        const el = document.querySelector(".fixed.bottom-4.right-4 > div");
        return el ? !el.className.includes("red") : false;
      })
    );

    // The transfer really stopped: no "Pushed code" success follows.
    await sleep(1500);
    ok(
      "a cancelled transfer never completes",
      !/pushed code/i.test(await bodyText())
    );

    // --- 3. history -------------------------------------------------------
    console.log("› sync history");
    const rows = await historyRows();
    ok("the cancel is recorded in sync history", rows.length > 0 && rows[0][2] === "cancelled");
    ok("it is recorded as a push code op", rows.length > 0 && /push\s+code/i.test(rows[0][1]));
    const cls = await newestResultClass();
    ok("cancelled renders neutral, not red", cls.includes("zinc") && !cls.includes("red"));

    // --- 4. a completed transfer still resolves green ---------------------
    console.log("› push db to completion");
    ok("Push DB is clickable", await click("button", "Push DB"));
    // 312 MB in 8 MiB steps at ~80ms/step ≈ 3.2s.
    await page.waitForFunction(
      () => /Pushed db/i.test(document.body.innerText),
      { timeout: 20_000 }
    );
    ok("an uninterrupted transfer completes", /Pushed db/i.test(await bodyText()));
    const after = await historyRows();
    ok("success is recorded", after.some((r) => r[2] === "success"));
    const okCls = await newestResultClass();
    ok("success stays emerald", okCls.includes("emerald"));
  } finally {
    if (browser) await browser.close();
    killTree(server);
  }

  console.log(failures ? `\n${failures} failure(s)` : "\nAll sync-progress checks passed");
  process.exit(failures ? 1 : 0);
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
