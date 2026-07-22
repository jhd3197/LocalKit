import { useEffect, useRef, useState } from "react";
import { useSites } from "../stores/sites";
import {
  acquireTerminal,
  getTerminal,
  hasTerminal,
  restartTerminal,
  type TermState,
} from "../lib/terminalRegistry";
import type { SiteWithStatus } from "../lib/types";

/** Per-site embedded terminals: one tab per site, each a shell inside the
 *  site's WordPress container (`docker compose exec wordpress bash`).
 *  Terminals live in the registry, so scrollback survives page switches. */
export default function TerminalPage({ siteId }: { siteId?: string }) {
  const sites = useSites((s) => s.sites);
  const [activeId, setActiveId] = useState<string | null>(siteId ?? null);

  // Follow a requested site (e.g. from SiteDetail's Terminal button), else
  // fall back to the first site.
  useEffect(() => {
    if (siteId) setActiveId(siteId);
  }, [siteId]);
  useEffect(() => {
    if (!activeId || !sites.some((s) => s.id === activeId)) {
      setActiveId(sites[0]?.id ?? null);
    }
  }, [sites, activeId]);

  if (sites.length === 0) {
    return (
      <div className="flex h-full items-center justify-center p-8">
        <p className="text-sm text-zinc-500">No sites yet — create a site to open a terminal.</p>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col">
      {/* Tab bar — one tab per site */}
      <div className="flex shrink-0 items-center gap-1 overflow-x-auto border-b border-zinc-800 bg-zinc-900/40 px-3 pt-2">
        {sites.map((site) => {
          const active = site.id === activeId;
          const running = site.live_status === "running";
          return (
            <button
              key={site.id}
              onClick={() => setActiveId(site.id)}
              className={`flex shrink-0 items-center gap-2 rounded-t-lg border border-b-0 px-3.5 py-2 text-xs font-medium transition-colors ${
                active
                  ? "border-zinc-800 bg-zinc-950 text-zinc-50"
                  : "border-transparent text-zinc-500 hover:bg-zinc-900 hover:text-zinc-300"
              }`}
            >
              <span
                className={`h-1.5 w-1.5 rounded-full ${
                  running ? "bg-emerald-400" : "bg-zinc-600"
                } ${running && active ? "animate-pulse" : ""}`}
              />
              {site.name}
            </button>
          );
        })}
      </div>

      {/* Every opened terminal stays mounted; only the active one is visible. */}
      <div className="relative flex-1 overflow-hidden bg-[#08090E]">
        {sites.map((site) =>
          site.id === activeId || hasTerminal(site.id) ? (
            <SiteTerminal key={site.id} site={site} visible={site.id === activeId} />
          ) : null
        )}
      </div>
    </div>
  );
}

function SiteTerminal({ site, visible }: { site: SiteWithStatus; visible: boolean }) {
  const hostRef = useRef<HTMLDivElement>(null);
  const start = useSites((s) => s.start);
  const busyId = useSites((s) => s.busyId);
  const [state, setState] = useState<TermState>({ status: "opening", error: null, exitCode: null });
  const running = site.live_status === "running";
  const [, forceRender] = useState(0);

  // Acquire + attach the registry instance; detach (NEVER dispose) on unmount.
  useEffect(() => {
    if (!running) return;
    const host = hostRef.current;
    const entry = acquireTerminal(site.id);
    if (host) entry.attach(host);
    const unsub = entry.subscribe(setState);
    forceRender((n) => n + 1); // entry now exists — re-render to drop the placeholder
    return () => {
      if (host) entry.detach(host);
      unsub();
    };
  }, [site.id, running]);

  // Refit whenever the host resizes (window drag, sidebar, tab switch).
  useEffect(() => {
    const el = hostRef.current;
    if (!el) return;
    const ro = new ResizeObserver(() => getTerminal(site.id)?.refit());
    ro.observe(el);
    return () => ro.disconnect();
  }, [site.id]);

  // Fit + focus on becoming visible.
  useEffect(() => {
    if (!visible || !running) return;
    const raf = requestAnimationFrame(() => {
      const entry = getTerminal(site.id);
      if (!entry) return;
      entry.refit();
      entry.term.focus();
    });
    return () => cancelAnimationFrame(raf);
  }, [visible, running, site.id]);

  return (
    <div
      className={`absolute inset-0 flex flex-col ${visible ? "" : "invisible pointer-events-none"}`}
      aria-hidden={!visible}
    >
      <div ref={hostRef} className="min-h-0 w-full flex-1 overflow-hidden" />

      {!running && (
        <div className="absolute inset-0 flex flex-col items-center justify-center gap-3 bg-[#08090E]">
          <p className="text-sm text-zinc-500">
            {site.name} is stopped — start it to open a shell in its container.
          </p>
          <button
            onClick={() => void start(site.id)}
            disabled={busyId === site.id}
            className="rounded-md bg-violet-600 px-4 py-2 text-sm font-medium text-white hover:bg-violet-500 disabled:opacity-50"
          >
            Start site
          </button>
        </div>
      )}

      {running && state.error && (
        <div className="absolute inset-0 flex flex-col items-center justify-center gap-3 bg-[#08090E] px-8 text-center">
          <p className="max-w-md text-sm text-red-400">{state.error}</p>
          <button
            onClick={() => restartTerminal(site.id)}
            className="rounded-md border border-zinc-700 px-4 py-2 text-sm text-zinc-200 hover:border-zinc-500"
          >
            Retry
          </button>
        </div>
      )}

      {running && !state.error && state.status === "exited" && (
        <div className="absolute inset-x-0 bottom-0 flex items-center justify-between border-t border-zinc-800 bg-zinc-900/80 px-4 py-2">
          <span className="text-xs text-zinc-500">
            Session ended{state.exitCode !== null ? ` (exit ${state.exitCode})` : ""} — the site may
            have been stopped or restarted.
          </span>
          <button
            onClick={() => restartTerminal(site.id)}
            className="rounded-md border border-zinc-700 px-3 py-1 text-xs text-zinc-200 hover:border-zinc-500"
          >
            Reconnect
          </button>
        </div>
      )}
    </div>
  );
}
