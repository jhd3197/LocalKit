import { useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ipc } from "../lib/ipc";
import { toastError } from "../lib/errors";
import { toast } from "../stores/toast";

/**
 * Tools → Database (plan 24).
 *
 * One button that starts the profile-gated Adminer sidecar on first use and
 * opens it with the server + username pre-filled. Adminer can't take the
 * password in the URL, so it is copied to the clipboard (with a toast) for the
 * user to paste. The login is the site's `wordpress` DB user.
 */
export default function DatabasePanel({ siteId, running }: { siteId: string; running: boolean }) {
  const [busy, setBusy] = useState(false);

  const open = async () => {
    setBusy(true);
    try {
      const info = await ipc.openSiteDatabase(siteId);
      // Adminer takes the password in its form; stage it on the clipboard.
      let copied = false;
      try {
        await navigator.clipboard.writeText(info.password);
        copied = true;
      } catch {
        // clipboard unavailable; fall through — the toast still shows the login
      }
      toast.success(
        copied ? "Password copied to clipboard" : "Opening Adminer",
        `Log in as “${info.username}” — paste the password into Adminer.`
      );
      await openUrl(info.url);
    } catch (e) {
      toastError(e, "Open database");
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="rounded-xl border border-zinc-800 bg-zinc-900/60 p-5">
      <h2 className="text-sm font-semibold uppercase tracking-wide text-zinc-500">Database</h2>
      <p className="mt-2 text-xs text-zinc-600">
        Browse and edit the database in Adminer, a lightweight web GUI. It starts on first use and
        opens with the server and username pre-filled; the password is copied to your clipboard to
        paste in.
      </p>
      <div className="mt-4">
        <button
          onClick={() => void open()}
          disabled={!running || busy}
          title={running ? "Start Adminer and open it" : "Start the site first"}
          className="rounded-md bg-violet-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-violet-500 disabled:opacity-50"
        >
          {busy ? "Starting Adminer…" : "Open database"}
        </button>
        {!running && <span className="ml-3 text-xs text-zinc-600">Start the site to open the database.</span>}
      </div>
    </section>
  );
}
