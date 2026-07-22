import { useCallback, useEffect, useRef, useState } from "react";
import { ipc } from "../lib/ipc";
import { toastError } from "../lib/errors";
import { toast } from "../stores/toast";
import type { DebugStatus } from "../lib/types";
import SectionTitle from "./SectionTitle";
import { BugIcon } from "./icons";

/**
 * Tools → Debug (plan 24).
 *
 * Toggles WP_DEBUG + WP_DEBUG_LOG (errors go to wp-content/debug.log, never to
 * the screen) and tails that log in the same mono styling as the container-log
 * viewer. The log is a plain bind-mounted file, so reading and clearing it
 * never touches the container.
 */
export default function DebugPanel({ siteId }: { siteId: string }) {
  const [status, setStatus] = useState<DebugStatus | null>(null);
  const [log, setLog] = useState("");
  const [autoRefresh, setAutoRefresh] = useState(true);
  const [toggling, setToggling] = useState(false);
  const logRef = useRef<HTMLPreElement>(null);

  const loadStatus = useCallback(() => {
    ipc.siteDebugStatus(siteId).then(setStatus).catch(() => setStatus(null));
  }, [siteId]);
  const loadLog = useCallback(() => {
    ipc.readSiteDebugLog(siteId).then(setLog).catch(() => setLog(""));
  }, [siteId]);

  useEffect(() => {
    loadStatus();
    loadLog();
  }, [loadStatus, loadLog]);

  useEffect(() => {
    if (!autoRefresh) return;
    const t = setInterval(loadLog, 3000);
    return () => clearInterval(t);
  }, [autoRefresh, loadLog]);

  // Keep the log pinned to the newest lines.
  useEffect(() => {
    if (logRef.current) logRef.current.scrollTop = logRef.current.scrollHeight;
  }, [log]);

  const toggle = async () => {
    if (!status) return;
    setToggling(true);
    try {
      const next = await ipc.setSiteDebug(siteId, !status.enabled);
      setStatus(next);
      loadLog();
      toast.success(next.enabled ? "Debug mode on" : "Debug mode off");
    } catch (e) {
      toastError(e, "Toggle debug mode");
    } finally {
      setToggling(false);
    }
  };

  const clear = async () => {
    try {
      await ipc.clearSiteDebugLog(siteId);
      setLog("");
      loadStatus();
    } catch (e) {
      toastError(e, "Clear debug log");
    }
  };

  return (
    <section className="rounded-xl border border-zinc-800 bg-zinc-900/60 p-5">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <SectionTitle icon={BugIcon}>Debug</SectionTitle>
        <button
          onClick={() => void toggle()}
          disabled={!status || toggling}
          role="switch"
          aria-checked={status?.enabled ?? false}
          className={`relative inline-flex h-6 w-11 items-center rounded-full transition-colors disabled:opacity-50 ${
            status?.enabled ? "bg-violet-600" : "bg-zinc-700"
          }`}
          title={status?.enabled ? "Turn WP_DEBUG off" : "Turn WP_DEBUG on"}
        >
          <span
            className={`inline-block h-4 w-4 transform rounded-full bg-white transition-transform ${
              status?.enabled ? "translate-x-6" : "translate-x-1"
            }`}
          />
        </button>
      </div>

      <p className="mt-2 text-xs text-zinc-600">
        {status?.enabled
          ? "WP_DEBUG is on — errors are written to the log below, never shown on the page."
          : "Turn on WP_DEBUG to log PHP notices and errors to wp-content/debug.log (never to the screen)."}
      </p>

      <div className="mt-4 flex items-center justify-between">
        <h3 className="text-xs font-semibold uppercase tracking-wide text-zinc-600">
          debug.log{status ? ` · ${status.log_bytes} bytes` : ""}
        </h3>
        <div className="flex items-center gap-3 text-xs text-zinc-500">
          <label className="flex items-center gap-1.5">
            <input
              type="checkbox"
              checked={autoRefresh}
              onChange={(e) => setAutoRefresh(e.target.checked)}
              className="accent-violet-500"
            />
            Auto-refresh
          </label>
          <button onClick={loadLog} className="hover:text-zinc-300">
            Refresh
          </button>
          <button onClick={() => void clear()} className="text-red-400 hover:text-red-300">
            Clear log
          </button>
        </div>
      </div>

      <pre
        ref={logRef}
        className="mt-2 h-64 overflow-auto rounded-lg bg-zinc-950 p-3 font-mono text-xs leading-relaxed text-zinc-400"
      >
        {log || "No debug output yet."}
      </pre>
    </section>
  );
}
