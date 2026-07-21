import { useState } from "react";
import { ipc } from "../lib/ipc";
import { toastError } from "../lib/errors";
import type { SearchReplaceResult } from "../lib/types";

/**
 * Tools → Search & Replace (plan 24).
 *
 * A serialization-safe `wp search-replace` across all tables — never a raw SQL
 * REPLACE, so PHP-serialized values (widget data, options) survive. The flow is
 * dry-run first: preview the per-column change counts, then an explicit Apply
 * that takes a `pre_search_replace` snapshot before writing (the undo button).
 */
export default function SearchReplacePanel({
  siteId,
  running,
  onShowSnapshots,
}: {
  siteId: string;
  running: boolean;
  onShowSnapshots?: () => void;
}) {
  const [from, setFrom] = useState("");
  const [to, setTo] = useState("");
  const [preview, setPreview] = useState<SearchReplaceResult | null>(null);
  const [applied, setApplied] = useState<SearchReplaceResult | null>(null);
  const [busy, setBusy] = useState<"preview" | "apply" | null>(null);

  // Any edit invalidates a stale preview — you never Apply a count you can no
  // longer see on screen.
  const editFrom = (v: string) => {
    setFrom(v);
    setPreview(null);
    setApplied(null);
  };
  const editTo = (v: string) => {
    setTo(v);
    setPreview(null);
    setApplied(null);
  };

  const runPreview = async () => {
    if (!from) return;
    setBusy("preview");
    setApplied(null);
    try {
      setPreview(await ipc.siteSearchReplace(siteId, from, to, true));
    } catch (e) {
      setPreview(null);
      toastError(e, "Preview search-replace");
    } finally {
      setBusy(null);
    }
  };

  const apply = async () => {
    if (!preview) return;
    const confirmed = window.confirm(
      `Replace "${from}" with "${to}" across all tables?\n\n` +
        `${preview.total} occurrence(s) will change. A snapshot is taken first, ` +
        "so this is reversible."
    );
    if (!confirmed) return;
    setBusy("apply");
    try {
      const result = await ipc.siteSearchReplace(siteId, from, to, false);
      setApplied(result);
      setPreview(null);
    } catch (e) {
      // The site-event stream already toasts the failure; dedupes inside.
      toastError(e, "Apply search-replace");
    } finally {
      setBusy(null);
    }
  };

  const result = preview ?? applied;

  return (
    <section className="rounded-xl border border-zinc-800 bg-zinc-900/60 p-5">
      <h2 className="text-sm font-semibold uppercase tracking-wide text-zinc-500">Search &amp; Replace</h2>
      <p className="mt-2 text-xs text-zinc-600">
        A serialization-safe replace across every table (<code className="font-mono">wp search-replace</code>) — safe for
        widget and option data a raw SQL replace would corrupt. Preview first, then apply.
      </p>

      <div className="mt-4 grid grid-cols-1 gap-3 sm:grid-cols-2">
        <label className="block">
          <span className="text-xs text-zinc-500">Replace this</span>
          <input
            value={from}
            onChange={(e) => editFrom(e.target.value)}
            placeholder="https://old.test"
            className="mt-1 w-full rounded-md border border-zinc-700 bg-zinc-950 px-3 py-1.5 font-mono text-sm text-zinc-100 outline-none focus:border-violet-600"
          />
        </label>
        <label className="block">
          <span className="text-xs text-zinc-500">With this</span>
          <input
            value={to}
            onChange={(e) => editTo(e.target.value)}
            placeholder="https://new.test"
            className="mt-1 w-full rounded-md border border-zinc-700 bg-zinc-950 px-3 py-1.5 font-mono text-sm text-zinc-100 outline-none focus:border-violet-600"
          />
        </label>
      </div>

      <div className="mt-3 flex flex-wrap items-center gap-2">
        <button
          onClick={() => void runPreview()}
          disabled={!from || busy !== null}
          className="rounded-md border border-zinc-700 px-3 py-1.5 text-xs font-medium text-zinc-200 hover:border-zinc-500 disabled:opacity-50"
        >
          {busy === "preview" ? "Previewing…" : "Preview changes"}
        </button>
        {preview && (
          <button
            onClick={() => void apply()}
            disabled={busy !== null || preview.total === 0}
            title={preview.total === 0 ? "Nothing to replace" : undefined}
            className="rounded-md bg-violet-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-violet-500 disabled:opacity-50"
          >
            {busy === "apply" ? "Applying…" : `Apply — ${preview.total} change${preview.total === 1 ? "" : "s"}`}
          </button>
        )}
      </div>

      {!running && (
        <p className="mt-3 text-xs text-zinc-600">
          The site is stopped — running this will briefly start the database to make the change.
        </p>
      )}

      {result && (
        <div className="mt-4">
          {applied && (
            <p className="text-sm text-emerald-400">
              Replaced {applied.total} occurrence{applied.total === 1 ? "" : "s"}.{" "}
              {onShowSnapshots && (
                <button onClick={onShowSnapshots} className="underline hover:text-emerald-300">
                  A snapshot was taken first — view snapshots
                </button>
              )}
            </p>
          )}
          {preview && (
            <p className="text-sm text-zinc-300">
              {preview.total === 0
                ? "No occurrences found — nothing would change."
                : `${preview.total} occurrence${preview.total === 1 ? "" : "s"} in ${preview.changes.length} column${
                    preview.changes.length === 1 ? "" : "s"
                  } would change:`}
            </p>
          )}
          {result.changes.length > 0 && (
            <table className="mt-3 w-full text-sm">
              <thead>
                <tr className="text-left text-xs text-zinc-600">
                  <th className="pb-1 font-medium">Table</th>
                  <th className="pb-1 font-medium">Column</th>
                  <th className="pb-1 font-medium text-right">Changes</th>
                </tr>
              </thead>
              <tbody>
                {result.changes.map((c) => (
                  <tr key={`${c.table}.${c.column}`} className="border-t border-zinc-800/60">
                    <td className="py-1 font-mono text-zinc-300">{c.table}</td>
                    <td className="py-1 font-mono text-zinc-400">{c.column}</td>
                    <td className="py-1 text-right text-zinc-300">{c.count}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </div>
      )}
    </section>
  );
}
