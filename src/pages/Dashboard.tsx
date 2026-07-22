import { useEffect, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { siteUrl, sitePort } from "../lib/domains";
import { useNav } from "../stores/nav";
import { useSiteView } from "../stores/settings";
import { useRouter } from "../stores/router";
import { useServerKit } from "../stores/serverkit";
import { useSites } from "../stores/sites";
import { useBlueprints } from "../stores/blueprints";
import { KIND_DOCKER, KIND_PHP, type SiteWithStatus } from "../lib/types";
import StatusBadge from "../components/StatusBadge";
import KindBadge from "../components/KindBadge";
import SiteTile from "../components/SiteTile";
import CloneSiteDialog from "../components/CloneSiteDialog";
import {
  ArrowUpRightIcon,
  DuplicateIcon,
  GridIcon,
  LinkIcon,
  ListIcon,
  PlayIcon,
  PlusIcon,
  StopSquareIcon,
  TrashIcon,
  WrenchIcon,
} from "../components/icons";

export default function Dashboard() {
  const sites = useSites((s) => s.sites);
  const loading = useSites((s) => s.loading);
  const [siteView, setSiteView] = useSiteView();
  const setNewSiteOpen = useNav((s) => s.setNewSiteOpen);
  const refreshConnections = useServerKit((s) => s.refresh);
  const blueprints = useBlueprints((s) => s.blueprints);
  const refreshBlueprints = useBlueprints((s) => s.refresh);
  // Which site the "name your copy" dialog is open for (plan 20).
  const [cloneTarget, setCloneTarget] = useState<SiteWithStatus | null>(null);

  // Imported sites name their origin connection, so the labels have to be
  // loaded even if the user never opens Settings → ServerKit. Blueprints feed
  // the empty-state hint and the New Site dialog.
  useEffect(() => {
    void refreshConnections();
    void refreshBlueprints();
  }, [refreshConnections, refreshBlueprints]);

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
          onClick={() => setNewSiteOpen(true)}
          className="flex items-center gap-1.5 rounded-md bg-violet-600 px-3.5 py-2 text-sm font-medium text-white hover:bg-violet-500"
        >
          <PlusIcon className="h-4 w-4" />
          New Site
        </button>
      </div>

      {sites.length === 0 && !loading ? (
        <div className="flex flex-col items-center justify-center rounded-xl border border-dashed border-zinc-800 py-20 text-center">
          {/* Three site tiles waiting to exist — the violet one is yours. */}
          <div aria-hidden className="relative h-16 w-28">
            <div className="absolute left-0 top-3 h-12 w-12 -rotate-6 rounded-xl border border-white/5 bg-zinc-900" />
            <div className="absolute right-0 top-3 h-12 w-12 rotate-6 rounded-xl border border-white/5 bg-zinc-900" />
            <div className="absolute left-8 top-0 flex h-12 w-12 rotate-3 items-center justify-center rounded-xl bg-gradient-to-br from-violet-500 to-violet-700 text-white shadow-[0_0_28px_rgba(108,92,231,0.35)]">
              <PlusIcon className="h-5 w-5" />
            </div>
          </div>
          <p className="mt-5 font-medium text-zinc-200">Create your first site</p>
          <p className="mt-1 text-sm text-zinc-600">
            {blueprints.length > 0
              ? `Blank WordPress, PHP or Docker — or one of your ${blueprints.length} blueprints.`
              : "WordPress, PHP or a Docker app — running in about a minute."}
          </p>
          <button
            onClick={() => setNewSiteOpen(true)}
            className="mt-4 flex items-center gap-1.5 rounded-md bg-violet-600 px-3.5 py-2 text-sm font-medium text-white hover:bg-violet-500"
          >
            <PlusIcon className="h-4 w-4" />
            New Site
          </button>
        </div>
      ) : siteView === "grid" ? (
        <GridView sites={sites} onClone={setCloneTarget} />
      ) : (
        <ListView sites={sites} onClone={setCloneTarget} />
      )}

      {cloneTarget && (
        <CloneSiteDialog
          source={{ id: cloneTarget.id, name: cloneTarget.name }}
          onClose={() => setCloneTarget(null)}
        />
      )}
    </div>
  );
}

/** The stack line for a site card/row — versions for WP, the app service for
 * a docker project (whose wp/php versions are empty), php alone for the
 * php/laravel kind (whose wp version is empty). */
function stackLabel(site: SiteWithStatus): string {
  if (site.kind === KIND_DOCKER) return `Docker · ${site.config.service}`;
  if (site.kind === KIND_PHP) return `PHP ${site.php_version}`;
  return `WP ${site.wp_version} · PHP ${site.php_version}`;
}

function useSiteActions(site: SiteWithStatus) {
  const busyId = useSites((s) => s.busyId);
  const start = useSites((s) => s.start);
  const stop = useSites((s) => s.stop);
  const resume = useSites((s) => s.resume);
  const remove = useSites((s) => s.remove);
  const navigate = useNav((s) => s.navigate);
  const router = useRouter((s) => s.status);
  const busy = busyId === site.id;
  // "up" covers degraded (containers running but unhealthy, plan 23): it can
  // still be opened and stopped, so it toggles and links like a running site.
  const up = site.live_status === "running" || site.live_status === "degraded";
  const url = siteUrl(site.slug, sitePort(site), router);

  return {
    busy,
    up,
    incomplete: !!site.incomplete,
    url,
    open: () => void openUrl(url),
    toggle: () => void (up ? stop(site.id) : start(site.id)),
    details: () => navigate({ name: "site", id: site.id }),
    resume: () => void resume(site.id),
    cleanup: () => {
      if (window.confirm(`Clean up "${site.name}"? Its half-created containers, database and files will be removed.`)) {
        void remove(site.id);
      }
    },
    remove: () => {
      if (window.confirm(`Delete "${site.name}"? This removes its containers, database and files.`)) {
        void remove(site.id);
      }
    },
  };
}

const ghostBtn =
  "inline-flex items-center gap-1.5 rounded-md border border-zinc-700 px-2.5 py-1 text-xs font-medium text-zinc-300 transition-colors hover:border-zinc-500 hover:bg-zinc-800/60 hover:text-zinc-100 disabled:opacity-50";
// The hero action on an up site — the one thing you most likely came to do.
const openBtn =
  "inline-flex items-center gap-1.5 rounded-md border border-violet-700/60 bg-violet-600/10 px-2.5 py-1 text-xs font-medium text-violet-300 transition-colors hover:bg-violet-600/25 hover:text-violet-200 disabled:opacity-50";
const resumeBtn =
  "inline-flex items-center gap-1.5 rounded-md bg-violet-600 px-2.5 py-1 text-xs font-medium text-white transition-colors hover:bg-violet-500 disabled:opacity-50";

/** Amber badge for a half-created site (plan 23), shown in place of the status. */
function IncompleteBadge() {
  return (
    <span className="inline-flex items-center gap-1.5 whitespace-nowrap rounded-full border border-amber-800 bg-amber-500/15 px-2.5 py-0.5 text-xs font-medium text-amber-400">
      <span className="h-1.5 w-1.5 animate-pulse rounded-full bg-current" />
      Setup incomplete
    </span>
  );
}

/** Resume / Clean up actions for a half-created site (plan 23). */
function IncompleteActions({ a }: { a: ReturnType<typeof useSiteActions> }) {
  return (
    <>
      <button onClick={a.resume} disabled={a.busy} className={resumeBtn}>
        <PlayIcon className="h-3.5 w-3.5" />
        Resume setup
      </button>
      <button onClick={a.cleanup} disabled={a.busy} className={dangerBtn}>
        <TrashIcon className="h-3.5 w-3.5" />
        Clean up
      </button>
    </>
  );
}

/**
 * The shared action row for a healthy site — identical buttons in grid cards
 * and list rows, so the two views never drift.
 */
function SiteActions({
  site,
  a,
  onClone,
}: {
  site: SiteWithStatus;
  a: ReturnType<typeof useSiteActions>;
  onClone: (site: SiteWithStatus) => void;
}) {
  const ToggleIcon = a.up ? StopSquareIcon : PlayIcon;
  return (
    <>
      {a.up && (
        <button onClick={a.open} className={openBtn}>
          <ArrowUpRightIcon className="h-3.5 w-3.5" />
          Open
        </button>
      )}
      <button
        onClick={a.toggle}
        disabled={a.busy || site.live_status === "creating"}
        className={ghostBtn}
      >
        <ToggleIcon className="h-3.5 w-3.5" />
        {a.up ? "Stop" : "Start"}
      </button>
      <button onClick={a.details} className={ghostBtn}>
        <WrenchIcon className="h-3.5 w-3.5" />
        Details
      </button>
      {/* Clone and Delete are icon-only: five labeled buttons wrap the card's
          action row, and these two are occasional actions whose glyphs (plus
          tooltip / confirm dialog) carry the meaning on their own. */}
      {site.capabilities.wp_tools && (
        <button
          onClick={() => onClone(site)}
          disabled={a.busy || site.live_status === "creating"}
          aria-label="Clone"
          title="Copy this site's database and files into a new site"
          className={ghostBtn}
        >
          <DuplicateIcon className="h-3.5 w-3.5" />
        </button>
      )}
      <button
        onClick={a.remove}
        disabled={a.busy}
        aria-label="Delete"
        title="Delete"
        className={dangerBtn}
      >
        <TrashIcon className="h-3.5 w-3.5" />
      </button>
    </>
  );
}

/**
 * Subtle marker on sites that came from a ServerKit server (plan 18). Renders
 * nothing for hand-made sites, and falls back to the raw connection id if the
 * connection has since been deleted — a site's origin is a fact about the
 * site, not a live lookup.
 */
function ImportedBadge({ site }: { site: SiteWithStatus }) {
  const connections = useServerKit((s) => s.connections);
  if (!site.connection_id) return null;
  const label = connections.find((c) => c.id === site.connection_id)?.label ?? site.connection_id;
  return (
    <span
      title={`Imported from ${label} (remote site #${site.remote_site_id})`}
      className="inline-flex shrink-0 items-center gap-1 text-xs text-zinc-600"
    >
      <LinkIcon className="h-3.5 w-3.5" />
      <span className="max-w-[9rem] truncate">{label}</span>
    </span>
  );
}
const dangerBtn =
  "inline-flex items-center gap-1.5 rounded-md border border-red-900 px-2.5 py-1 text-xs font-medium text-red-400 transition-colors hover:border-red-700 hover:bg-red-500/10 disabled:opacity-50";

function GridView({
  sites,
  onClone,
}: {
  sites: SiteWithStatus[];
  onClone: (site: SiteWithStatus) => void;
}) {
  return (
    <div className="grid grid-cols-1 gap-4 lg:grid-cols-2 xl:grid-cols-3">
      {sites.map((site) => (
        <GridCard key={site.id} site={site} onClone={onClone} />
      ))}
    </div>
  );
}

function GridCard({
  site,
  onClone,
}: {
  site: SiteWithStatus;
  onClone: (site: SiteWithStatus) => void;
}) {
  const a = useSiteActions(site);
  return (
    <div className="rounded-xl border border-zinc-800 bg-zinc-900/60 p-4 transition-all duration-150 hover:-translate-y-0.5 hover:border-zinc-700 hover:shadow-panel motion-reduce:transform-none motion-reduce:transition-none">
      <div className="flex items-start justify-between gap-2">
        <div className="flex min-w-0 items-center gap-3">
          <SiteTile name={site.name} slug={site.slug} status={site.live_status} />
          <div className="min-w-0">
            <div className="flex items-center gap-2">
              <button
                onClick={a.details}
                className="truncate text-left text-[15px] font-semibold tracking-tight text-white hover:text-violet-400"
              >
                {site.name}
              </button>
              <KindBadge kind={site.kind} />
            </div>
            <p className="truncate font-mono text-xs text-zinc-500">{a.url}</p>
          </div>
        </div>
        {a.incomplete ? <IncompleteBadge /> : <StatusBadge status={site.live_status} />}
      </div>
      <div className="mt-2.5 flex items-center gap-2 text-xs text-zinc-600">
        <span>{stackLabel(site)}</span>
        <ImportedBadge site={site} />
      </div>

      <div className="mt-3 flex flex-wrap gap-1.5">
        {a.incomplete ? (
          <IncompleteActions a={a} />
        ) : (
          <SiteActions site={site} a={a} onClone={onClone} />
        )}
      </div>
    </div>
  );
}

function ListView({
  sites,
  onClone,
}: {
  sites: SiteWithStatus[];
  onClone: (site: SiteWithStatus) => void;
}) {
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
            <ListRow key={site.id} site={site} onClone={onClone} />
          ))}
        </tbody>
      </table>
    </div>
  );
}

function ListRow({
  site,
  onClone,
}: {
  site: SiteWithStatus;
  onClone: (site: SiteWithStatus) => void;
}) {
  const a = useSiteActions(site);
  return (
    <tr className="border-b border-zinc-800/60 transition-colors last:border-0 hover:bg-zinc-900">
      <td className="px-4 py-2.5">
        <div className="flex items-center gap-2.5">
          <SiteTile name={site.name} slug={site.slug} status={site.live_status} size="sm" />
          <button
            onClick={a.details}
            className="font-medium tracking-tight text-white hover:text-violet-400"
          >
            {site.name}
          </button>
          <KindBadge kind={site.kind} />
          <ImportedBadge site={site} />
        </div>
      </td>
      <td className="px-4 py-2.5 font-mono text-xs text-zinc-500">{a.url}</td>
      <td className="px-4 py-2.5 text-xs text-zinc-500">{stackLabel(site)}</td>
      <td className="px-4 py-2.5">
        {a.incomplete ? <IncompleteBadge /> : <StatusBadge status={site.live_status} />}
      </td>
      <td className="px-4 py-2.5">
        <div className="flex justify-end gap-1.5">
          {a.incomplete ? (
            <IncompleteActions a={a} />
          ) : (
            <SiteActions site={site} a={a} onClone={onClone} />
          )}
        </div>
      </td>
    </tr>
  );
}
