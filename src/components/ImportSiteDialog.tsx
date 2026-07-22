import { useEffect, useState } from "react";
import { ipc } from "../lib/ipc";
import { useNav } from "../stores/nav";
import { useServerKit } from "../stores/serverkit";
import { useDialog } from "../hooks/useDialog";
import type { AppInfo } from "../lib/types";

/**
 * Plan 18 — confirm cloning a remote ServerKit site down as a new local site.
 *
 * The dialog exists mostly to show the version readout: importing a PHP 8.3
 * site onto an 8.1 image usually works but not always, and it is much cheaper
 * to say so here than to have the user discover it in a broken local copy.
 */
export default function ImportSiteDialog() {
  const target = useServerKit((s) => s.importing);
  const busy = useServerKit((s) => s.importBusy);
  const close = useServerKit((s) => s.closeImport);
  const importSite = useServerKit((s) => s.importSite);
  const navigate = useNav((s) => s.navigate);
  const { overlayProps, panelProps } = useDialog(() => {
    // An import provisions containers; closing mid-flight would only orphan
    // the dialog, not the operation.
    if (!busy) close();
  });

  const [name, setName] = useState("");
  const [versions, setVersions] = useState<Pick<AppInfo, "wp_versions" | "php_versions">>({
    wp_versions: ["6.7", "6.6", "6.5"],
    php_versions: ["8.3", "8.2", "8.1"],
  });

  useEffect(() => {
    ipc.appInfo().then(setVersions).catch(() => {});
  }, []);

  // Re-seed the name whenever a different remote site is picked.
  useEffect(() => {
    setName(target?.site.name ?? "");
  }, [target?.site.id, target?.site.name]);

  if (!target) return null;
  const { site } = target;
  const isPhp = site.kind === "php";

  const wp = matchVersion(versions.wp_versions, site.wp_version);
  const php = matchVersion(versions.php_versions, site.php_version);
  // A php site has no WordPress version, so only its PHP image match matters.
  const mismatch = isPhp ? !php.exact : !wp.exact || !php.exact;

  const submit = async () => {
    const created = await importSite(name.trim() || undefined);
    if (created) navigate({ name: "site", id: created.id });
  };

  return (
    <div {...overlayProps} className="fixed inset-0 z-40 flex items-center justify-center bg-black/60">
      <div
        {...panelProps}
        role="dialog"
        aria-modal="true"
        aria-label="Import remote site"
        className="w-full max-w-md rounded-xl border border-zinc-800 bg-zinc-900 p-6 shadow-2xl"
      >
        <h2 className="text-lg font-semibold text-zinc-50">Import “{site.name}”</h2>
        <p className="mt-1 text-sm text-zinc-500">
          {isPhp
            ? "LocalKit will create a new local PHP/Laravel site and copy this server site's application code and database into it. The remote site is not modified."
            : "LocalKit will create a new local site and copy this server site's wp-content and database into it, rewriting URLs to the local address. The remote site is not modified."}
        </p>

        <div className="mt-5 flex flex-col gap-4">
          <label className="block">
            <span className="mb-1 block text-sm text-zinc-400">Local site name</span>
            <input
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder={site.name}
              autoFocus
              disabled={busy}
              className="w-full rounded-md border border-zinc-700 bg-zinc-950 px-3 py-2 text-sm text-zinc-100 outline-none focus:border-violet-600 disabled:opacity-50"
            />
          </label>

          <dl className="rounded-lg border border-zinc-800 bg-zinc-950/60 p-3 text-sm">
            <Row label="Remote URL" value={site.url ?? "—"} mono />
            <Row label="Kind" value={isPhp ? "PHP / Laravel" : "WordPress"} />
            {!isPhp && (
              <Row
                label="WordPress"
                value={`${site.wp_version ?? "unknown"} → ${wp.chosen}`}
                warn={!wp.exact}
              />
            )}
            <Row
              label="PHP"
              value={`${site.php_version ?? "unknown"} → ${php.chosen}`}
              warn={!php.exact}
            />
          </dl>

          {mismatch && (
            <p className="rounded-md border border-amber-900/60 bg-amber-500/10 px-3 py-2 text-xs text-amber-300">
              LocalKit does not have an exact image match for this site's versions and will use
              the closest available. That usually works, but plugins pinned to the remote
              version may misbehave.
            </p>
          )}
        </div>

        <div className="mt-6 flex justify-end gap-2">
          <button
            onClick={close}
            disabled={busy}
            className="rounded-md px-4 py-2 text-sm text-zinc-400 hover:text-zinc-200 disabled:opacity-50"
          >
            Cancel
          </button>
          <button
            onClick={() => void submit()}
            disabled={busy}
            className="rounded-md bg-violet-600 px-4 py-2 text-sm font-medium text-white hover:bg-violet-500 disabled:opacity-50"
          >
            {busy ? "Importing…" : "Import site"}
          </button>
        </div>
      </div>
    </div>
  );
}

function Row({
  label,
  value,
  mono,
  warn,
}: {
  label: string;
  value: string;
  mono?: boolean;
  warn?: boolean;
}) {
  return (
    <div className="flex items-baseline justify-between gap-3 py-0.5">
      <dt className="shrink-0 text-zinc-500">{label}</dt>
      <dd
        className={`truncate ${mono ? "font-mono text-xs" : ""} ${
          warn ? "text-amber-400" : "text-zinc-300"
        }`}
      >
        {value}
      </dd>
    </div>
  );
}

/**
 * Frontend mirror of `sync::match_version`: drop the remote's patch level and
 * match major.minor against the image allowlist, else fall back to newest.
 * Kept in sync by hand — it only drives the readout, and the backend's copy
 * is what actually picks the image.
 */
function matchVersion(available: string[], remote: string | null): { chosen: string; exact: boolean } {
  const newest = available[0];
  if (!remote?.trim()) return { chosen: newest, exact: false };
  const [major, minor] = remote.trim().split(".");
  const majorMinor = minor === undefined ? remote.trim() : `${major}.${minor}`;
  return available.includes(majorMinor)
    ? { chosen: majorMinor, exact: true }
    : { chosen: newest, exact: false };
}
