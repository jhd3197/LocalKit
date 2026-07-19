import { useNav } from "../stores/nav";
import { GearIcon } from "./icons";
import logo from "../assets/logo.png";

export default function Sidebar() {
  const page = useNav((s) => s.page);
  const navigate = useNav((s) => s.navigate);
  const settingsOpen = useNav((s) => s.settingsOpen);
  const setSettingsOpen = useNav((s) => s.setSettingsOpen);

  const sitesActive = (page.name === "sites" || page.name === "site") && !settingsOpen;

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
          className={`w-full rounded-md px-3 py-2 text-left text-sm font-medium transition-colors ${
            sitesActive
              ? "bg-zinc-800 text-white"
              : "text-zinc-400 hover:bg-zinc-900 hover:text-zinc-200"
          }`}
        >
          Sites
        </button>
      </nav>
      <div className="mt-auto flex items-center justify-between border-t border-zinc-800 px-3 py-2.5">
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
