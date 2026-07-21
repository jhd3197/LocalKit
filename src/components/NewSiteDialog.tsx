import { useEffect, useState } from "react";
import { ipc } from "../lib/ipc";
import { useNav } from "../stores/nav";
import { useSites } from "../stores/sites";
import { useBlueprints } from "../stores/blueprints";
import { useDialog } from "../hooks/useDialog";
import type { AppInfo, Blueprint } from "../lib/types";

export default function NewSiteDialog({ onClose }: { onClose: () => void }) {
  const createSite = useSites((s) => s.createSite);
  const blueprints = useBlueprints((s) => s.blueprints);
  const refreshBlueprints = useBlueprints((s) => s.refresh);
  const removeBlueprint = useBlueprints((s) => s.remove);
  const createFromBlueprint = useBlueprints((s) => s.createSite);
  const navigate = useNav((s) => s.navigate);
  const { overlayProps, panelProps } = useDialog(onClose);

  const [name, setName] = useState("");
  const [wpVersion, setWpVersion] = useState("");
  const [phpVersion, setPhpVersion] = useState("");
  const [versions, setVersions] = useState<Pick<AppInfo, "wp_versions" | "php_versions">>({
    wp_versions: ["6.7", "6.6", "6.5"],
    php_versions: ["8.3", "8.2", "8.1"],
  });
  // null = blank site; otherwise the blueprint to stamp from.
  const [blueprint, setBlueprint] = useState<Blueprint | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    ipc
      .appInfo()
      .then((info) => setVersions(info))
      .catch(() => {});
    void refreshBlueprints();
  }, [refreshBlueprints]);

  useEffect(() => {
    setWpVersion((v) => v || versions.wp_versions[0]);
    setPhpVersion((v) => v || versions.php_versions[0]);
  }, [versions]);

  // Selecting a blueprint prefills the name; going back to blank clears it.
  const pickBlueprint = (bp: Blueprint | null) => {
    setBlueprint(bp);
    setError(null);
    setName(bp ? bp.name : "");
  };

  const submit = async () => {
    setBusy(true);
    setError(null);
    try {
      const site = blueprint
        ? await createFromBlueprint(blueprint.id, name)
        : await createSite(name, wpVersion, phpVersion);
      onClose();
      navigate({ name: "site", id: site.id });
    } catch (e) {
      setError(typeof e === "string" ? e : String(e));
      setBusy(false);
    }
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
        aria-label="New WordPress site"
        className="flex max-h-[85vh] w-full max-w-md flex-col rounded-xl border border-zinc-800 bg-zinc-900 p-6 shadow-2xl"
      >
        <h2 className="text-lg font-semibold text-white">New WordPress site</h2>
        <p className="mt-1 text-sm text-zinc-500">
          {blueprint
            ? "LocalKit will stamp out a new site from this blueprint — its database, files, plugins and theme."
            : "LocalKit will create a Docker project, start it, and install WordPress automatically."}
        </p>

        <div className="mt-5 flex min-h-0 flex-col gap-4 overflow-y-auto">
          <label className="block">
            <span className="mb-1 block text-sm text-zinc-400">Site name</span>
            <input
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="My Blog"
              autoFocus
              className="w-full rounded-md border border-zinc-700 bg-zinc-950 px-3 py-2 text-sm text-zinc-100 outline-none focus:border-violet-600"
            />
          </label>

          {blueprint ? (
            <BlueprintSummary blueprint={blueprint} onClear={() => pickBlueprint(null)} />
          ) : (
            <div className="grid grid-cols-2 gap-4">
              <label className="block">
                <span className="mb-1 block text-sm text-zinc-400">WordPress</span>
                <select
                  value={wpVersion}
                  onChange={(e) => setWpVersion(e.target.value)}
                  className="w-full rounded-md border border-zinc-700 bg-zinc-950 px-3 py-2 text-sm text-zinc-100 outline-none focus:border-violet-600"
                >
                  {versions.wp_versions.map((v) => (
                    <option key={v} value={v}>
                      {v}
                    </option>
                  ))}
                </select>
              </label>
              <label className="block">
                <span className="mb-1 block text-sm text-zinc-400">PHP</span>
                <select
                  value={phpVersion}
                  onChange={(e) => setPhpVersion(e.target.value)}
                  className="w-full rounded-md border border-zinc-700 bg-zinc-950 px-3 py-2 text-sm text-zinc-100 outline-none focus:border-violet-600"
                >
                  {versions.php_versions.map((v) => (
                    <option key={v} value={v}>
                      {v}
                    </option>
                  ))}
                </select>
              </label>
            </div>
          )}

          {!blueprint && blueprints.length > 0 && (
            <div>
              <p className="mb-2 text-xs font-semibold uppercase tracking-wide text-zinc-500">
                Or start from a blueprint
              </p>
              <div className="flex flex-col gap-2">
                {blueprints.map((bp) => (
                  <BlueprintRow
                    key={bp.id}
                    blueprint={bp}
                    onUse={() => pickBlueprint(bp)}
                    onDelete={() => void removeBlueprint(bp.id)}
                  />
                ))}
              </div>
            </div>
          )}

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
            {busy ? "Creating…" : blueprint ? "Create from blueprint" : "Create site"}
          </button>
        </div>
      </div>
    </div>
  );
}

/**
 * A generated initial-letter tile standing in for a blueprint thumbnail
 * (plan 20 phase 3 — screenshots are dev-only, so v1 is a letter tile). The
 * hue is derived from the name so a blueprint keeps a stable colour.
 */
function LetterTile({ name }: { name: string }) {
  let hash = 0;
  for (let i = 0; i < name.length; i++) hash = (hash * 31 + name.charCodeAt(i)) | 0;
  const hue = Math.abs(hash) % 360;
  return (
    <div
      aria-hidden
      className="flex h-9 w-9 shrink-0 items-center justify-center rounded-md text-sm font-semibold text-white"
      style={{ backgroundColor: `hsl(${hue} 42% 42%)` }}
    >
      {name.trim().charAt(0).toUpperCase() || "?"}
    </div>
  );
}

/** Theme + plugin chips for a blueprint (active plugins first, then a "+N"). */
function BlueprintChips({ blueprint }: { blueprint: Blueprint }) {
  const active = blueprint.plugins.filter((p) => p.status === "active");
  const shown = active.slice(0, 3);
  const extra = blueprint.plugins.length - shown.length;
  const chip = "rounded-full border border-zinc-700 bg-zinc-800/60 px-2 py-0.5 text-[11px] text-zinc-300";
  return (
    <div className="flex flex-wrap gap-1.5">
      {blueprint.theme && <span className={`${chip} text-violet-300`}>🎨 {blueprint.theme}</span>}
      {shown.map((p) => (
        <span key={p.name} className={chip}>
          {p.name}
        </span>
      ))}
      {extra > 0 && <span className={chip}>+{extra} more</span>}
    </div>
  );
}

/** Selectable blueprint row in the blank-site view. */
function BlueprintRow({
  blueprint,
  onUse,
  onDelete,
}: {
  blueprint: Blueprint;
  onUse: () => void;
  onDelete: () => void;
}) {
  return (
    <div className="rounded-lg border border-zinc-800 bg-zinc-950/60 p-3 transition-colors hover:border-zinc-700">
      <div className="flex items-start justify-between gap-2">
        <button onClick={onUse} className="flex min-w-0 flex-1 items-center gap-3 text-left">
          <LetterTile name={blueprint.name} />
          <span className="min-w-0">
            <span className="block truncate text-sm font-medium text-zinc-100">
              {blueprint.name}
            </span>
            <span className="block truncate text-xs text-zinc-600">
              from {blueprint.source_site_name} · WP {blueprint.wp_version} · PHP{" "}
              {blueprint.php_version}
            </span>
          </span>
        </button>
        <div className="flex shrink-0 gap-1">
          <button
            onClick={onUse}
            className="rounded-md border border-zinc-700 px-2.5 py-1 text-xs font-medium text-zinc-200 hover:border-zinc-500"
          >
            Use
          </button>
          <button
            onClick={onDelete}
            title="Delete blueprint"
            className="rounded-md border border-zinc-800 px-2 py-1 text-xs text-zinc-600 hover:border-red-900 hover:text-red-400"
          >
            ✕
          </button>
        </div>
      </div>
      {blueprint.description && (
        <p className="mt-2 line-clamp-2 text-xs text-zinc-500">{blueprint.description}</p>
      )}
      <div className="mt-2">
        <BlueprintChips blueprint={blueprint} />
      </div>
    </div>
  );
}

/** The chosen-blueprint summary shown in place of the version selects. */
function BlueprintSummary({
  blueprint,
  onClear,
}: {
  blueprint: Blueprint;
  onClear: () => void;
}) {
  return (
    <div className="rounded-lg border border-violet-900/60 bg-violet-950/20 p-3">
      <div className="flex items-start justify-between gap-2">
        <div className="min-w-0">
          <p className="text-sm font-medium text-zinc-100">
            Based on <span className="text-violet-300">{blueprint.name}</span>
          </p>
          <p className="text-xs text-zinc-500">
            from {blueprint.source_site_name} · WP {blueprint.wp_version} · PHP{" "}
            {blueprint.php_version}
          </p>
        </div>
        <button
          onClick={onClear}
          className="shrink-0 text-xs text-zinc-500 hover:text-zinc-300"
        >
          Use a blank site
        </button>
      </div>
      {blueprint.description && (
        <p className="mt-2 text-xs text-zinc-500">{blueprint.description}</p>
      )}
      <div className="mt-2">
        <BlueprintChips blueprint={blueprint} />
      </div>
    </div>
  );
}
