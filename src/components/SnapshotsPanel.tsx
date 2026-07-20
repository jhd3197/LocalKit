import { useCallback, useEffect, useState } from "react";
import { ipc } from "../lib/ipc";
import { toastError } from "../lib/errors";
import { toast } from "../stores/toast";
import { useSites } from "../stores/sites";
import type { Snapshot, SnapshotKind } from "../lib/types";

/**
 * Snapshots panel on the site detail page (plan 17).
 *
 * Snapshots are the undo button for everything destructive: push, pull and
 * delete each leave one behind automatically, and restoring takes one first.
 * The table is therefore also a history — the kind badge says *why* each one
 * exists.
 */

/** Human label + badge colour per kind. Violet = user action, zinc = automatic. */
const KIND_META: Record<SnapshotKind, { label: string; className: string }> = {
  manual: { label: "Manual", className: "bg-violet-500/15 text-violet-300" },
  pre_push: { label: "Before push", className: "bg-zinc-700/50 text-zinc-300" },
  pre_pull: { label: "Before pull", className: "bg-zinc-700/50 text-zinc-300" },
  pre_delete: { label: "Before delete", className: "bg-zinc-700/50 text-zinc-300" },
  pre_restore: { label: "Before restore", className: "bg-zinc-700/50 text-zinc-300" },
};

export function formatBytes(bytes: number): string {
  const units = ["B", "KB", "MB", "GB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return unit === 0 ? `${bytes} B` : `${value.toFixed(1)} ${units[unit]}`;
}

export default function SnapshotsPanel({ siteId }: { siteId: string }) {
  const progress = useSites((s) => s.progress);

  const [snapshots, setSnapshots] = useState<Snapshot[]>([]);
  const [note, setNote] = useState("");
  const [busy, setBusy] = useState<string | null>(null);

  const refresh = useCallback(() => {
    ipc
      .listSnapshots(siteId)
      .then(setSnapshots)
      .catch(() => setSnapshots([]));
  }, [siteId]);

  useEffect(refresh, [refresh]);

  // Push/pull/delete take snapshots of their own — pick them up when the
  // operation that created them finishes.
  useEffect(() => {
    if (progress && (progress.stage === "done" || progress.stage === "error")) {
      refresh();
      setBusy(null);
    }
  }, [progress, refresh]);

  const create = async () => {
    setBusy("create");
    try {
      await ipc.createSnapshot(siteId, note.trim() || undefined);
      setNote("");
      refresh();
    } catch (e) {
      // The site-event stream already toasts the failure; this catches
      // rejections it didn't (deduped inside toastError).
      toastError(e, "Create snapshot");
    } finally {
      setBusy(null);
    }
  };

  const restore = async (snap: Snapshot) => {
    const when = new Date(snap.created_at).toLocaleString();
    const confirmed = window.confirm(
      `Restore this site to the snapshot from ${when}?\n\n` +
        "Its database and wp-content will be replaced. A snapshot of the current " +
        "state is taken first, so this is reversible."
    );
    if (!confirmed) return;
    setBusy(snap.id);
    try {
      await ipc.restoreSnapshot(siteId, snap.id);
      refresh();
    } catch (e) {
      toastError(e, "Restore snapshot");
    } finally {
      setBusy(null);
    }
  };

  const remove = async (snap: Snapshot) => {
    const when = new Date(snap.created_at).toLocaleString();
    if (!window.confirm(`Delete the snapshot from ${when}? This cannot be undone.`)) return;
    setBusy(snap.id);
    try {
      await ipc.deleteSnapshot(siteId, snap.id);
      toast.success("Snapshot deleted", when);
      refresh();
    } catch (e) {
      toastError(e, "Delete snapshot");
    } finally {
      setBusy(null);
    }
  };

  return (
    <section className="rounded-xl border border-zinc-800 bg-zinc-900/60 p-5">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <h2 className="text-sm font-semibold uppercase tracking-wide text-zinc-500">Snapshots</h2>
        <div className="flex items-center gap-2">
          <input
            value={note}
            onChange={(e) => setNote(e.target.value)}
            placeholder="Note (optional)…"
            className="w-56 rounded-md border border-zinc-700 bg-zinc-950 px-3 py-1.5 text-sm text-zinc-100 outline-none focus:border-violet-600"
          />
          <button
            onClick={() => void create()}
            disabled={busy !== null}
            className="rounded-md bg-violet-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-violet-500 disabled:opacity-50"
          >
            {busy === "create" ? "Taking…" : "Take snapshot"}
          </button>
        </div>
      </div>

      <p className="mt-2 text-xs text-zinc-600">
        A copy of the database and <code className="font-mono">wp-content</code>. One is taken
        automatically before every push, pull, delete and restore.
      </p>

      {snapshots.length === 0 ? (
        <p className="mt-4 text-sm text-zinc-600">
          No snapshots yet — take one before you try something risky.
        </p>
      ) : (
        <table className="mt-4 w-full text-sm">
          <thead>
            <tr className="text-left text-xs text-zinc-600">
              <th className="pb-1 font-medium">When</th>
              <th className="pb-1 font-medium">Kind</th>
              <th className="pb-1 font-medium">Size</th>
              <th className="pb-1 font-medium">Note</th>
              <th className="pb-1" />
            </tr>
          </thead>
          <tbody>
            {snapshots.map((s) => {
              const meta = KIND_META[s.kind] ?? {
                label: s.kind,
                className: "bg-zinc-700/50 text-zinc-300",
              };
              return (
                <tr key={s.id} className="border-t border-zinc-800/60">
                  <td className="py-2 pr-2 text-xs text-zinc-400">
                    {new Date(s.created_at).toLocaleString()}
                  </td>
                  <td className="py-2 pr-2">
                    <span className={`rounded px-1.5 py-0.5 text-xs ${meta.className}`}>
                      {meta.label}
                    </span>
                  </td>
                  <td
                    className="py-2 pr-2 text-xs text-zinc-500"
                    title={`database ${formatBytes(s.db_bytes)} + wp-content ${formatBytes(s.code_bytes)}`}
                  >
                    {formatBytes(s.db_bytes + s.code_bytes)}
                  </td>
                  <td className="max-w-xs truncate py-2 pr-2 text-xs text-zinc-500" title={s.note}>
                    {s.note || "—"}
                  </td>
                  <td className="py-2 text-right">
                    <button
                      onClick={() => void restore(s)}
                      disabled={busy !== null}
                      className="rounded-md border border-zinc-700 px-2.5 py-1 text-xs text-zinc-200 hover:border-zinc-500 disabled:opacity-50"
                    >
                      {busy === s.id ? "Working…" : "Restore"}
                    </button>
                    <button
                      onClick={() => void remove(s)}
                      disabled={busy !== null}
                      className="ml-2 rounded-md border border-red-900 px-2.5 py-1 text-xs text-red-400 hover:border-red-700 disabled:opacity-50"
                    >
                      Delete
                    </button>
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      )}
    </section>
  );
}
