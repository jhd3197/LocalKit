import { useEffect } from "react";
import { onSiteEvent } from "./lib/ipc";
import { useNav } from "./stores/nav";
import { useRouter } from "./stores/router";
import { useSites } from "./stores/sites";
import Sidebar from "./components/Sidebar";
import Dashboard from "./pages/Dashboard";
import SiteDetail from "./pages/SiteDetail";
import TerminalPage from "./pages/Terminal";
import Settings from "./pages/Settings";

export default function App() {
  const page = useNav((s) => s.page);
  const settingsOpen = useNav((s) => s.settingsOpen);
  const handleEvent = useSites((s) => s.handleEvent);
  const progress = useSites((s) => s.progress);
  const dismissProgress = useSites((s) => s.dismissProgress);
  const error = useSites((s) => s.error);
  const clearError = useSites((s) => s.clearError);

  useEffect(() => {
    void useSites.getState().refresh();
    void useRouter.getState().refresh();
    const unlisten = onSiteEvent(handleEvent);
    return () => {
      void unlisten.then((f) => f());
    };
  }, [handleEvent]);

  const inFlight = progress && progress.stage !== "done" && progress.stage !== "error";

  return (
    <div className="flex h-screen overflow-hidden bg-zinc-950 text-zinc-200">
      <Sidebar />
      <main className={`flex-1 ${page.name === "terminal" ? "overflow-hidden" : "overflow-y-auto"}`}>
        {page.name === "sites" && <Dashboard />}
        {page.name === "site" && <SiteDetail id={page.id} />}
        {page.name === "terminal" && <TerminalPage key={page.siteId ?? ""} siteId={page.siteId} />}
      </main>

      {settingsOpen && <Settings />}

      {/* Progress toast for long operations (site create) */}
      {progress && (
        <div
          className={`fixed bottom-4 right-4 z-50 flex max-w-sm items-start gap-3 rounded-lg border px-4 py-3 shadow-xl ${
            progress.stage === "error"
              ? "border-red-800 bg-red-950/90"
              : progress.stage === "done"
                ? "border-emerald-800 bg-emerald-950/90"
                : "border-zinc-700 bg-zinc-900/95"
          }`}
        >
          {inFlight && (
            <span className="mt-0.5 inline-block h-4 w-4 animate-spin rounded-full border-2 border-zinc-500 border-t-violet-400" />
          )}
          <p className="flex-1 text-sm">{progress.message}</p>
          <button
            onClick={dismissProgress}
            className="text-zinc-500 hover:text-zinc-300"
            aria-label="Dismiss"
          >
            ✕
          </button>
        </div>
      )}

      {/* Error toast */}
      {error && (
        <div className="fixed bottom-4 left-4 z-50 flex max-w-md items-start gap-3 rounded-lg border border-red-800 bg-red-950/90 px-4 py-3 shadow-xl">
          <p className="flex-1 text-sm text-red-200">{error}</p>
          <button onClick={clearError} className="text-red-400 hover:text-red-200" aria-label="Dismiss">
            ✕
          </button>
        </div>
      )}
    </div>
  );
}
