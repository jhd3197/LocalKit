import { useEffect } from "react";
import { onSiteEvent, onSitesChanged } from "./lib/ipc";
import { useNav } from "./stores/nav";
import { useRouter } from "./stores/router";
import { useSites } from "./stores/sites";
import { useShortcuts } from "./hooks/useShortcuts";
import Sidebar from "./components/Sidebar";
import Toasts from "./components/Toasts";
import CommandPalette from "./components/CommandPalette";
import KeyboardShortcutsDialog from "./components/KeyboardShortcutsDialog";
import NewSiteDialog from "./components/NewSiteDialog";
import ImportSiteDialog from "./components/ImportSiteDialog";
import Dashboard from "./pages/Dashboard";
import SiteDetail from "./pages/SiteDetail";
import TerminalPage from "./pages/Terminal";
import Settings from "./pages/Settings";

export default function App() {
  const page = useNav((s) => s.page);
  const settingsOpen = useNav((s) => s.settingsOpen);
  const newSiteOpen = useNav((s) => s.newSiteOpen);
  const setNewSiteOpen = useNav((s) => s.setNewSiteOpen);
  const handleEvent = useSites((s) => s.handleEvent);
  useShortcuts();

  useEffect(() => {
    void useSites.getState().refresh();
    void useRouter.getState().refresh();
    const unlisten = onSiteEvent(handleEvent);
    // The reconciler settles status in the background; re-fetch when it does
    // so an external stop/start corrects itself without a manual refresh.
    const unlistenChanged = onSitesChanged(() => void useSites.getState().refresh());
    return () => {
      void unlisten.then((f) => f());
      void unlistenChanged.then((f) => f());
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
      {newSiteOpen && <NewSiteDialog onClose={() => setNewSiteOpen(false)} />}
      {/* Opened from Settings → ServerKit, but rendered here so the import
          keeps running (and the dialog keeps reporting) if Settings closes. */}
      <ImportSiteDialog />
      <CommandPalette />
      <KeyboardShortcutsDialog />

      <Toasts />
    </div>
  );
}
