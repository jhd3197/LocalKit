import { useCallback, useEffect, useRef, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ipc } from "../lib/ipc";
import { siteUrl } from "../lib/domains";
import { useNav } from "../stores/nav";
import { useRouter } from "../stores/router";
import { useSites } from "../stores/sites";
import type { SiteDetail as SiteDetailData, WpUser } from "../lib/types";
import StatusBadge from "../components/StatusBadge";
import CopyButton from "../components/CopyButton";
import PushPanel from "../components/PushPanel";

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

  // Load WP users for the WP Admin login picker (running sites only).
  useEffect(() => {
    setWpUsers(null);
    setLoginUserId(null);
    if (detail?.live_status === "running") {
      ipc
        .siteWpUsers(id)
        .then(setWpUsers)
        .catch(() => setWpUsers(null));
    }
  }, [id, detail?.live_status]);

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

  const url = siteUrl(detail.slug, detail.port, router);
  const running = detail.live_status === "running";

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
          <StatusBadge status={detail.live_status} />
        </div>
        <div className="flex gap-2">
          {running ? (
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
          <button
            onClick={() => {
              if (window.confirm(`Delete "${detail.name}"? This removes its containers, database and files.`)) {
                void remove(id).then(() => navigate({ name: "sites" }));
              }
            }}
            disabled={busyId === id}
            className="rounded-md border border-red-900 px-4 py-2 text-sm text-red-400 hover:border-red-700 disabled:opacity-50"
          >
            Delete
          </button>
        </div>
      </div>

      <div className="mt-6 grid grid-cols-1 gap-4 xl:grid-cols-2">
        {/* URL */}
        <section className="rounded-xl border border-zinc-800 bg-zinc-900/60 p-5">
          <h2 className="text-sm font-semibold uppercase tracking-wide text-zinc-500">Site</h2>
          <p className="mt-3 font-mono text-sm text-violet-400">{url}</p>
          <p className="mt-1 text-xs text-zinc-600">
            WordPress {detail.wp_version} · PHP {detail.php_version}
          </p>
          <div className="mt-4 flex flex-wrap items-center gap-2">
            <button
              onClick={() => void openUrl(url)}
              disabled={!running}
              className="rounded-md bg-violet-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-violet-500 disabled:opacity-50"
            >
              Open site
            </button>
            <button
              onClick={() => void wpAdminLogin()}
              disabled={!running || loggingIn}
              title={running ? "Log straight into wp-admin — no password needed" : "Start the site to log in"}
              className="rounded-md border border-zinc-700 px-3 py-1.5 text-xs font-medium text-zinc-200 hover:border-zinc-500 disabled:opacity-50"
            >
              {loggingIn ? "Logging in…" : "WP Admin"}
            </button>
            {running && wpUsers && wpUsers.length > 1 && (
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

        {/* Credentials */}
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

        {/* Database */}
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

        {/* wp-cli info */}
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
      </div>

      {/* ServerKit push/pull (M4) */}
      <div className="mt-4">
        <PushPanel siteId={id} running={running} />
      </div>

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
