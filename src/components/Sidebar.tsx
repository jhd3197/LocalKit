import { useNav } from "../stores/nav";

export default function Sidebar() {
  const page = useNav((s) => s.page);
  const navigate = useNav((s) => s.navigate);

  const item = (label: string, active: boolean, onClick: () => void) => (
    <button
      onClick={onClick}
      className={`w-full rounded-md px-3 py-2 text-left text-sm font-medium transition-colors ${
        active ? "bg-zinc-800 text-white" : "text-zinc-400 hover:bg-zinc-900 hover:text-zinc-200"
      }`}
    >
      {label}
    </button>
  );

  return (
    <aside className="flex w-52 flex-col border-r border-zinc-800 bg-zinc-900/40">
      <div className="flex items-center gap-2 border-b border-zinc-800 px-4 py-4">
        <span className="flex h-7 w-7 items-center justify-center rounded-md bg-emerald-500 text-sm font-bold text-zinc-950">
          LK
        </span>
        <span className="text-base font-semibold tracking-tight text-white">LocalKit</span>
      </div>
      <nav className="flex flex-col gap-1 p-3">
        {item("Sites", page.name === "sites" || page.name === "site", () => navigate({ name: "sites" }))}
        {item("Settings", page.name === "settings", () => navigate({ name: "settings" }))}
      </nav>
      <div className="mt-auto px-4 py-3 text-xs text-zinc-600">v0.1.0</div>
    </aside>
  );
}
