import { useDocker } from "../stores/docker";
import { useNav, type SiteTab } from "../stores/nav";
import { useSites } from "../stores/sites";
import { useRailCollapsed } from "../stores/settings";
import type { SiteWithStatus } from "../lib/types";
import SiteTile from "./SiteTile";
import {
  CameraIcon,
  ChevronsLeftIcon,
  ChevronsRightIcon,
  FileTextIcon,
  GearIcon,
  GlobeIcon,
  HomeIcon,
  LayersIcon,
  TerminalIcon,
  WrenchIcon,
} from "./icons";
import logo from "../assets/logo.png";

/**
 * The control rail (plan 28) — nav plus a live list of every site with its
 * tile and status dot, so "is anything running?" is answered without going
 * anywhere. Collapsible to a tile-only strip (Discord-style); the choice
 * persists in the settings KV. The active site expands sub-links into the
 * detail page's tabs.
 */

const DOT: Record<string, string> = {
  running: "bg-emerald-400",
  degraded: "bg-amber-400",
  creating: "bg-amber-400",
  error: "bg-red-400",
  stopped: "bg-zinc-600",
};

export default function Sidebar() {
  const page = useNav((s) => s.page);
  const navigate = useNav((s) => s.navigate);
  const settingsOpen = useNav((s) => s.settingsOpen);
  const setSettingsOpen = useNav((s) => s.setSettingsOpen);
  const docker = useDocker((s) => s.status);
  const sites = useSites((s) => s.sites);
  const [collapsed, setCollapsed] = useRailCollapsed();
  // Only claim "unavailable" once a check has actually returned (plan 23) — a
  // null status is "not checked yet", not "down".
  const dockerDown = docker != null && !docker.available;

  const homeActive = page.name === "home" && !settingsOpen;
  const sitesActive = page.name === "sites" && !settingsOpen;
  const terminalActive = page.name === "terminal" && !settingsOpen;
  const activeSiteId = page.name === "site" ? page.id : null;
  const upCount = sites.filter(
    (x) => x.live_status === "running" || x.live_status === "degraded",
  ).length;

  const navBtn = (active: boolean) =>
    `flex w-full items-center gap-2.5 rounded-md px-2.5 py-2 text-left text-sm font-medium transition-colors ${
      active ? "bg-zinc-800 text-zinc-50" : "text-zinc-400 hover:bg-zinc-900 hover:text-zinc-200"
    } ${collapsed ? "justify-center px-0" : ""}`;

  const go = (p: Parameters<typeof navigate>[0]) => {
    setSettingsOpen(false);
    navigate(p);
  };

  return (
    <aside
      className={`flex shrink-0 flex-col border-r border-zinc-800 bg-zinc-900/40 transition-[width] duration-150 motion-reduce:transition-none ${
        collapsed ? "w-[60px]" : "w-56"
      }`}
    >
      <div
        className={`flex items-center border-b border-zinc-800 px-3 py-3.5 ${
          collapsed ? "justify-center" : "gap-2.5"
        }`}
      >
        <img src={logo} alt="LocalKit" className="h-7 w-7 shrink-0" />
        {!collapsed && (
          <span className="text-base font-semibold tracking-tight text-zinc-50">LocalKit</span>
        )}
      </div>

      <nav className={`flex flex-col gap-1 p-2 ${collapsed ? "items-stretch" : ""}`}>
        <button
          onClick={() => go({ name: "home" })}
          aria-label="Home"
          title="Home"
          className={navBtn(homeActive)}
        >
          <HomeIcon className="h-4 w-4 shrink-0" />
          {!collapsed && "Home"}
        </button>
        <button
          onClick={() => go({ name: "sites" })}
          aria-label="Sites"
          title="Sites"
          className={navBtn(sitesActive)}
        >
          <LayersIcon className="h-4 w-4 shrink-0" />
          {!collapsed && (
            <>
              Sites
              {upCount > 0 && (
                <span
                  title={`${upCount} running`}
                  className="ml-auto inline-flex items-center gap-1 rounded-full bg-emerald-500/15 px-1.5 py-0.5 text-[10px] font-semibold text-emerald-400"
                >
                  <span className="h-1 w-1 rounded-full bg-current" />
                  {upCount}
                </span>
              )}
            </>
          )}
        </button>
        <button
          onClick={() => go({ name: "terminal" })}
          aria-label="Terminal"
          title="Terminal"
          className={navBtn(terminalActive)}
        >
          <TerminalIcon className="h-4 w-4 shrink-0" />
          {!collapsed && "Terminal"}
        </button>
      </nav>

      {/* Live site rail */}
      {sites.length > 0 && (
        <div className="min-h-0 flex-1 overflow-y-auto border-t border-zinc-800 p-2">
          {!collapsed && (
            <p className="px-2 pb-1.5 pt-1 text-[10px] font-semibold uppercase tracking-widest text-zinc-600">
              Your sites
            </p>
          )}
          <div className="flex flex-col gap-0.5">
            {sites.map((site) => (
              <RailSite
                key={site.id}
                site={site}
                active={activeSiteId === site.id && !settingsOpen}
                activeTab={page.name === "site" ? page.tab ?? "overview" : null}
                collapsed={collapsed}
                onOpen={(tab) => go({ name: "site", id: site.id, tab })}
              />
            ))}
          </div>
        </div>
      )}
      {sites.length === 0 && <div className="flex-1" />}

      {dockerDown && !collapsed && (
        <button
          onClick={() => setSettingsOpen(true)}
          title={docker?.error ?? "Docker is not available."}
          className="mx-3 mb-1 flex items-center gap-2 rounded-md border border-amber-800 bg-amber-500/10 px-2.5 py-2 text-left text-xs font-medium text-amber-400 hover:bg-amber-500/15"
        >
          <span className="h-1.5 w-1.5 shrink-0 animate-pulse rounded-full bg-current" />
          <span className="truncate">Docker unavailable</span>
        </button>
      )}

      <div
        className={`flex items-center border-t border-zinc-800 px-2 py-2 ${
          collapsed ? "flex-col gap-1" : "justify-between"
        }`}
      >
        {!collapsed && <span className="px-1.5 text-xs text-zinc-600">v0.1.0</span>}
        <div className={`flex items-center ${collapsed ? "flex-col gap-1" : "gap-0.5"}`}>
          <button
            onClick={() => setCollapsed(!collapsed)}
            aria-label={collapsed ? "Expand sidebar" : "Collapse sidebar"}
            title={collapsed ? "Expand sidebar" : "Collapse sidebar"}
            className="rounded-md p-1.5 text-zinc-500 transition-colors hover:bg-zinc-900 hover:text-zinc-300"
          >
            {collapsed ? (
              <ChevronsRightIcon className="h-4 w-4" />
            ) : (
              <ChevronsLeftIcon className="h-4 w-4" />
            )}
          </button>
          <button
            onClick={() => setSettingsOpen(true)}
            aria-label="Settings"
            title={dockerDown && collapsed ? "Settings — Docker unavailable" : "Settings"}
            className={`relative rounded-md p-1.5 transition-colors ${
              settingsOpen
                ? "bg-zinc-800 text-violet-400"
                : "text-zinc-500 hover:bg-zinc-900 hover:text-zinc-300"
            }`}
          >
            <GearIcon className="h-[18px] w-[18px]" />
            {dockerDown && collapsed && (
              <span className="absolute right-0.5 top-0.5 h-1.5 w-1.5 animate-pulse rounded-full bg-amber-400" />
            )}
          </button>
        </div>
      </div>
    </aside>
  );
}

/** Sub-links of the active site — the detail page's tabs, capability-gated. */
function subTabs(site: SiteWithStatus): { tab: SiteTab; label: string; icon: typeof GlobeIcon }[] {
  const caps = site.capabilities;
  const tabs: { tab: SiteTab; label: string; icon: typeof GlobeIcon }[] = [
    { tab: "overview", label: "Overview", icon: GlobeIcon },
  ];
  if (caps.db_gui || caps.search_replace || caps.wp_tools)
    tabs.push({ tab: "tools", label: "Tools", icon: WrenchIcon });
  if (caps.snapshots) tabs.push({ tab: "snapshots", label: "Snapshots", icon: CameraIcon });
  tabs.push({ tab: "logs", label: "Logs", icon: FileTextIcon });
  return tabs;
}

function RailSite({
  site,
  active,
  activeTab,
  collapsed,
  onOpen,
}: {
  site: SiteWithStatus;
  active: boolean;
  activeTab: SiteTab | null;
  collapsed: boolean;
  onOpen: (tab?: SiteTab) => void;
}) {
  const status = site.incomplete ? "error" : site.live_status;
  if (collapsed) {
    return (
      <button
        onClick={() => onOpen()}
        title={site.name}
        className={`flex justify-center rounded-lg p-1.5 transition-colors ${
          active ? "bg-zinc-800" : "hover:bg-zinc-900"
        }`}
      >
        <SiteTile name={site.name} slug={site.slug} status={site.live_status} size="sm" />
      </button>
    );
  }
  return (
    <div>
      <button
        onClick={() => onOpen()}
        className={`flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left transition-colors ${
          active ? "bg-zinc-800" : "hover:bg-zinc-900"
        }`}
      >
        <SiteTile name={site.name} slug={site.slug} status={site.live_status} size="sm" />
        <span
          className={`min-w-0 flex-1 truncate text-[13px] font-medium ${
            active ? "text-zinc-50" : "text-zinc-300"
          }`}
        >
          {site.name}
        </span>
        <span
          title={site.incomplete ? "Setup incomplete" : site.live_status}
          className={`h-1.5 w-1.5 shrink-0 rounded-full ${DOT[status] ?? DOT.stopped} ${
            status === "creating" || status === "degraded" ? "animate-pulse" : ""
          }`}
        />
      </button>
      {active && !site.incomplete && (
        <div className="mb-1 ml-[26px] mt-0.5 flex flex-col gap-0.5 border-l border-zinc-800 pl-2">
          {subTabs(site).map(({ tab, label, icon: Icon }) => (
            <button
              key={tab}
              onClick={() => onOpen(tab)}
              className={`flex items-center gap-2 rounded px-2 py-1 text-left text-xs transition-colors ${
                activeTab === tab
                  ? "bg-violet-500/15 text-violet-400"
                  : "text-zinc-500 hover:bg-zinc-900 hover:text-zinc-300"
              }`}
            >
              <Icon className="h-3 w-3" />
              {label}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
