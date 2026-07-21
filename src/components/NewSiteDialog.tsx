import { useEffect, useState } from "react";
import { ipc } from "../lib/ipc";
import { useNav } from "../stores/nav";
import { useSites } from "../stores/sites";
import { useBlueprints } from "../stores/blueprints";
import { useDialog } from "../hooks/useDialog";
import type { AppInfo, Blueprint, DockerProjectInspection } from "../lib/types";

type Mode = "wordpress" | "php" | "docker";

function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let value = n / 1024;
  let unit = 0;
  while (value >= 1024 && unit + 1 < units.length) {
    value /= 1024;
    unit += 1;
  }
  return `${value >= 100 ? value.toFixed(0) : value.toFixed(1)} ${units[unit]}`;
}

export default function NewSiteDialog({ onClose }: { onClose: () => void }) {
  const createSite = useSites((s) => s.createSite);
  const createPhpSite = useSites((s) => s.createPhpSite);
  const blueprints = useBlueprints((s) => s.blueprints);
  const refreshBlueprints = useBlueprints((s) => s.refresh);
  const removeBlueprint = useBlueprints((s) => s.remove);
  const createFromBlueprint = useBlueprints((s) => s.createSite);
  const navigate = useNav((s) => s.navigate);
  const { overlayProps, panelProps } = useDialog(onClose);

  const [mode, setMode] = useState<Mode>("wordpress");
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

  // Docker import (plan 22).
  const [dockerPath, setDockerPath] = useState("");
  const [inspecting, setInspecting] = useState(false);
  const [inspection, setInspection] = useState<DockerProjectInspection | null>(null);
  const [dockerService, setDockerService] = useState("");
  const [dockerPort, setDockerPort] = useState<number>(0);
  const [includeAll, setIncludeAll] = useState(false);

  // PHP/Laravel stack (plan 26): empty skeleton, or import an existing folder.
  const [phpImport, setPhpImport] = useState(false);
  const [phpPath, setPhpPath] = useState("");

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

  const switchMode = (m: Mode) => {
    setMode(m);
    setError(null);
    setBlueprint(null);
  };

  const inspect = async () => {
    setError(null);
    setInspection(null);
    setInspecting(true);
    try {
      const info = await ipc.inspectDockerProject(dockerPath.trim());
      setInspection(info);
      setDockerService(info.suggested_service ?? info.services[0]?.name ?? "");
      setDockerPort(info.suggested_port ?? 0);
      // Default the site name to the folder's basename if none typed yet.
      if (!name.trim()) {
        const base = dockerPath.trim().replace(/[\\/]+$/, "").split(/[\\/]/).pop() ?? "";
        if (base) setName(base);
      }
    } catch (e) {
      setError(typeof e === "string" ? e : String(e));
    } finally {
      setInspecting(false);
    }
  };

  const submit = async () => {
    setBusy(true);
    setError(null);
    try {
      let site;
      if (mode === "docker") {
        site = await ipc.importDockerProject(
          name.trim(),
          dockerPath.trim(),
          dockerService,
          dockerPort,
          includeAll
        );
      } else if (mode === "php") {
        site = await createPhpSite(
          name.trim(),
          phpVersion,
          phpImport ? phpPath.trim() : undefined
        );
      } else if (blueprint) {
        site = await createFromBlueprint(blueprint.id, name);
      } else {
        site = await createSite(name, wpVersion, phpVersion);
      }
      onClose();
      navigate({ name: "site", id: site.id });
    } catch (e) {
      setError(typeof e === "string" ? e : String(e));
      setBusy(false);
    }
  };

  const tab = (m: Mode, label: string) => (
    <button
      onClick={() => switchMode(m)}
      className={`flex-1 rounded-md px-3 py-1.5 text-sm font-medium transition-colors ${
        mode === m ? "bg-zinc-800 text-violet-300" : "text-zinc-500 hover:text-zinc-300"
      }`}
    >
      {label}
    </button>
  );

  const canSubmit =
    mode === "docker"
      ? !!inspection && !!name.trim() && !!dockerService && dockerPort > 0
      : mode === "php"
        ? !!name.trim() && (!phpImport || !!phpPath.trim())
        : !!name.trim();
  const primaryLabel =
    mode === "docker"
      ? busy
        ? "Importing…"
        : "Import project"
      : busy
        ? "Creating…"
        : blueprint
          ? "Create from blueprint"
          : "Create site";

  return (
    <div
      {...overlayProps}
      className="fixed inset-0 z-40 flex items-center justify-center bg-black/60"
    >
      <div
        {...panelProps}
        role="dialog"
        aria-modal="true"
        aria-label="New site"
        className="flex max-h-[85vh] w-full max-w-md flex-col rounded-xl border border-zinc-800 bg-zinc-900 p-6 shadow-2xl"
      >
        <h2 className="text-lg font-semibold text-white">New site</h2>

        <div className="mt-3 flex items-center gap-1 rounded-lg border border-zinc-800 bg-zinc-950/60 p-1">
          {tab("wordpress", "WordPress")}
          {tab("php", "PHP / Laravel")}
          {tab("docker", "Docker project")}
        </div>

        <p className="mt-3 text-sm text-zinc-500">
          {mode === "docker"
            ? "LocalKit copies an existing Docker Compose project into a managed site — with a local domain, terminal and logs."
            : mode === "php"
              ? "LocalKit generates a php-fpm + nginx + MariaDB stack. Start from an empty Laravel-ready webroot, or import an existing PHP project."
              : blueprint
                ? "LocalKit will stamp out a new site from this blueprint — its database, files, plugins and theme."
                : "LocalKit will create a Docker project, start it, and install WordPress automatically."}
        </p>

        <div className="mt-5 flex min-h-0 flex-col gap-4 overflow-y-auto">
          {mode === "docker" ? (
            <DockerImportFields
              name={name}
              onName={setName}
              path={dockerPath}
              onPath={(p) => {
                setDockerPath(p);
                setInspection(null);
              }}
              inspecting={inspecting}
              inspection={inspection}
              onInspect={inspect}
              service={dockerService}
              onService={setDockerService}
              port={dockerPort}
              onPort={setDockerPort}
              includeAll={includeAll}
              onIncludeAll={setIncludeAll}
            />
          ) : mode === "php" ? (
            <PhpFields
              name={name}
              onName={setName}
              phpVersion={phpVersion}
              onPhpVersion={setPhpVersion}
              phpVersions={versions.php_versions}
              importing={phpImport}
              onImporting={setPhpImport}
              path={phpPath}
              onPath={setPhpPath}
            />
          ) : (
            <>
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
            </>
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
            disabled={busy || !canSubmit}
            className="rounded-md bg-violet-600 px-4 py-2 text-sm font-medium text-white hover:bg-violet-500 disabled:opacity-50"
          >
            {primaryLabel}
          </button>
        </div>
      </div>
    </div>
  );
}

/** The "PHP / Laravel" fields (plan 26): a name, a PHP version, and the choice
 * between an empty Laravel-ready skeleton and importing an existing folder. */
function PhpFields({
  name,
  onName,
  phpVersion,
  onPhpVersion,
  phpVersions,
  importing,
  onImporting,
  path,
  onPath,
}: {
  name: string;
  onName: (v: string) => void;
  phpVersion: string;
  onPhpVersion: (v: string) => void;
  phpVersions: string[];
  importing: boolean;
  onImporting: (v: boolean) => void;
  path: string;
  onPath: (v: string) => void;
}) {
  const input =
    "w-full rounded-md border border-zinc-700 bg-zinc-950 px-3 py-2 text-sm text-zinc-100 outline-none focus:border-violet-600";
  const seg = (on: boolean, label: string, sub: string, onClick: () => void) => (
    <button
      onClick={onClick}
      className={`flex-1 rounded-md border px-3 py-2 text-left transition-colors ${
        on
          ? "border-violet-600 bg-violet-950/30"
          : "border-zinc-800 bg-zinc-950/60 hover:border-zinc-700"
      }`}
    >
      <span className="block text-sm font-medium text-zinc-100">{label}</span>
      <span className="block text-xs text-zinc-500">{sub}</span>
    </button>
  );
  return (
    <>
      <label className="block">
        <span className="mb-1 block text-sm text-zinc-400">Site name</span>
        <input
          value={name}
          onChange={(e) => onName(e.target.value)}
          placeholder="My App"
          autoFocus
          className={input}
        />
      </label>

      <label className="block">
        <span className="mb-1 block text-sm text-zinc-400">PHP</span>
        <select value={phpVersion} onChange={(e) => onPhpVersion(e.target.value)} className={input}>
          {phpVersions.map((v) => (
            <option key={v} value={v}>
              {v}
            </option>
          ))}
        </select>
      </label>

      <div className="flex gap-2">
        {seg(!importing, "Empty skeleton", "A Laravel-ready public/ webroot", () => onImporting(false))}
        {seg(importing, "Import a folder", "Copy existing PHP code in", () => onImporting(true))}
      </div>

      {importing && (
        <label className="block">
          <span className="mb-1 block text-sm text-zinc-400">Project folder</span>
          <input
            value={path}
            onChange={(e) => onPath(e.target.value)}
            placeholder="C:\\path\\to\\my-php-app"
            className={input}
          />
          <span className="mt-1 block text-xs text-zinc-600">
            The folder is copied into LocalKit's <code className="font-mono">app/</code> directory —
            the original is left untouched.
          </span>
        </label>
      )}
    </>
  );
}

/** The "Import a Docker project" fields — pick a folder, inspect it, choose the
 * app service and port (plan 22). */
function DockerImportFields({
  name,
  onName,
  path,
  onPath,
  inspecting,
  inspection,
  onInspect,
  service,
  onService,
  port,
  onPort,
  includeAll,
  onIncludeAll,
}: {
  name: string;
  onName: (v: string) => void;
  path: string;
  onPath: (v: string) => void;
  inspecting: boolean;
  inspection: DockerProjectInspection | null;
  onInspect: () => void;
  service: string;
  onService: (v: string) => void;
  port: number;
  onPort: (v: number) => void;
  includeAll: boolean;
  onIncludeAll: (v: boolean) => void;
}) {
  const input =
    "w-full rounded-md border border-zinc-700 bg-zinc-950 px-3 py-2 text-sm text-zinc-100 outline-none focus:border-violet-600";
  return (
    <>
      <label className="block">
        <span className="mb-1 block text-sm text-zinc-400">Project folder</span>
        <div className="flex gap-2">
          <input
            value={path}
            onChange={(e) => onPath(e.target.value)}
            placeholder="C:\\path\\to\\my-app  (contains docker-compose.yml)"
            autoFocus
            className={input}
          />
          <button
            onClick={onInspect}
            disabled={inspecting || !path.trim()}
            className="shrink-0 rounded-md border border-zinc-700 px-3 py-2 text-sm text-zinc-200 hover:border-zinc-500 disabled:opacity-50"
          >
            {inspecting ? "Inspecting…" : "Inspect"}
          </button>
        </div>
        <span className="mt-1 block text-xs text-zinc-600">
          The folder is copied into LocalKit — the original is left untouched.
        </span>
      </label>

      {inspection && (
        <>
          <label className="block">
            <span className="mb-1 block text-sm text-zinc-400">Site name</span>
            <input value={name} onChange={(e) => onName(e.target.value)} placeholder="My App" className={input} />
          </label>

          <div className="grid grid-cols-2 gap-4">
            <label className="block">
              <span className="mb-1 block text-sm text-zinc-400">App service</span>
              <select value={service} onChange={(e) => onService(e.target.value)} className={input}>
                {inspection.services.map((s) => (
                  <option key={s.name} value={s.name}>
                    {s.name} ({s.image || "—"})
                  </option>
                ))}
              </select>
            </label>
            <label className="block">
              <span className="mb-1 block text-sm text-zinc-400">App port</span>
              <input
                type="number"
                value={port || ""}
                onChange={(e) => onPort(Number(e.target.value))}
                placeholder="8080"
                className={input}
              />
            </label>
          </div>

          <div className="rounded-lg border border-zinc-800 bg-zinc-950/60 p-3 text-xs text-zinc-500">
            <p>
              Copying <span className="text-zinc-300">{formatBytes(inspection.copy_bytes)}</span> from{" "}
              <code className="font-mono">{inspection.compose_file}</code>
              {inspection.db_engine && (
                <>
                  {" "}· detected a <span className="text-sky-300">{inspection.db_engine}</span> database
                </>
              )}
              .
            </p>
            <p className="mt-1">
              Excluding <code className="font-mono">{inspection.excluded.join(", ")}</code> by default.
            </p>
            <label className="mt-2 flex items-center gap-1.5 text-zinc-400">
              <input
                type="checkbox"
                checked={includeAll}
                onChange={(e) => onIncludeAll(e.target.checked)}
                className="accent-violet-500"
              />
              Copy those too
            </label>
          </div>
        </>
      )}
    </>
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
