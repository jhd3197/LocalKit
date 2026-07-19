import { useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { siteUrl } from "../lib/domains";
import { useNav } from "../stores/nav";
import { useRouter } from "../stores/router";
import { useSites } from "../stores/sites";
import type { SiteWithStatus } from "../lib/types";
import StatusBadge from "../components/StatusBadge";
import NewSiteDialog from "../components/NewSiteDialog";
import { GridIcon, ListIcon, PlusIcon } from "../components/icons";

export default function Dashboard() {
  const sites = useSites((s) => s.sites);
  const loading = useSites((s) => s.loading);
  const siteView = useNav((s) => s.siteView);
  const setSiteView = useNav((s) => s.setSiteView);
  const [showDialog, setShowDialog] = useState(false);

  return (
    <div className="p-6">
      {/* Toolbar — the sidebar already says where you are, so no page title */}
      <div className="mb-5 flex items-center justify-between">
        <div className="flex items-center rounded-md border border-zinc-800 bg-zinc-900/60 p-0.5">
          {(
            [
              ["grid", GridIcon, "Grid view"],
              ["list", ListIcon, "List view"],
            ] as const
          ).map(([view, Icon, label]) => (
            <button
              key={view}
              onClick={() => setSiteView(view)}
              aria-label={label}
              title={label}
              className={`rounded px-2.5 py-1.5 transition-colors ${
                siteView === view
                  ? "bg-zinc-800 text-violet-400"
                  : "text-zinc-500 hover:text-zinc-300"
              }`}
            >
              <Icon className="h-4 w-4" />
            </button>
          ))}
        </div>
        <button
          onClick={() => setShowDialog(true)}
          className="flex items-center gap-1.5 rounded-md bg-violet-600 px-3.5 py-2 text-sm font-medium text-white hover:bg-violet-500"
        >
          <PlusIcon className="h-4 w-4" />
          New Site
        </button>
      </div>

      {sites.length === 0 && !loading ? (
        <div className="flex flex-col items-center justify-center rounded-xl border border-dashed border-zinc-800 py-20 text-center">
          <p className="text-zinc-400">No sites yet.</p>
          <p className="mt-1 text-sm text-zinc-600">Create your first local WordPress site in one click.</p>
          <button
            onClick={() => setShowDialog(true)}
            className="mt-4 flex items-center gap-1.5 rounded-md bg-violet-600 px-3.5 py-2 text-sm font-medium text-white hover:bg-violet-500"
          >
            <PlusIcon className="h-4 w-4" />
            New Site
          </button>
        </div>
      ) : siteView === "grid" ? (
        <GridView sites={sites} />
      ) : (
        <ListView sites={sites} />
      )}

      {showDialog && <NewSiteDialog onClose={() => setShowDialog(false)} />}
    </div>
  );
}

function useSiteActions(site: SiteWithStatus) {
  const busyId = useSites((s) => s.busyId);
  const start = useSites((s) => s.start);
  const stop = useSites((s) => s.stop);
  const remove = useSites((s) => s.remove);
  const navigate = useNav((s) => s.navigate);
  const router = useRouter((s) => s.status);
  const busy = busyId === site.id;
  const running = site.live_status === "running";
  const url = siteUrl(site.slug, site.port, router);

  return {
    busy,
    url,
    open: () => void openUrl(url),
    toggle: () => void (running ? stop(site.id) : start(site.id)),
    details: () => navigate({ name: "site", id: site.id }),
    remove: () => {
      if (window.confirm(`Delete "${site.name}"? This removes its containers, database and files.`)) {
        void remove(site.id);
      }
    },
  };
}

const ghostBtn =
  "rounded-md border border-zinc-700 px-2.5 py-1 text-xs font-medium text-zinc-300 hover:border-zinc-500 hover:text-zinc-100 disabled:opacity-50";
const dangerBtn =
  "rounded-md border border-red-900 px-2.5 py-1 text-xs font-medium text-red-400 hover:border-red-700 disabled:opacity-50";

function GridView({ sites }: { sites: SiteWithStatus[] }) {
  return (
    <div className="grid grid-cols-1 gap-4 lg:grid-cols-2 xl:grid-cols-3">
      {sites.map((site) => (
        <GridCard key={site.id} site={site} />
      ))}
    </div>
  );
}

function GridCard({ site }: { site: SiteWithStatus }) {
  const a = useSiteActions(site);
  const running = site.live_status === "running";
  return (
    <div className="rounded-xl border border-zinc-800 bg-zinc-900/60 p-4 transition-colors hover:border-zinc-700">
      <div className="flex items-start justify-between gap-2">
        <button
          onClick={a.details}
          className="text-left text-[15px] font-semibold tracking-tight text-white hover:text-violet-400"
        >
          {site.name}
        </button>
        <StatusBadge status={site.live_status} />
      </div>
      <p className="mt-1 font-mono text-xs text-zinc-500">{a.url}</p>
      <p className="mt-1 text-xs text-zinc-600">
        WordPress {site.wp_version} · PHP {site.php_version}
      </p>

      <div className="mt-3.5 flex flex-wrap gap-1.5">
        {running && (
          <button onClick={a.open} className={ghostBtn}>
            Open
          </button>
        )}
        <button
          onClick={a.toggle}
          disabled={a.busy || site.live_status === "creating"}
          className={ghostBtn}
        >
          {running ? "Stop" : "Start"}
        </button>
        <button onClick={a.details} className={ghostBtn}>
          Details
        </button>
        <button onClick={a.remove} disabled={a.busy} className={dangerBtn}>
          Delete
        </button>
      </div>
    </div>
  );
}

function ListView({ sites }: { sites: SiteWithStatus[] }) {
  return (
    <div className="overflow-hidden rounded-xl border border-zinc-800 bg-zinc-900/60">
      <table className="w-full text-sm">
        <thead>
          <tr className="border-b border-zinc-800 text-left text-xs font-medium uppercase tracking-wide text-zinc-600">
            <th className="px-4 py-2.5">Name</th>
            <th className="px-4 py-2.5">URL</th>
            <th className="px-4 py-2.5">Stack</th>
            <th className="px-4 py-2.5">Status</th>
            <th className="px-4 py-2.5 text-right">Actions</th>
          </tr>
        </thead>
        <tbody>
          {sites.map((site) => (
            <ListRow key={site.id} site={site} />
          ))}
        </tbody>
      </table>
    </div>
  );
}

function ListRow({ site }: { site: SiteWithStatus }) {
  const a = useSiteActions(site);
  const running = site.live_status === "running";
  return (
    <tr className="border-b border-zinc-800/60 transition-colors last:border-0 hover:bg-zinc-900">
      <td className="px-4 py-2.5">
        <button
          onClick={a.details}
          className="font-medium tracking-tight text-white hover:text-violet-400"
        >
          {site.name}
        </button>
      </td>
      <td className="px-4 py-2.5 font-mono text-xs text-zinc-500">{a.url}</td>
      <td className="px-4 py-2.5 text-xs text-zinc-500">
        WP {site.wp_version} · PHP {site.php_version}
      </td>
      <td className="px-4 py-2.5">
        <StatusBadge status={site.live_status} />
      </td>
      <td className="px-4 py-2.5">
        <div className="flex justify-end gap-1.5">
          {running && (
            <button onClick={a.open} className={ghostBtn}>
              Open
            </button>
          )}
          <button
            onClick={a.toggle}
            disabled={a.busy || site.live_status === "creating"}
            className={ghostBtn}
          >
            {running ? "Stop" : "Start"}
          </button>
          <button onClick={a.details} className={ghostBtn}>
            Details
          </button>
          <button onClick={a.remove} disabled={a.busy} className={dangerBtn}>
            Delete
          </button>
        </div>
      </td>
    </tr>
  );
}
