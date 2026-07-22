import { useEffect, useState } from "react";
import { useServerKit } from "../stores/serverkit";
import type { RemoteWpSite, ServerKitConnection } from "../lib/types";

export default function ServerKitSettings() {
  const connections = useServerKit((s) => s.connections);
  const refresh = useServerKit((s) => s.refresh);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  return (
    <section className="rounded-xl border border-zinc-800 bg-zinc-900/60 p-5">
      <div className="flex items-center justify-between">
        <h2 className="text-sm font-semibold uppercase tracking-wide text-zinc-500">
          ServerKit connections
        </h2>
        <span className="rounded-full border border-violet-800 bg-violet-500/15 px-2.5 py-0.5 text-xs font-medium text-violet-400">
          Push/pull
        </span>
      </div>
      <p className="mt-3 text-sm text-zinc-500">
        Connect to a ServerKit-managed server to browse its WordPress sites and push/pull code and
        databases from a site's detail page. Requires the serverkit-localkit extension on the
        server. API keys are stored in LocalKit's local database.
      </p>

      <ConnectionForm />

      {connections.length > 0 && (
        <div className="mt-5 space-y-3">
          {connections.map((c) => (
            <ConnectionCard key={c.id} conn={c} />
          ))}
        </div>
      )}
    </section>
  );
}

function ConnectionForm() {
  const test = useServerKit((s) => s.test);
  const testing = useServerKit((s) => s.testing);
  const testResult = useServerKit((s) => s.testResult);
  const save = useServerKit((s) => s.save);
  const saving = useServerKit((s) => s.saving);
  const clearTestResult = useServerKit((s) => s.clearTestResult);

  const [label, setLabel] = useState("");
  const [url, setUrl] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [error, setError] = useState<string | null>(null);

  const input =
    "w-full rounded-md border border-zinc-700 bg-zinc-950 px-3 py-2 text-sm text-zinc-100 outline-none focus:border-violet-600";

  const doTest = async () => {
    setError(null);
    await test(url, apiKey);
  };

  const doSave = async () => {
    setError(null);
    if (!label.trim() || !url.trim() || !apiKey.trim()) {
      setError("Label, URL and API key are all required.");
      return;
    }
    if (await save(label, url, apiKey)) {
      setLabel("");
      setUrl("");
      setApiKey("");
      clearTestResult();
    }
  };

  return (
    <div className="mt-4 rounded-lg border border-zinc-800 bg-zinc-950/60 p-4">
      <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
        <input value={label} onChange={(e) => setLabel(e.target.value)} placeholder="Label (e.g. Production)" className={input} />
        <input value={url} onChange={(e) => setUrl(e.target.value)} placeholder="Server URL (https://panel.example.com)" className={input} />
        <input
          value={apiKey}
          onChange={(e) => setApiKey(e.target.value)}
          placeholder="API key"
          type="password"
          className={`${input} sm:col-span-2`}
        />
      </div>
      {error && <p className="mt-2 text-sm text-red-400">{error}</p>}
      {testResult && (
        <p className={`mt-2 text-sm ${testResult.ok ? "text-emerald-400" : "text-red-400"}`}>
          {testResult.message}
        </p>
      )}
      <div className="mt-3 flex gap-2">
        <button
          onClick={() => void doTest()}
          disabled={testing || !url.trim() || !apiKey.trim()}
          className="rounded-md border border-zinc-700 px-4 py-2 text-sm text-zinc-200 hover:border-zinc-500 disabled:opacity-50"
        >
          {testing ? "Testing…" : "Test"}
        </button>
        <button
          onClick={() => void doSave()}
          disabled={saving || !label.trim() || !url.trim() || !apiKey.trim()}
          className="rounded-md bg-violet-600 px-4 py-2 text-sm font-medium text-white hover:bg-violet-500 disabled:opacity-50"
        >
          {saving ? "Saving…" : "Save connection"}
        </button>
      </div>
    </div>
  );
}

function ConnectionCard({ conn }: { conn: ServerKitConnection }) {
  const remove = useServerKit((s) => s.remove);
  const busyId = useServerKit((s) => s.busyId);
  const remote = useServerKit((s) => s.remote[conn.id]);
  const fetchRemoteSites = useServerKit((s) => s.fetchRemoteSites);
  const [expanded, setExpanded] = useState(false);

  const toggle = () => {
    const next = !expanded;
    setExpanded(next);
    if (next) void fetchRemoteSites(conn.id);
  };

  return (
    <div className="rounded-lg border border-zinc-800 bg-zinc-950/60">
      <div className="flex items-center justify-between gap-3 p-4">
        <div className="min-w-0">
          <p className="truncate text-sm font-medium text-zinc-50">{conn.label}</p>
          <p className="truncate text-xs text-zinc-500">{conn.url}</p>
        </div>
        <div className="flex shrink-0 gap-2">
          <button
            onClick={toggle}
            className="rounded-md border border-zinc-700 px-3 py-1.5 text-xs font-medium text-zinc-200 hover:border-zinc-500"
          >
            {expanded ? "Hide sites" : "View WP sites"}
          </button>
          <button
            onClick={() => {
              if (window.confirm(`Delete connection "${conn.label}"?`)) void remove(conn.id);
            }}
            disabled={busyId === conn.id}
            className="rounded-md border border-red-900 px-3 py-1.5 text-xs font-medium text-red-400 hover:border-red-700 disabled:opacity-50"
          >
            Delete
          </button>
        </div>
      </div>

      {expanded && (
        <div className="border-t border-zinc-800 p-4">
          <div className="mb-2 flex items-center justify-between">
            <h3 className="text-xs font-semibold uppercase tracking-wide text-zinc-500">
              Remote WordPress sites
            </h3>
            <button
              onClick={() => void fetchRemoteSites(conn.id)}
              className="text-xs text-zinc-500 hover:text-zinc-300"
            >
              Refresh
            </button>
          </div>
          {!remote || remote.loading ? (
            <p className="text-sm text-zinc-600">Loading…</p>
          ) : remote.error ? (
            <p className="text-sm text-red-400">{remote.error}</p>
          ) : remote.sites && remote.sites.length === 0 ? (
            <p className="text-sm text-zinc-600">No WordPress sites on this server.</p>
          ) : (
            <table className="w-full text-sm">
              <thead>
                <tr className="text-left text-xs text-zinc-600">
                  <th className="pb-1 font-medium">Name</th>
                  <th className="pb-1 font-medium">URL</th>
                  <th className="pb-1 font-medium">Status</th>
                  <th className="pb-1 font-medium">WP</th>
                  <th className="pb-1 font-medium">Envs</th>
                  <th className="pb-1" />
                </tr>
              </thead>
              <tbody>
                {(remote.sites ?? []).map((s) => (
                  <tr key={s.id} className="border-t border-zinc-800/60">
                    <td className="py-1.5 text-zinc-200">{s.name}</td>
                    <td className="py-1.5 font-mono text-xs text-zinc-400">{s.url ?? "—"}</td>
                    <td className={`py-1.5 ${s.status === "running" ? "text-emerald-400" : "text-zinc-500"}`}>
                      {s.status}
                    </td>
                    <td className="py-1.5 text-zinc-500">{s.wp_version ?? "—"}</td>
                    <td className="py-1.5 text-zinc-500">{s.environment_count}</td>
                    <td className="py-1.5 text-right">
                      <ImportButton
                        connectionId={conn.id}
                        site={s}
                        canImport={remote.canImport}
                      />
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </div>
      )}
    </div>
  );
}

/**
 * Per-site "clone this down here". Disabled — with the reason in the tooltip —
 * rather than hidden when the site or server can't support it: a missing
 * button reads as a bug, a disabled one explains itself.
 */
function ImportButton({
  connectionId,
  site,
  canImport,
}: {
  connectionId: string;
  site: RemoteWpSite;
  canImport: boolean | null;
}) {
  const openImport = useServerKit((s) => s.openImport);

  const blocked = site.multisite
    ? "Multisite installs cannot be imported."
    : canImport === false
      ? "The serverkit-localkit extension on this server is too old to import sites."
      : canImport === null
        ? "Checking what this server supports…"
        : null;

  return (
    <button
      onClick={() => openImport(connectionId, site)}
      disabled={blocked !== null}
      title={blocked ?? `Import "${site.name}" as a new local site`}
      className="rounded-md border border-violet-800 px-2.5 py-1 text-xs font-medium text-violet-300 hover:border-violet-600 hover:text-violet-200 disabled:cursor-not-allowed disabled:border-zinc-800 disabled:text-zinc-600"
    >
      Import
    </button>
  );
}
