import { useState } from "react";
import { useDialog } from "../hooks/useDialog";

/**
 * Delete confirmation for a site (plan 17).
 *
 * Replaces the old `window.confirm` because deletion now has a choice in it:
 * a `pre_delete` snapshot is kept by default, and the checkbox is the only way
 * to actually get rid of the data. The copy leads with the safety net so
 * deleting stops feeling like a one-way door.
 */
export default function DeleteSiteDialog({
  siteName,
  busy,
  onClose,
  onConfirm,
}: {
  siteName: string;
  busy: boolean;
  onClose: () => void;
  onConfirm: (deleteSnapshots: boolean) => void;
}) {
  const { overlayProps, panelProps } = useDialog(onClose);
  const [deleteSnapshots, setDeleteSnapshots] = useState(false);

  return (
    <div
      {...overlayProps}
      className="fixed inset-0 z-40 flex items-center justify-center bg-black/60"
    >
      <div
        {...panelProps}
        role="dialog"
        aria-modal="true"
        aria-label={`Delete ${siteName}`}
        className="w-full max-w-md rounded-xl border border-zinc-800 bg-zinc-900 p-6 shadow-2xl"
      >
        <h2 className="text-lg font-semibold text-zinc-50">Delete “{siteName}”?</h2>
        <p className="mt-2 text-sm text-zinc-400">
          This removes its containers, database volume and files. A restorable snapshot will be
          kept.
        </p>

        <label className="mt-5 flex items-start gap-2.5 text-sm text-zinc-300">
          <input
            type="checkbox"
            checked={deleteSnapshots}
            onChange={(e) => setDeleteSnapshots(e.target.checked)}
            className="mt-0.5 accent-red-500"
          />
          <span>
            Also delete this site's snapshots
            <span className="mt-0.5 block text-xs text-zinc-600">
              Frees the disk space, but nothing about this site will be recoverable.
            </span>
          </span>
        </label>

        <div className="mt-6 flex justify-end gap-2">
          <button
            onClick={onClose}
            disabled={busy}
            className="rounded-md px-4 py-2 text-sm text-zinc-400 hover:text-zinc-200 disabled:opacity-50"
          >
            Cancel
          </button>
          <button
            onClick={() => onConfirm(deleteSnapshots)}
            disabled={busy}
            autoFocus
            className="rounded-md bg-red-600 px-4 py-2 text-sm font-medium text-white hover:bg-red-500 disabled:opacity-50"
          >
            {busy ? "Deleting…" : deleteSnapshots ? "Delete everything" : "Delete site"}
          </button>
        </div>
      </div>
    </div>
  );
}
