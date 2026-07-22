import { useDocker } from "../stores/docker";
import { useNav } from "../stores/nav";
import { useSites } from "../stores/sites";
import { GearIcon, LayersIcon, TerminalIcon } from "./icons";
import logo from "../assets/logo.png";

export default function Sidebar() {
  const page = useNav((s) => s.page);
  const navigate = useNav((s) => s.navigate);
  const settingsOpen = useNav((s) => s.settingsOpen);
  const setSettingsOpen = useNav((s) => s.setSettingsOpen);
  const docker = useDocker((s) => s.status);
  // Live "n up" pill on Sites — the sidebar answers "is anything running?"
  // without a trip to the dashboard. Selector returns a primitive, so no
  // re-render churn.
  const upCount = useSites(
    (s) =>
      s.sites.filter((x) => x.live_status === "running" || x.live_status === "degraded").length,
  );
  // Only claim "unavailable" once a check has actually returned (plan 23) — a
  // null status is "not checked yet", not "down".
  const dockerDown = docker != null && !docker.available;

  const sitesActive = (page.name === "sites" || page.name === "site") && !settingsOpen;
  const terminalActive = page.name === "terminal" && !settingsOpen;

  const navBtn = (active: boolean) =>
    `flex w-full items-center gap-2.5 rounded-md px-3 py-2 text-left text-sm font-medium transition-colors ${
      active ? "bg-zinc-800 text-white" : "text-zinc-400 hover:bg-zinc-900 hover:text-zinc-200"
    }`;

  return (
    <aside className="flex w-52 flex-col border-r border-zinc-800 bg-zinc-900/40">
      <div className="flex items-center gap-2.5 border-b border-zinc-800 px-4 py-4">
        <img src={logo} alt="LocalKit" className="h-7 w-7" />
        <span className="text-base font-semibold tracking-tight text-white">LocalKit</span>
      </div>
      <nav className="flex flex-col gap-1 p-3">
        <button
          onClick={() => {
            setSettingsOpen(false);
            navigate({ name: "sites" });
          }}
          className={navBtn(sitesActive)}
        >
          <LayersIcon className="h-4 w-4" />
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
        </button>
        <button
          onClick={() => {
            setSettingsOpen(false);
            navigate({ name: "terminal" });
          }}
          className={navBtn(terminalActive)}
        >
          <TerminalIcon className="h-4 w-4" />
          Terminal
        </button>
      </nav>
      {dockerDown && (
        <button
          onClick={() => setSettingsOpen(true)}
          title={docker?.error ?? "Docker is not available."}
          className="mt-auto mx-3 mb-1 flex items-center gap-2 rounded-md border border-amber-800 bg-amber-500/10 px-2.5 py-2 text-left text-xs font-medium text-amber-400 hover:bg-amber-500/15"
        >
          <span className="h-1.5 w-1.5 shrink-0 animate-pulse rounded-full bg-current" />
          <span className="truncate">Docker unavailable</span>
        </button>
      )}
      <div
        className={`${dockerDown ? "" : "mt-auto"} flex items-center justify-between border-t border-zinc-800 px-3 py-2.5`}
      >
        <span className="px-1 text-xs text-zinc-600">v0.1.0</span>
        <button
          onClick={() => setSettingsOpen(true)}
          aria-label="Settings"
          title="Settings"
          className={`rounded-md p-1.5 transition-colors ${
            settingsOpen ? "bg-zinc-800 text-violet-400" : "text-zinc-500 hover:bg-zinc-900 hover:text-zinc-300"
          }`}
        >
          <GearIcon className="h-[18px] w-[18px]" />
        </button>
      </div>
    </aside>
  );
}
