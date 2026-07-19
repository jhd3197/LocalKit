import { useEffect } from "react";
import { onSiteEvent } from "./lib/ipc";
import { useNav } from "./stores/nav";
import { useRouter } from "./stores/router";
import { useSites } from "./stores/sites";
import Sidebar from "./components/Sidebar";
import Toasts from "./components/Toasts";
import Dashboard from "./pages/Dashboard";
import SiteDetail from "./pages/SiteDetail";
import TerminalPage from "./pages/Terminal";
import Settings from "./pages/Settings";

export default function App() {
  const page = useNav((s) => s.page);
  const settingsOpen = useNav((s) => s.settingsOpen);
  const handleEvent = useSites((s) => s.handleEvent);

  useEffect(() => {
    void useSites.getState().refresh();
    void useRouter.getState().refresh();
    const unlisten = onSiteEvent(handleEvent);
    return () => {
      void unlisten.then((f) => f());
    };
  }, [handleEvent]);

  return (
    <div className="flex h-screen overflow-hidden bg-zinc-950 text-zinc-200">
      <Sidebar />
      <main className={`flex-1 ${page.name === "terminal" ? "overflow-hidden" : "overflow-y-auto"}`}>
        {page.name === "sites" && <Dashboard />}
        {page.name === "site" && <SiteDetail id={page.id} />}
        {page.name === "terminal" && <TerminalPage key={page.siteId ?? ""} siteId={page.siteId} />}
      </main>

      {settingsOpen && <Settings />}

      <Toasts />
    </div>
  );
}
