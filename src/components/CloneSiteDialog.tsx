import { useState } from "react";
import { useSites } from "../stores/sites";
import { useNav } from "../stores/nav";
import { useDialog } from "../hooks/useDialog";
import type { Site } from "../lib/types";

/**
 * Name-the-copy dialog for a one-click site clone (plan 20). The heavy lifting
 * — snapshot the source, provision a fresh site, lay the data down and rewrite
 * URLs — happens in the backend and streams progress through the pinned toast;
 * this only collects the new name and, on success, opens the clone.
 */
export default function CloneSiteDialog({
  source,
  onClose,
}: {
  source: { id: string; name: string };
  onClose: () => void;
}) {
  const cloneSite = useSites((s) => s.cloneSite);
  const navigate = useNav((s) => s.navigate);
  const { overlayProps, panelProps } = useDialog(onClose);

  const [name, setName] = useState(`${source.name} copy`);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const submit = async () => {
    setBusy(true);
    setError(null);
    let site: Site;
    try {
      site = await cloneSite(source.id, name);
    } catch (e) {
      setError(typeof e === "string" ? e : String(e));
      setBusy(false);
      return;
    }
    onClose();
    navigate({ name: "site", id: site.id });
  };

  return (
    <div
      {...overlayProps}
      className="fixed inset-0 z-40 flex items-center justify-center bg-black/60"
    >
      <div
        {...panelProps}
        role="dialog"
        aria-modal="true"
        aria-label="Clone site"
        className="w-full max-w-md rounded-xl border border-zinc-800 bg-zinc-900 p-6 shadow-2xl"
      >
        <h2 className="text-lg font-semibold text-zinc-50">Clone “{source.name}”</h2>
        <p className="mt-1 text-sm text-zinc-500">
          LocalKit copies this site's database and files into a brand-new site with fresh ports and
          credentials — a throwaway copy to test a plugin or theme change.
        </p>

        <label className="mt-5 block">
          <span className="mb-1 block text-sm text-zinc-400">New site name</span>
          <input
            value={name}
            onChange={(e) => setName(e.target.value)}
            autoFocus
            className="w-full rounded-md border border-zinc-700 bg-zinc-950 px-3 py-2 text-sm text-zinc-100 outline-none focus:border-violet-600"
          />
        </label>

        {error && <p className="mt-3 text-sm text-red-400">{error}</p>}

        <div className="mt-6 flex justify-end gap-2">
          <button
            onClick={onClose}
            disabled={busy}
            className="rounded-md px-4 py-2 text-sm text-zinc-400 hover:text-zinc-200 disabled:opacity-50"
          >
            Cancel
          </button>
          <button
            onClick={submit}
            disabled={busy || !name.trim()}
            className="rounded-md bg-violet-600 px-4 py-2 text-sm font-medium text-white hover:bg-violet-500 disabled:opacity-50"
          >
            {busy ? "Cloning…" : "Clone site"}
          </button>
        </div>
      </div>
    </div>
  );
}
