// Headless runtime verification for plan 17 (snapshots & one-click restore).
//
// Spins up the mock Vite build (no Tauri runtime, no Docker) and walks the
// snapshot UX: the panel lists existing snapshots with their kind badges,
// taking one with a note prepends it, restoring confirms and reports back,
// deleting removes it, a DB pull leaves a `pre_pull` snapshot behind, and the
// delete-site dialog leads with the kept snapshot while offering the opt-out.
//
//   node scripts/verify-snapshots.mjs
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
    // window.confirm blocks headless; auto-accept so Restore/Delete proceed.
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
    /** Snapshot rows as [when, kind, size, note] tuples. */
    const rows = () =>
      page.evaluate(() => {
        const heading = [...document.querySelectorAll("h2")].find(
          (h) => h.textContent.trim() === "Snapshots"
        );
        const table = heading?.closest("section")?.querySelector("table");
        if (!table) return [];
        return [...table.querySelectorAll("tbody tr")].map((tr) =>
          [...tr.querySelectorAll("td")].map((td) => td.innerText.trim())
        );
      });

    await page.waitForFunction(() => document.body.innerText.includes("Pixel Bakery"));
    console.log("› dashboard loaded");

    // Open Pixel Bakery's detail page (it has seeded snapshots).
    await page.evaluate(() => {
      const card = [...document.querySelectorAll("div")].find(
        (d) =>
          d.textContent.includes("Pixel Bakery") &&
          [...d.querySelectorAll("button")].some((b) => b.textContent.trim() === "Details")
      );
      [...card.querySelectorAll("button")].find((b) => b.textContent.trim() === "Details").click();
    });
    await sleep(900);

    let text = await bodyText();
    ok("navigated to SiteDetail", text.includes("Back to sites"));
    // Panel headings are CSS-uppercased, and innerText applies text-transform.
    ok("Snapshots panel is present", /snapshots/i.test(text));

    // 1) Existing snapshots list with human labels, not raw kinds.
    let table = await rows();
    ok("seeded snapshots are listed", table.length === 3);
    ok(
      "kinds render as readable badges",
      table.some((r) => r[1] === "Manual") &&
        table.some((r) => r[1] === "Before pull") &&
        table.some((r) => r[1] === "Before push")
    );
    ok(
      "sizes are human-readable",
      table.every((r) => /^\d+(\.\d+)? (B|KB|MB|GB)$/.test(r[2]))
    );
    ok("notes are shown", table.some((r) => r[3].includes("before the checkout rewrite")));
    ok("newest is first", table[0][1] === "Before pull");

    // 2) Take a snapshot with a note.
    await page.evaluate(() => {
      const input = [...document.querySelectorAll("input")].find(
        (i) => i.placeholder && i.placeholder.startsWith("Note")
      );
      const setter = Object.getOwnPropertyDescriptor(
        window.HTMLInputElement.prototype,
        "value"
      ).set;
      setter.call(input, "verification run");
      input.dispatchEvent(new Event("input", { bubbles: true }));
    });
    await clickByText("button", "Take snapshot");
    await sleep(900);

    table = await rows();
    ok("taking a snapshot prepends a row", table.length === 4);
    ok("the new snapshot is Manual", table[0][1] === "Manual");
    ok("the note is stored", table[0][3] === "verification run");
    ok("success is toasted", (await bodyText()).includes("Snapshot of Pixel Bakery taken"));

    // 3) Restore the newest snapshot — confirms, then snapshots first.
    await page.evaluate(() => {
      const heading = [...document.querySelectorAll("h2")].find(
        (h) => h.textContent.trim() === "Snapshots"
      );
      const row = heading.closest("section").querySelector("tbody tr");
      [...row.querySelectorAll("button")].find((b) => b.textContent.trim() === "Restore").click();
    });
    await sleep(3200);

    text = await bodyText();
    table = await rows();
    ok("restore reports back", text.includes("restored to the snapshot from"));
    ok("restore snapshots the current state first", table.length === 5);
    ok("that snapshot is labelled Before restore", table[0][1] === "Before restore");

    // 4) Delete a snapshot.
    const before = table.length;
    await page.evaluate(() => {
      const heading = [...document.querySelectorAll("h2")].find(
        (h) => h.textContent.trim() === "Snapshots"
      );
      const row = heading.closest("section").querySelector("tbody tr");
      [...row.querySelectorAll("button")].find((b) => b.textContent.trim() === "Delete").click();
    });
    await sleep(900);
    ok("deleting a snapshot removes its row", (await rows()).length === before - 1);
    ok("deletion is toasted", (await bodyText()).includes("Snapshot deleted"));

    // 5) A DB pull must leave a pre_pull snapshot behind (plan 17 phase 2).
    const countBeforePull = (await rows()).length;
    await page.select("select", "conn-prod").catch(() => {});
    await page.evaluate(() => {
      const selects = [...document.querySelectorAll("select")];
      const set = (el, value) => {
        const setter = Object.getOwnPropertyDescriptor(
          window.HTMLSelectElement.prototype,
          "value"
        ).set;
        setter.call(el, value);
        el.dispatchEvent(new Event("change", { bubbles: true }));
      };
      const conn = selects.find((s) => s.innerHTML.includes("Production"));
      if (conn) set(conn, "conn-prod");
    });
    await sleep(700);
    await page.evaluate(() => {
      const selects = [...document.querySelectorAll("select")];
      const remote = selects.find((s) => s.innerHTML.includes("pixel-bakery"));
      if (remote) {
        const setter = Object.getOwnPropertyDescriptor(
          window.HTMLSelectElement.prototype,
          "value"
        ).set;
        setter.call(remote, "27");
        remote.dispatchEvent(new Event("change", { bubbles: true }));
      }
    });
    await sleep(400);
    await clickByText("button", "Pull DB");
    await sleep(2200);

    table = await rows();
    ok("a DB pull leaves a snapshot behind", table.length === countBeforePull + 1);
    ok("it is labelled Before pull", table[0][1] === "Before pull");
    ok("it names the connection it pulled from", table[0][3].includes("Production"));

    // 6) Delete-site dialog: leads with the kept snapshot, offers the opt-out.
    await clickByText("button", "Delete");
    await sleep(500);
    text = await bodyText();
    ok("delete dialog promises a snapshot", text.includes("A restorable snapshot will be kept"));
    ok("delete dialog offers the opt-out", text.includes("Also delete this site's snapshots"));
    ok(
      "default action keeps the snapshots",
      await page.evaluate(() =>
        [...document.querySelectorAll("button")].some((b) => b.textContent.trim() === "Delete site")
      )
    );
    // Ticking the box escalates the button copy — the destructive path reads
    // differently from the safe one.
    await page.evaluate(() => {
      const box = [...document.querySelectorAll('input[type="checkbox"]')].find((c) =>
        c.closest("label")?.textContent.includes("Also delete")
      );
      box.click();
    });
    await sleep(300);
    ok(
      "opting out escalates the confirm button",
      await page.evaluate(() =>
        [...document.querySelectorAll("button")].some(
          (b) => b.textContent.trim() === "Delete everything"
        )
      )
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
