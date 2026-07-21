import { useCallback, useEffect, useState } from "react";
import { ipc } from "../lib/ipc";
import { errMsg, toastError } from "../lib/errors";
import { toast } from "../stores/toast";
import { useSites } from "../stores/sites";

/**
 * Tools → Config (plan 24).
 *
 * A plain textarea editor for `wp-config.php` and the site `.env` — no Monaco,
 * no diff/backup machinery (snapshots are the safety net). Saving `.env` offers
 * a restart, since compose only picks env changes up on a recreate; `wp-config`
 * is read live by PHP and needs nothing. Danger styling throughout: editing
 * these can break the site.
 */
type ConfigFile = "wp-config" | "env";

const FILES: { key: ConfigFile; label: string }[] = [
  { key: "wp-config", label: "wp-config.php" },
  { key: "env", label: ".env" },
];

export default function ConfigEditorPanel({ siteId }: { siteId: string }) {
  const restart = useSites((s) => s.restart);
  const [file, setFile] = useState<ConfigFile>("wp-config");
  const [content, setContent] = useState("");
  const [original, setOriginal] = useState("");
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(
    (which: ConfigFile) => {
      setLoading(true);
      setError(null);
      ipc
        .readSiteConfigFile(siteId, which)
        .then((text) => {
          setContent(text);
          setOriginal(text);
        })
        .catch((e) => {
          setError(errMsg(e));
          setContent("");
          setOriginal("");
        })
        .finally(() => setLoading(false));
    },
    [siteId]
  );

  useEffect(() => load(file), [load, file]);

  const dirty = content !== original;

  const save = async () => {
    setSaving(true);
    try {
      await ipc.writeSiteConfigFile(siteId, file, content);
      setOriginal(content);
      toast.success(`Saved ${FILES.find((f) => f.key === file)?.label}`);
      // .env only takes effect on a recreate — offer it.
      if (file === "env" && window.confirm("Restart the site now to apply the .env changes?")) {
        await restart(siteId);
      }
    } catch (e) {
      toastError(e, "Save config file");
    } finally {
      setSaving(false);
    }
  };

  return (
    <section className="rounded-xl border border-red-900/50 bg-zinc-900/60 p-5">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <h2 className="text-sm font-semibold uppercase tracking-wide text-zinc-500">Config</h2>
        <div className="flex gap-1">
          {FILES.map((f) => (
            <button
              key={f.key}
              onClick={() => setFile(f.key)}
              className={`rounded-md px-3 py-1 font-mono text-xs transition-colors ${
                file === f.key
                  ? "bg-zinc-800 text-zinc-100"
                  : "text-zinc-500 hover:text-zinc-300"
              }`}
            >
              {f.label}
            </button>
          ))}
        </div>
      </div>

      <p className="mt-2 text-xs text-red-400/80">
        Editing this can break the site. There is no undo here — take a snapshot first if unsure.
      </p>

      {loading ? (
        <p className="mt-4 text-sm text-zinc-600">Loading…</p>
      ) : error ? (
        <p className="mt-4 text-sm text-red-400">{error}</p>
      ) : (
        <>
          <textarea
            value={content}
            onChange={(e) => setContent(e.target.value)}
            spellCheck={false}
            className="mt-3 h-72 w-full resize-y rounded-lg border border-zinc-800 bg-zinc-950 p-3 font-mono text-xs leading-relaxed text-zinc-200 outline-none focus:border-violet-600"
          />
          <div className="mt-3 flex items-center gap-3">
            <button
              onClick={() => void save()}
              disabled={!dirty || saving}
              className="rounded-md bg-violet-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-violet-500 disabled:opacity-50"
            >
              {saving ? "Saving…" : "Save"}
            </button>
            {dirty && (
              <button
                onClick={() => setContent(original)}
                disabled={saving}
                className="text-xs text-zinc-500 hover:text-zinc-300 disabled:opacity-50"
              >
                Revert
              </button>
            )}
            {file === "env" && (
              <span className="text-xs text-zinc-600">Changing ports or passwords needs a restart.</span>
            )}
          </div>
        </>
      )}
    </section>
  );
}
