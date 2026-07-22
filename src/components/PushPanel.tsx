import { useCallback, useEffect, useState } from "react";
import { ipc } from "../lib/ipc";
import { toastError } from "../lib/errors";
import { useServerKit } from "../stores/serverkit";
import { isTerminalStage, useSites } from "../stores/sites";
import type { RemoteWpSite, SyncRecord } from "../lib/types";
import SectionTitle from "./SectionTitle";
import { SyncIcon } from "./icons";

/** Sync-history result colours; anything unrecognised falls through to red. */
const STATUS_CLASSES: Record<string, string> = {
  success: "text-emerald-400",
  cancelled: "text-zinc-400",
};

/** "Push to ServerKit" panel on the site detail page (M4). */
export default function PushPanel({ siteId, running }: { siteId: string; running: boolean }) {
  const connections = useServerKit((s) => s.connections);
  const refreshConnections = useServerKit((s) => s.refresh);
  const remote = useServerKit((s) => s.remote);
  const fetchRemoteSites = useServerKit((s) => s.fetchRemoteSites);
  const progress = useSites((s) => s.progress);

  const [connectionId, setConnectionId] = useState("");
  const [remoteSiteId, setRemoteSiteId] = useState<number | "">("");
  const [newSiteName, setNewSiteName] = useState("");
  const [creating, setCreating] = useState(false);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [history, setHistory] = useState<SyncRecord[]>([]);

  const refreshHistory = useCallback(() => {
    ipc.listSyncHistory(siteId).then(setHistory).catch(() => {});
  }, [siteId]);

  useEffect(() => {
    void refreshConnections();
    refreshHistory();
  }, [refreshConnections, refreshHistory]);

  // Refresh history when a sync operation for this site finishes — including
  // a cancel, which is just as terminal as a failure. Missing that stage here
  // would leave `busy` set and every push button disabled for good.
  useEffect(() => {
    if (progress && isTerminalStage(progress.stage)) {
      refreshHistory();
      setBusy(null);
    }
  }, [progress, refreshHistory]);

  useEffect(() => {
    if (connectionId) void fetchRemoteSites(connectionId);
    setRemoteSiteId("");
  }, [connectionId, fetchRemoteSites]);

  const conn = connections.find((c) => c.id === connectionId);
  const remoteState = connectionId ? remote[connectionId] : undefined;
  const sites = remoteState?.sites ?? [];
  const selectedRemote: RemoteWpSite | undefined = sites.find((s) => s.id === remoteSiteId);

  const select =
    "w-full rounded-md border border-zinc-700 bg-zinc-950 px-3 py-2 text-sm text-zinc-100 outline-none focus:border-violet-600";

  const op = async (label: string, context: string, fn: () => Promise<void>) => {
    setBusy(label);
    try {
      await fn();
    } catch (e) {
      // Success/failure feedback comes from the site-event stream; this
      // covers rejections the event stream didn't (deduped in toastError).
      toastError(e, context);
      setBusy(null);
    }
  };

  const createRemote = async () => {
    if (!newSiteName.trim()) return;
    setCreating(true);
    setError(null);
    try {
      await ipc.createRemoteSite(connectionId, newSiteName.trim());
      setNewSiteName("");
      await fetchRemoteSites(connectionId);
    } catch (e) {
      setError(typeof e === "string" ? e : String(e));
    } finally {
      setCreating(false);
    }
  };

  if (connections.length === 0) {
    return (
      <section className="rounded-xl border border-zinc-800 bg-zinc-900/60 p-5">
        <SectionTitle icon={SyncIcon}>ServerKit sync</SectionTitle>
        <p className="mt-3 text-sm text-zinc-600">
          Add a ServerKit connection in Settings to push this site to a server.
        </p>
      </section>
    );
  }

  return (
    <section className="rounded-xl border border-zinc-800 bg-zinc-900/60 p-5">
      <SectionTitle icon={SyncIcon}>ServerKit sync</SectionTitle>

      <div className="mt-4 grid grid-cols-1 gap-3 sm:grid-cols-2">
        <label className="block">
          <span className="mb-1 block text-xs text-zinc-500">Connection</span>
          <select value={connectionId} onChange={(e) => setConnectionId(e.target.value)} className={select}>
            <option value="">Select a server…</option>
            {connections.map((c) => (
              <option key={c.id} value={c.id}>
                {c.label} ({c.url})
              </option>
            ))}
          </select>
        </label>
        <label className="block">
          <span className="mb-1 block text-xs text-zinc-500">Remote site</span>
          <select
            value={remoteSiteId}
            onChange={(e) => setRemoteSiteId(e.target.value ? Number(e.target.value) : "")}
            disabled={!connectionId || remoteState?.loading}
            className={select}
          >
            <option value="">
              {remoteState?.loading ? "Loading…" : "Select a remote site…"}
            </option>
            {sites.map((s) => (
              <option key={s.id} value={s.id}>
                {s.name} {s.url ? `(${s.url})` : ""}
              </option>
            ))}
          </select>
        </label>
      </div>

      {remoteState?.error && <p className="mt-2 text-sm text-red-400">{remoteState.error}</p>}

      {connectionId && (
        <div className="mt-3 flex items-center gap-2">
          <input
            value={newSiteName}
            onChange={(e) => setNewSiteName(e.target.value)}
            placeholder="New remote site name…"
            className="w-56 rounded-md border border-zinc-700 bg-zinc-950 px-3 py-1.5 text-sm text-zinc-100 outline-none focus:border-violet-600"
          />
          <button
            onClick={() => void createRemote()}
            disabled={creating || !newSiteName.trim()}
            className="rounded-md border border-zinc-700 px-3 py-1.5 text-xs font-medium text-zinc-200 hover:border-zinc-500 disabled:opacity-50"
          >
            {creating ? "Creating…" : "Create remote site"}
          </button>
          <span className="text-xs text-zinc-600">provisions a fresh WordPress site on {conn?.label}</span>
        </div>
      )}

      <div className="mt-4 flex flex-wrap gap-2">
        <button
          onClick={() =>
            void op("code", "Push code", () => ipc.pushSiteCode(connectionId, siteId, remoteSiteId as number))
          }
          disabled={busy !== null || !connectionId || !remoteSiteId}
          className="rounded-md bg-violet-600 px-4 py-2 text-sm font-medium text-white hover:bg-violet-500 disabled:opacity-50"
        >
          {busy === "code" ? "Pushing…" : "Push code"}
        </button>
        <button
          onClick={() =>
            void op("pushdb", "Push DB", () => ipc.pushSiteDb(connectionId, siteId, remoteSiteId as number))
          }
          disabled={busy !== null || !connectionId || !remoteSiteId || !running}
          className="rounded-md bg-violet-600 px-4 py-2 text-sm font-medium text-white hover:bg-violet-500 disabled:opacity-50"
        >
          {busy === "pushdb" ? "Pushing…" : "Push DB"}
        </button>
        <button
          onClick={() =>
            void op("pulldb", "Pull DB", () =>
              ipc.pullSiteDb(connectionId, siteId, remoteSiteId as number, selectedRemote?.url ?? null)
            )
          }
          disabled={busy !== null || !connectionId || !remoteSiteId || !running}
          className="rounded-md border border-zinc-700 px-4 py-2 text-sm font-medium text-zinc-200 hover:border-zinc-500 disabled:opacity-50"
        >
          {busy === "pulldb" ? "Pulling…" : "Pull DB"}
        </button>
        {!running && (
          <span className="self-center text-xs text-zinc-600">DB operations need the site running</span>
        )}
      </div>

      {error && <p className="mt-2 text-sm text-red-400">{error}</p>}

      {history.length > 0 && (
        <div className="mt-5">
          <h3 className="text-xs font-semibold uppercase tracking-wide text-zinc-500">Sync history</h3>
          <table className="mt-2 w-full text-sm">
            <thead>
              <tr className="text-left text-xs text-zinc-600">
                <th className="pb-1 font-medium">When</th>
                <th className="pb-1 font-medium">Op</th>
                <th className="pb-1 font-medium">Result</th>
                <th className="pb-1 font-medium">Message</th>
              </tr>
            </thead>
            <tbody>
              {history.map((h) => (
                <tr key={h.id} className="border-t border-zinc-800/60">
                  <td className="py-1.5 pr-2 text-xs text-zinc-500">
                    {new Date(h.created_at).toLocaleString()}
                  </td>
                  <td className="py-1.5 pr-2 text-zinc-300">
                    {h.direction} {h.kind}
                  </td>
                  {/* A cancel was deliberate — neutral, not red like a failure. */}
                  <td className={`py-1.5 pr-2 ${STATUS_CLASSES[h.status] ?? "text-red-400"}`}>
                    {h.status}
                  </td>
                  <td className="max-w-xs truncate py-1.5 text-xs text-zinc-500" title={h.message}>
                    {h.message}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </section>
  );
}
