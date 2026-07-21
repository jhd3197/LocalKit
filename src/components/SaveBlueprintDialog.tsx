import { useState } from "react";
import { useBlueprints } from "../stores/blueprints";
import { useDialog } from "../hooks/useDialog";

/**
 * "Save as blueprint" dialog (plan 20). Captures a name + optional description;
 * the backend snapshots the site, copies the artifacts into the blueprint and
 * records its plugin/theme list, streaming progress through the pinned toast.
 */
export default function SaveBlueprintDialog({
  source,
  onClose,
}: {
  source: { id: string; name: string };
  onClose: () => void;
}) {
  const save = useBlueprints((s) => s.save);
  const { overlayProps, panelProps } = useDialog(onClose);

  const [name, setName] = useState(`${source.name} blueprint`);
  const [description, setDescription] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const submit = async () => {
    setBusy(true);
    setError(null);
    try {
      await save(source.id, name, description || undefined);
    } catch (e) {
      setError(typeof e === "string" ? e : String(e));
      setBusy(false);
      return;
    }
    onClose();
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
        aria-label="Save as blueprint"
        className="w-full max-w-md rounded-xl border border-zinc-800 bg-zinc-900 p-6 shadow-2xl"
      >
        <h2 className="text-lg font-semibold text-white">Save “{source.name}” as a blueprint</h2>
        <p className="mt-1 text-sm text-zinc-500">
          Captures this site's database, files and plugin list as a reusable template. New sites can
          be created from it with one click.
        </p>

        <div className="mt-5 flex flex-col gap-4">
          <label className="block">
            <span className="mb-1 block text-sm text-zinc-400">Blueprint name</span>
            <input
              value={name}
              onChange={(e) => setName(e.target.value)}
              autoFocus
              className="w-full rounded-md border border-zinc-700 bg-zinc-950 px-3 py-2 text-sm text-zinc-100 outline-none focus:border-violet-600"
            />
          </label>
          <label className="block">
            <span className="mb-1 block text-sm text-zinc-400">Description (optional)</span>
            <textarea
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              rows={2}
              placeholder="What's in this starter stack?"
              className="w-full resize-none rounded-md border border-zinc-700 bg-zinc-950 px-3 py-2 text-sm text-zinc-100 outline-none focus:border-violet-600"
            />
          </label>
          {error && <p className="text-sm text-red-400">{error}</p>}
        </div>

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
            {busy ? "Saving…" : "Save blueprint"}
          </button>
        </div>
      </div>
    </div>
  );
}
