import { useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useNav } from "../stores/nav";
import { useSites } from "../stores/sites";
import StatusBadge from "../components/StatusBadge";
import NewSiteDialog from "../components/NewSiteDialog";

export default function Dashboard() {
  const sites = useSites((s) => s.sites);
  const loading = useSites((s) => s.loading);
  const busyId = useSites((s) => s.busyId);
  const start = useSites((s) => s.start);
  const stop = useSites((s) => s.stop);
  const remove = useSites((s) => s.remove);
  const navigate = useNav((s) => s.navigate);
  const [showDialog, setShowDialog] = useState(false);

  return (
    <div className="p-8">
      <div className="mb-6 flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-semibold text-white">Sites</h1>
          <p className="mt-1 text-sm text-zinc-500">Local WordPress sites running in Docker.</p>
        </div>
        <button
          onClick={() => setShowDialog(true)}
          className="rounded-md bg-emerald-600 px-4 py-2 text-sm font-medium text-white hover:bg-emerald-500"
        >
          + New Site
        </button>
      </div>

      {sites.length === 0 && !loading ? (
        <div className="flex flex-col items-center justify-center rounded-xl border border-dashed border-zinc-800 py-20 text-center">
          <p className="text-zinc-400">No sites yet.</p>
          <p className="mt-1 text-sm text-zinc-600">Create your first local WordPress site in one click.</p>
          <button
            onClick={() => setShowDialog(true)}
            className="mt-4 rounded-md bg-emerald-600 px-4 py-2 text-sm font-medium text-white hover:bg-emerald-500"
          >
            + New Site
          </button>
        </div>
      ) : (
        <div className="grid grid-cols-1 gap-4 lg:grid-cols-2 xl:grid-cols-3">
          {sites.map((site) => (
            <div
              key={site.id}
              className="rounded-xl border border-zinc-800 bg-zinc-900/60 p-5 transition-colors hover:border-zinc-700"
            >
              <div className="flex items-start justify-between gap-2">
                <button
                  onClick={() => navigate({ name: "site", id: site.id })}
                  className="text-left text-base font-semibold text-white hover:text-emerald-400"
                >
                  {site.name}
                </button>
                <StatusBadge status={site.live_status} />
              </div>
              <p className="mt-1 text-sm text-zinc-500">http://localhost:{site.port}</p>
              <p className="mt-1 text-xs text-zinc-600">
                WordPress {site.wp_version} · PHP {site.php_version}
              </p>

              <div className="mt-4 flex flex-wrap gap-2">
                {site.live_status === "running" ? (
                  <>
                    <button
                      onClick={() => void openUrl(`http://localhost:${site.port}`)}
                      className="rounded-md border border-zinc-700 px-3 py-1.5 text-xs font-medium text-zinc-200 hover:border-zinc-500"
                    >
                      Open
                    </button>
                    <button
                      onClick={() => void stop(site.id)}
                      disabled={busyId === site.id}
                      className="rounded-md border border-zinc-700 px-3 py-1.5 text-xs font-medium text-zinc-200 hover:border-zinc-500 disabled:opacity-50"
                    >
                      Stop
                    </button>
                  </>
                ) : (
                  <button
                    onClick={() => void start(site.id)}
                    disabled={busyId === site.id || site.live_status === "creating"}
                    className="rounded-md border border-zinc-700 px-3 py-1.5 text-xs font-medium text-zinc-200 hover:border-zinc-500 disabled:opacity-50"
                  >
                    Start
                  </button>
                )}
                <button
                  onClick={() => navigate({ name: "site", id: site.id })}
                  className="rounded-md border border-zinc-700 px-3 py-1.5 text-xs font-medium text-zinc-200 hover:border-zinc-500"
                >
                  Details
                </button>
                <button
                  onClick={() => {
                    if (window.confirm(`Delete "${site.name}"? This removes its containers, database and files.`)) {
                      void remove(site.id);
                    }
                  }}
                  disabled={busyId === site.id}
                  className="rounded-md border border-red-900 px-3 py-1.5 text-xs font-medium text-red-400 hover:border-red-700 disabled:opacity-50"
                >
                  Delete
                </button>
              </div>
            </div>
          ))}
        </div>
      )}

      {showDialog && <NewSiteDialog onClose={() => setShowDialog(false)} />}
    </div>
  );
}
