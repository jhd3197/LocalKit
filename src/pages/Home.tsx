import { useEffect } from "react";
import { useNav } from "../stores/nav";
import { useDocker } from "../stores/docker";
import { useRouter } from "../stores/router";
import { useServerKit } from "../stores/serverkit";
import { useBlueprints } from "../stores/blueprints";
import { useSites } from "../stores/sites";
import type { SiteWithStatus } from "../lib/types";
import SiteTile from "../components/SiteTile";
import {
  ArrowUpRightIcon,
  BookmarkIcon,
  GlobeIcon,
  LayersIcon,
  PlusIcon,
  ServerIcon,
  TerminalIcon,
} from "../components/icons";

/**
 * Home (plan 28) — the screen you walk up to the machine to look at. Status
 * headline, the wall of site tiles (glow = running), what needs attention,
 * and the environment at a glance. Everything reads from existing stores;
 * management stays on the Sites page.
 */
export default function Home() {
  const sites = useSites((s) => s.sites);
  const docker = useDocker((s) => s.status);
  const router = useRouter((s) => s.status);
  const connections = useServerKit((s) => s.connections);
  const refreshConnections = useServerKit((s) => s.refresh);
  const blueprints = useBlueprints((s) => s.blueprints);
  const refreshBlueprints = useBlueprints((s) => s.refresh);
  const navigate = useNav((s) => s.navigate);
  const setNewSiteOpen = useNav((s) => s.setNewSiteOpen);
  const openSettings = useNav((s) => s.openSettings);

  useEffect(() => {
    void refreshConnections();
    void refreshBlueprints();
  }, [refreshConnections, refreshBlueprints]);

  const up = sites.filter(
    (s) => s.live_status === "running" || s.live_status === "degraded",
  ).length;
  const attention = sites.filter(
    (s) => s.incomplete || s.live_status === "degraded" || s.live_status === "error",
  );

  const headline =
    sites.length === 0
      ? "Let's create your first site."
      : up === 0
        ? "All quiet — every site is stopped."
        : up === sites.length
          ? sites.length === 1
            ? "Your site is running."
            : `All ${sites.length} sites are running.`
          : `${up} of ${sites.length} sites running.`;

  return (
    <div className="mx-auto max-w-5xl p-8">
      {/* Hero */}
      <div className="mt-2 flex flex-wrap items-end justify-between gap-4">
        <div>
          <p className="text-[11px] font-semibold uppercase tracking-widest text-violet-400">
            Local environment
          </p>
          <h1 className="mt-1.5 text-3xl font-semibold tracking-tight text-zinc-50">{headline}</h1>
          <p className="mt-2 text-sm text-zinc-500">
            {docker == null
              ? "Checking Docker…"
              : docker.available
                ? "Docker is healthy"
                : "Docker is unavailable"}
            {" · "}
            {router?.enabled
              ? router.conflicts.length > 0
                ? "local domains blocked"
                : "local domains on"
              : "local domains off"}
            {connections.length > 0 &&
              ` · ${connections.length} ServerKit ${connections.length === 1 ? "connection" : "connections"}`}
          </p>
        </div>
        <button
          onClick={() => setNewSiteOpen(true)}
          className="flex items-center gap-1.5 rounded-md bg-violet-600 px-3.5 py-2 text-sm font-medium text-white transition-colors hover:bg-violet-500"
        >
          <PlusIcon className="h-4 w-4" />
          New Site
        </button>
      </div>

      {/* Tile wall */}
      {sites.length > 0 && (
        <div className="mt-8 flex flex-wrap gap-3">
          {sites.map((site) => (
            <WallTile key={site.id} site={site} onOpen={() => navigate({ name: "site", id: site.id })} />
          ))}
        </div>
      )}

      <div className="mt-8 grid grid-cols-1 gap-4 lg:grid-cols-2">
        {/* Needs attention — the whole reason to glance at this screen. */}
        {attention.length > 0 && (
          <section className="rounded-2xl border border-amber-900/50 bg-amber-500/[0.04] p-5">
            <h2 className="flex items-center gap-2 text-sm font-semibold uppercase tracking-wide text-amber-400/90">
              <span className="h-1.5 w-1.5 animate-pulse rounded-full bg-amber-400" />
              Needs attention
            </h2>
            <div className="mt-3 flex flex-col gap-1">
              {attention.map((site) => (
                <button
                  key={site.id}
                  onClick={() => navigate({ name: "site", id: site.id })}
                  className="flex items-center gap-2.5 rounded-lg px-2 py-1.5 text-left transition-colors hover:bg-zinc-900/60"
                >
                  <SiteTile name={site.name} slug={site.slug} status={site.live_status} size="sm" />
                  <span className="min-w-0 flex-1 truncate text-sm font-medium text-zinc-50">
                    {site.name}
                  </span>
                  <span className="shrink-0 text-xs text-amber-400">
                    {site.incomplete
                      ? "setup incomplete"
                      : site.live_status === "degraded"
                        ? "unhealthy"
                        : "error"}
                  </span>
                </button>
              ))}
            </div>
          </section>
        )}

        {/* Environment */}
        <section className="rounded-2xl border border-zinc-800 bg-zinc-900/60 p-5">
          <h2 className="text-sm font-semibold uppercase tracking-wide text-zinc-500">
            Environment
          </h2>
          <dl className="mt-3 space-y-2.5 text-sm">
            <EnvRow
              ok={docker?.available ?? null}
              label="Docker"
              value={
                docker == null
                  ? "checking…"
                  : docker.available
                    ? docker.version
                      ? `running · server ${docker.version}`
                      : "running"
                    : (docker.error ?? "unavailable")
              }
            />
            <EnvRow
              ok={router?.enabled ? router.conflicts.length === 0 : null}
              label="Local domains"
              value={
                router?.enabled
                  ? router.conflicts.length > 0
                    ? "ports blocked by another app"
                    : "sites served at <slug>.test"
                  : "off — sites use localhost ports"
              }
            />
            <EnvRow
              ok={connections.length > 0 ? true : null}
              label="ServerKit"
              value={
                connections.length > 0
                  ? connections.map((c) => c.label).join(", ")
                  : "no connections yet"
              }
            />
            <EnvRow
              ok={blueprints.length > 0 ? true : null}
              label="Blueprints"
              value={
                blueprints.length > 0
                  ? `${blueprints.length} ready to reuse`
                  : "save a site as a blueprint to reuse its stack"
              }
            />
          </dl>
        </section>

        {/* Quick actions */}
        <section className="rounded-2xl border border-zinc-800 bg-zinc-900/60 p-5 lg:col-span-1">
          <h2 className="text-sm font-semibold uppercase tracking-wide text-zinc-500">
            Quick actions
          </h2>
          <div className="mt-3 grid grid-cols-2 gap-2">
            <QuickAction icon={LayersIcon} label="Browse sites" onClick={() => navigate({ name: "sites" })} />
            <QuickAction icon={TerminalIcon} label="Open terminal" onClick={() => navigate({ name: "terminal" })} />
            <QuickAction icon={ServerIcon} label="ServerKit" onClick={() => openSettings("serverkit")} />
            <QuickAction icon={GlobeIcon} label="Local domains" onClick={() => openSettings("domains")} />
            <QuickAction icon={BookmarkIcon} label="New from blueprint" onClick={() => setNewSiteOpen(true)} />
            <QuickAction icon={ArrowUpRightIcon} label="New site" onClick={() => setNewSiteOpen(true)} />
          </div>
        </section>
      </div>
    </div>
  );
}

function WallTile({ site, onOpen }: { site: SiteWithStatus; onOpen: () => void }) {
  return (
    <button
      onClick={onOpen}
      title={`${site.name} — ${site.incomplete ? "setup incomplete" : site.live_status}`}
      className="group flex w-[104px] flex-col items-center gap-2 rounded-xl p-2 transition-all duration-150 hover:-translate-y-0.5 hover:bg-zinc-900/60 motion-reduce:transform-none motion-reduce:transition-none"
    >
      <SiteTile name={site.name} slug={site.slug} status={site.live_status} kind={site.kind} size="lg" />
      <span className="w-full truncate text-center text-xs font-medium text-zinc-400 group-hover:text-zinc-200">
        {site.name}
      </span>
    </button>
  );
}

function EnvRow({ ok, label, value }: { ok: boolean | null; label: string; value: string }) {
  return (
    <div className="flex items-center gap-2.5">
      <span
        className={`h-2 w-2 shrink-0 rounded-full ${
          ok === null ? "bg-zinc-600" : ok ? "bg-emerald-400" : "bg-amber-400"
        }`}
      />
      <dt className="w-28 shrink-0 text-zinc-500">{label}</dt>
      <dd className="min-w-0 flex-1 truncate text-zinc-300" title={value}>
        {value}
      </dd>
    </div>
  );
}

function QuickAction({
  icon: Icon,
  label,
  onClick,
}: {
  icon: typeof LayersIcon;
  label: string;
  onClick: () => void;
}) {
  return (
    <button
      onClick={onClick}
      className="flex items-center gap-2.5 rounded-lg border border-zinc-800 bg-zinc-950/40 px-3 py-2.5 text-left text-sm font-medium text-zinc-300 transition-colors hover:border-zinc-700 hover:bg-zinc-900 hover:text-zinc-100"
    >
      <Icon className="h-4 w-4 shrink-0 text-zinc-500" />
      <span className="truncate">{label}</span>
    </button>
  );
}
