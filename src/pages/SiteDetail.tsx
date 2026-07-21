import { useCallback, useEffect, useRef, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ipc } from "../lib/ipc";
import { siteUrl, sitePort } from "../lib/domains";
import { useNav } from "../stores/nav";
import { useRouter } from "../stores/router";
import { useSites } from "../stores/sites";
import type { SiteDetail as SiteDetailData, WpUser } from "../lib/types";
import StatusBadge from "../components/StatusBadge";
import KindBadge from "../components/KindBadge";
import CopyButton from "../components/CopyButton";
import PushPanel from "../components/PushPanel";
import SnapshotsPanel from "../components/SnapshotsPanel";
import DeleteSiteDialog from "../components/DeleteSiteDialog";
import CloneSiteDialog from "../components/CloneSiteDialog";
import SaveBlueprintDialog from "../components/SaveBlueprintDialog";
import { describeConflicts } from "../components/DomainsSettings";

export default function SiteDetail({ id }: { id: string }) {
  const navigate = useNav((s) => s.navigate);
  const start = useSites((s) => s.start);
  const stop = useSites((s) => s.stop);
  const remove = useSites((s) => s.remove);
  const busyId = useSites((s) => s.busyId);
  const logs = useSites((s) => s.logs[id]);
  const fetchLogs = useSites((s) => s.fetchLogs);
  const wpInfo = useSites((s) => s.wpInfo[id]);
  const fetchWpInfo = useSites((s) => s.fetchWpInfo);
  const sites = useSites((s) => s.sites);
  const router = useRouter((s) => s.status);

  const [detail, setDetail] = useState<SiteDetailData | null>(null);
  const [autoRefresh, setAutoRefresh] = useState(true);
  const [wpUsers, setWpUsers] = useState<WpUser[] | null>(null);
  const [loginUserId, setLoginUserId] = useState<number | null>(null);
  const [loggingIn, setLoggingIn] = useState(false);
  const [loginError, setLoginError] = useState<string | null>(null);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [cloneOpen, setCloneOpen] = useState(false);
  const [blueprintOpen, setBlueprintOpen] = useState(false);
  const logRef = useRef<HTMLPreElement>(null);

  const loadDetail = useCallback(() => {
    ipc
      .getSite(id)
      .then(setDetail)
      .catch(() => setDetail(null));
  }, [id]);

  useEffect(() => {
    loadDetail();
    void fetchLogs(id);
    void fetchWpInfo(id);
  }, [id, loadDetail, fetchLogs, fetchWpInfo]);

  // Re-fetch detail whenever the list refreshes (status may have changed).
  useEffect(() => {
    loadDetail();
  }, [sites, loadDetail]);

  // Poll logs.
  useEffect(() => {
    if (!autoRefresh) return;
    const t = setInterval(() => void fetchLogs(id), 3000);
    return () => clearInterval(t);
  }, [autoRefresh, id, fetchLogs]);

  // Keep the log viewer scrolled to the bottom.
  useEffect(() => {
    if (logRef.current) logRef.current.scrollTop = logRef.current.scrollHeight;
  }, [logs]);

  // Load WP users for the WP Admin login picker (running WordPress sites only).
  const canLogin = detail?.capabilities.one_click_login ?? false;
  useEffect(() => {
    setWpUsers(null);
    setLoginUserId(null);
    if (canLogin && detail?.live_status === "running") {
      ipc
        .siteWpUsers(id)
        .then(setWpUsers)
        .catch(() => setWpUsers(null));
    }
  }, [id, canLogin, detail?.live_status]);

  if (!detail) {
    return (
      <div className="p-8">
        <button onClick={() => navigate({ name: "sites" })} className="text-sm text-zinc-500 hover:text-zinc-300">
          ← Back to sites
        </button>
        <p className="mt-8 text-zinc-500">Site not found.</p>
      </div>
    );
  }

  const url = siteUrl(detail.slug, sitePort(detail), router);
  const running = detail.live_status === "running";
  // Up = running or degraded (containers up but unhealthy, plan 23). WP-only
  // affordances (login, wp-cli) stay gated on a healthy `running`; only the
  // Stop control treats a degraded site as stoppable.
  const up = running || detail.live_status === "degraded";
  const caps = detail.capabilities;

  // WP Admin one-click login: default to the install admin, picker overrides.
  const defaultUserId =
    wpUsers?.find((u) => u.login === detail.admin_user)?.id ?? wpUsers?.[0]?.id ?? null;
  const selectedUserId = loginUserId ?? defaultUserId;

  const wpAdminLogin = async () => {
    setLoggingIn(true);
    setLoginError(null);
    try {
      const loginUrl = await ipc.loginSite(id, selectedUserId ?? undefined);
      await openUrl(loginUrl);
    } catch (e) {
      setLoginError(String(e));
    } finally {
      setLoggingIn(false);
    }
  };

  return (
    <div className="p-8">
      <button onClick={() => navigate({ name: "sites" })} className="text-sm text-zinc-500 hover:text-zinc-300">
        ← Back to sites
      </button>

      <div className="mt-3 flex flex-wrap items-center justify-between gap-3">
        <div className="flex items-center gap-3">
          <h1 className="text-2xl font-semibold text-white">{detail.name}</h1>
          <KindBadge kind={detail.kind} />
          <StatusBadge status={detail.live_status} />
        </div>
        <div className="flex gap-2">
          {up ? (
            <button
              onClick={() => void stop(id)}
              disabled={busyId === id}
              className="rounded-md border border-zinc-700 px-4 py-2 text-sm text-zinc-200 hover:border-zinc-500 disabled:opacity-50"
            >
              Stop
            </button>
          ) : (
            <button
              onClick={() => void start(id)}
              disabled={busyId === id}
              className="rounded-md bg-violet-600 px-4 py-2 text-sm font-medium text-white hover:bg-violet-500 disabled:opacity-50"
            >
              Start
            </button>
          )}
          <button
            onClick={() => navigate({ name: "terminal", siteId: id })}
            className="rounded-md border border-zinc-700 px-4 py-2 text-sm text-zinc-200 hover:border-zinc-500"
          >
            Terminal
          </button>
          {caps.wp_tools && (
            <button
              onClick={() => setCloneOpen(true)}
              disabled={busyId === id}
              title="Copy this site's database and files into a new site"
              className="rounded-md border border-zinc-700 px-4 py-2 text-sm text-zinc-200 hover:border-zinc-500 disabled:opacity-50"
            >
              Clone
            </button>
          )}
          {caps.wp_tools && (
            <button
              onClick={() => setBlueprintOpen(true)}
              disabled={busyId === id}
              title="Save this site's stack as a reusable blueprint"
              className="rounded-md border border-zinc-700 px-4 py-2 text-sm text-zinc-200 hover:border-zinc-500 disabled:opacity-50"
            >
              Save as blueprint
            </button>
          )}
          <button
            onClick={() => setConfirmDelete(true)}
            disabled={busyId === id}
            className="rounded-md border border-red-900 px-4 py-2 text-sm text-red-400 hover:border-red-700 disabled:opacity-50"
          >
            Delete
          </button>
        </div>
      </div>

      {confirmDelete && (
        <DeleteSiteDialog
          siteName={detail.name}
          busy={busyId === id}
          onClose={() => setConfirmDelete(false)}
          onConfirm={(deleteSnapshots) => {
            setConfirmDelete(false);
            void remove(id, deleteSnapshots).then(() => navigate({ name: "sites" }));
          }}
        />
      )}

      {cloneOpen && (
        <CloneSiteDialog
          source={{ id, name: detail.name }}
          onClose={() => setCloneOpen(false)}
        />
      )}

      {blueprintOpen && (
        <SaveBlueprintDialog
          source={{ id, name: detail.name }}
          onClose={() => setBlueprintOpen(false)}
        />
      )}

      <RouterConflictBanner slug={detail.slug} port={sitePort(detail)} />

      <div className="mt-6 grid grid-cols-1 gap-4 xl:grid-cols-2">
        {/* URL */}
        <section className="rounded-xl border border-zinc-800 bg-zinc-900/60 p-5">
          <h2 className="text-sm font-semibold uppercase tracking-wide text-zinc-500">Site</h2>
          <p className="mt-3 font-mono text-sm text-violet-400">{url}</p>
          <p className="mt-1 text-xs text-zinc-600">
            {caps.wp_tools
              ? `WordPress ${detail.wp_version} · PHP ${detail.php_version}`
              : `Docker Compose · app service “${detail.config.service}”`}
          </p>
          <div className="mt-4 flex flex-wrap items-center gap-2">
            <button
              onClick={() => void openUrl(url)}
              disabled={!running}
              className="rounded-md bg-violet-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-violet-500 disabled:opacity-50"
            >
              Open site
            </button>
            {caps.one_click_login && (
              <button
                onClick={() => void wpAdminLogin()}
                disabled={!running || loggingIn}
                title={running ? "Log straight into wp-admin — no password needed" : "Start the site to log in"}
                className="rounded-md border border-zinc-700 px-3 py-1.5 text-xs font-medium text-zinc-200 hover:border-zinc-500 disabled:opacity-50"
              >
                {loggingIn ? "Logging in…" : "WP Admin"}
              </button>
            )}
            {caps.one_click_login && running && wpUsers && wpUsers.length > 1 && (
              <select
                value={selectedUserId ?? ""}
                onChange={(e) => setLoginUserId(Number(e.target.value))}
                title="User to log in as"
                className="rounded-md border border-zinc-700 bg-zinc-900 px-2 py-1.5 text-xs text-zinc-300"
              >
                {wpUsers.map((u) => (
                  <option key={u.id} value={u.id}>
                    {u.login} ({u.roles})
                  </option>
                ))}
              </select>
            )}
          </div>
          {loginError && <p className="mt-2 text-xs text-red-400">{loginError}</p>}
        </section>

        {/* Credentials (WordPress only) */}
        {caps.one_click_login && (
        <section className="rounded-xl border border-zinc-800 bg-zinc-900/60 p-5">
          <h2 className="text-sm font-semibold uppercase tracking-wide text-zinc-500">WP Admin credentials</h2>
          <dl className="mt-3 space-y-2 text-sm">
            <div className="flex items-center justify-between gap-2">
              <dt className="text-zinc-500">Username</dt>
              <dd className="flex items-center gap-2 font-mono text-zinc-200">
                {detail.admin_user} <CopyButton value={detail.admin_user} />
              </dd>
            </div>
            <div className="flex items-center justify-between gap-2">
              <dt className="text-zinc-500">Password</dt>
              <dd className="flex items-center gap-2 font-mono text-zinc-200">
                {detail.admin_pass || "—"} {detail.admin_pass && <CopyButton value={detail.admin_pass} />}
              </dd>
            </div>
          </dl>
        </section>
        )}

        {/* Database (needs the db_gui capability — hidden for docker apps) */}
        {caps.db_gui && (
        <section className="rounded-xl border border-zinc-800 bg-zinc-900/60 p-5">
          <h2 className="text-sm font-semibold uppercase tracking-wide text-zinc-500">Database (MariaDB)</h2>
          <dl className="mt-3 space-y-2 text-sm">
            {[
              ["Host", detail.db_host],
              ["Port", String(detail.db_port)],
              ["Database", detail.db_name],
              ["User", detail.db_user],
              ["Password", detail.db_password || "—"],
            ].map(([k, v]) => (
              <div key={k} className="flex items-center justify-between gap-2">
                <dt className="text-zinc-500">{k}</dt>
                <dd className="flex items-center gap-2 font-mono text-zinc-200">
                  {v} {v !== "—" && <CopyButton value={v} />}
                </dd>
              </div>
            ))}
          </dl>
        </section>
        )}

        {/* wp-cli info (WordPress only) */}
        {caps.wp_tools && (
        <section className="rounded-xl border border-zinc-800 bg-zinc-900/60 p-5">
          <div className="flex items-center justify-between">
            <h2 className="text-sm font-semibold uppercase tracking-wide text-zinc-500">WordPress info (wp-cli)</h2>
            <button
              onClick={() => void fetchWpInfo(id)}
              disabled={!running}
              className="text-xs text-zinc-500 hover:text-zinc-300 disabled:opacity-50"
            >
              Refresh
            </button>
          </div>
          {!running ? (
            <p className="mt-3 text-sm text-zinc-600">Start the site to query wp-cli.</p>
          ) : wpInfo === undefined ? (
            <p className="mt-3 text-sm text-zinc-600">Loading…</p>
          ) : wpInfo === null ? (
            <p className="mt-3 text-sm text-zinc-600">wp-cli info unavailable.</p>
          ) : (
            <>
              <p className="mt-3 text-sm text-zinc-300">
                Core version: <span className="font-mono text-violet-400">{wpInfo.core_version}</span>
              </p>
              <table className="mt-3 w-full text-sm">
                <thead>
                  <tr className="text-left text-xs text-zinc-600">
                    <th className="pb-1 font-medium">Plugin</th>
                    <th className="pb-1 font-medium">Status</th>
                    <th className="pb-1 font-medium">Version</th>
                  </tr>
                </thead>
                <tbody>
                  {wpInfo.plugins.map((p) => (
                    <tr key={p.name} className="border-t border-zinc-800/60">
                      <td className="py-1 font-mono text-zinc-300">{p.name}</td>
                      <td className={`py-1 ${p.status === "active" ? "text-emerald-400" : "text-zinc-500"}`}>
                        {p.status}
                      </td>
                      <td className="py-1 text-zinc-500">{p.version}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </>
          )}
        </section>
        )}
      </div>

      {/* Snapshots + one-click restore (plan 17) — every kind that supports them */}
      {caps.snapshots && (
        <div className="mt-4">
          <SnapshotsPanel siteId={id} />
        </div>
      )}

      {/* ServerKit push/pull (M4) — WordPress only until plan 26 */}
      {caps.wp_tools && (
        <div className="mt-4">
          <PushPanel siteId={id} running={running} />
        </div>
      )}

      {/* Logs */}
      <section className="mt-4 rounded-xl border border-zinc-800 bg-zinc-900/60 p-5">
        <div className="flex items-center justify-between">
          <h2 className="text-sm font-semibold uppercase tracking-wide text-zinc-500">Container logs</h2>
          <div className="flex items-center gap-3 text-xs text-zinc-500">
            <label className="flex items-center gap-1.5">
              <input
                type="checkbox"
                checked={autoRefresh}
                onChange={(e) => setAutoRefresh(e.target.checked)}
                className="accent-violet-500"
              />
              Auto-refresh
            </label>
            <button onClick={() => void fetchLogs(id)} className="hover:text-zinc-300">
              Refresh
            </button>
          </div>
        </div>
        <pre
          ref={logRef}
          className="mt-3 h-64 overflow-auto rounded-lg bg-zinc-950 p-3 font-mono text-xs leading-relaxed text-zinc-400"
        >
          {logs || "No logs yet."}
        </pre>
      </section>
    </div>
  );
}

/**
 * Plan 16: local domains are on but the router is blocked on its ports.
 *
 * This is the case where a toast is not enough — WordPress still has
 * `home`/`siteurl` pointing at `<slug>.test`, so the user is most likely
 * staring at the *other* program's "Site Not Found" page and has no way to
 * know two apps are fighting over port 80. Dismissible, but it comes back if
 * the conflict changes.
 */
function RouterConflictBanner({ slug, port }: { slug: string; port: number }) {
  const status = useRouter((s) => s.status);
  const refresh = useRouter((s) => s.refresh);
  const openSettings = useNav((s) => s.openSettings);
  const [dismissed, setDismissed] = useState<string | null>(null);

  // App.tsx only refreshes router status at startup, so a conflict that
  // appears later (the other app launched while LocalKit was open) would go
  // unreported on exactly the page where it matters. Re-check on arrival.
  useEffect(() => {
    void refresh();
  }, [refresh, slug]);

  const conflicts = status?.conflicts ?? [];
  if (!status?.enabled || conflicts.length === 0) return null;

  const key = conflicts.map((c) => `${c.port}:${c.process ?? "?"}`).join(",");
  if (dismissed === key) return null;

  return (
    <div className="mt-4 flex items-start gap-3 rounded-xl border border-amber-900/60 bg-amber-950/40 p-4">
      <div className="min-w-0 flex-1">
        <p className="text-sm font-medium text-amber-200">
          Local domains are blocked — another program is using{" "}
          {describeConflicts(conflicts)}
        </p>
        <p className="mt-1 text-xs text-amber-200/80">
          <code className="font-mono">{slug}.test</code> currently reaches that program, not this
          site — which is why you may be seeing someone else's “not found” page. This site is still
          served directly at{" "}
          <code className="font-mono">http://localhost:{port}</code>.
        </p>
        <button
          onClick={() => openSettings("domains")}
          className="mt-2.5 rounded-md bg-violet-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-violet-500"
        >
          Fix in Settings
        </button>
      </div>
      <button
        onClick={() => setDismissed(key)}
        aria-label="Dismiss"
        className="shrink-0 rounded-md px-2 py-1 text-xs text-amber-200/70 hover:bg-amber-900/40 hover:text-amber-100"
      >
        Dismiss
      </button>
    </div>
  );
}
