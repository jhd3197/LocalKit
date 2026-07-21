import { create } from "zustand";
import { ipc } from "../lib/ipc";
import { errMsg, markEventError, toastError } from "../lib/errors";
import { toast, useToast, type ToastKind } from "./toast";
import type { Site, SiteEvent, SiteWithStatus, WpInfo } from "../lib/types";

interface SitesState {
  sites: SiteWithStatus[];
  loading: boolean;
  creating: boolean;
  busyId: string | null;
  /** Last site-event received (drives the progress toast + sync refreshes). */
  progress: SiteEvent | null;
  logs: Record<string, string>;
  wpInfo: Record<string, WpInfo | null>;
  refresh: () => Promise<void>;
  createSite: (name: string, wpVersion: string, phpVersion: string) => Promise<Site>;
  cloneSite: (id: string, newName: string) => Promise<Site>;
  start: (id: string) => Promise<void>;
  stop: (id: string) => Promise<void>;
  restart: (id: string) => Promise<void>;
  resume: (id: string) => Promise<void>;
  remove: (id: string, deleteSnapshots?: boolean) => Promise<void>;
  fetchLogs: (id: string) => Promise<void>;
  fetchWpInfo: (id: string) => Promise<void>;
  handleEvent: (event: SiteEvent) => void;
}

// The pinned progress toast currently tracking site-event stages (create,
// push/pull). One at a time, like the single `progress` state before it.
let progressToastId: number | null = null;

function siteName(sites: SiteWithStatus[], id: string): string | undefined {
  return sites.find((s) => s.id === id)?.name;
}

/** Terminal stages: the ones that resolve the pinned progress toast. */
const TERMINAL: Record<string, ToastKind> = {
  done: "success",
  error: "error",
  // A cancel is deliberate, so it resolves neutral rather than red (plan 19).
  cancelled: "info",
};

/**
 * Has a site-event stream reached its end?
 *
 * Exported so every listener agrees on what "finished" means. Components that
 * hardcoded `done | error` silently stopped resetting themselves the moment
 * `cancelled` was added — one list, one source of truth.
 */
export function isTerminalStage(stage: string): boolean {
  return stage in TERMINAL;
}

/**
 * Byte count for a progress line. Mirrors `transfer::human_bytes` on the Rust
 * side so the CLI's stderr output and the GUI toast read identically.
 */
function humanBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let value = n / 1024;
  let unit = 0;
  while (value >= 1024 && unit + 1 < units.length) {
    value /= 1024;
    unit += 1;
  }
  return `${value >= 100 ? value.toFixed(0) : value.toFixed(1)} ${units[unit]}`;
}

/**
 * Title for a progress toast: "Pushing wp-content — 148 MB / 312 MB" during a
 * chunked transfer, the bare stage message everywhere else. The backend sends
 * counters, not prose, so this is the only place the readout is composed.
 */
function progressTitle(event: SiteEvent): string {
  const { bytes_done, bytes_total } = event;
  if (bytes_done == null || !bytes_total) return event.message;
  return `${event.message} — ${humanBytes(bytes_done)} / ${humanBytes(bytes_total)}`;
}

export const useSites = create<SitesState>((set, get) => ({
  sites: [],
  loading: false,
  creating: false,
  busyId: null,
  progress: null,
  logs: {},
  wpInfo: {},

  refresh: async () => {
    set({ loading: true });
    try {
      const sites = await ipc.listSites();
      set({ sites, loading: false });
    } catch (e) {
      set({ loading: false });
      toastError(e, "Refresh sites");
    }
  },

  createSite: async (name, wpVersion, phpVersion) => {
    set({ creating: true, progress: null });
    try {
      const site = await ipc.createSite(name, wpVersion, phpVersion);
      await get().refresh();
      return site;
    } catch (e) {
      toastError(e, "Create site");
      throw e;
    } finally {
      set({ creating: false });
    }
  },

  cloneSite: async (id, newName) => {
    // The clone streams the same site-event stages as create, so the pinned
    // progress toast covers feedback; the dialog owns its own submitting flag.
    set({ progress: null });
    try {
      const site = await ipc.cloneSite(id, newName);
      await get().refresh();
      return site;
    } catch (e) {
      toastError(e, "Clone site");
      throw e;
    }
  },

  start: async (id) => {
    set({ busyId: id });
    try {
      await ipc.startSite(id);
      await get().refresh();
      toast.success("Site started", siteName(get().sites, id));
    } catch (e) {
      toastError(e, "Start site");
    } finally {
      set({ busyId: null });
    }
  },

  stop: async (id) => {
    set({ busyId: id });
    try {
      await ipc.stopSite(id);
      await get().refresh();
      toast.success("Site stopped", siteName(get().sites, id));
    } catch (e) {
      toastError(e, "Stop site");
    } finally {
      set({ busyId: null });
    }
  },

  restart: async (id) => {
    set({ busyId: id });
    try {
      await ipc.restartSite(id);
      await get().refresh();
      toast.success("Site restarted", siteName(get().sites, id));
    } catch (e) {
      toastError(e, "Restart site");
    } finally {
      set({ busyId: null });
    }
  },

  resume: async (id) => {
    // Resume streams the same site-event stages as create, so the pinned
    // progress toast covers feedback; busyId disables the card while it runs.
    set({ busyId: id });
    try {
      await ipc.resumeSite(id);
      await get().refresh();
    } catch (e) {
      toastError(e, "Resume setup");
    } finally {
      set({ busyId: null });
    }
  },

  remove: async (id, deleteSnapshots = false) => {
    set({ busyId: id });
    try {
      const name = siteName(get().sites, id);
      await ipc.deleteSite(id, deleteSnapshots);
      await get().refresh();
      toast.success(
        "Site deleted",
        // The kept snapshot is the whole point of plan 17 — say so, or the
        // user has no reason to believe the delete was reversible.
        deleteSnapshots ? name : name && `${name} — a snapshot was kept`
      );
    } catch (e) {
      toastError(e, "Delete site");
    } finally {
      set({ busyId: null });
    }
  },

  fetchLogs: async (id) => {
    try {
      const text = await ipc.siteLogs(id);
      set((s) => ({ logs: { ...s.logs, [id]: text } }));
    } catch (e) {
      set((s) => ({ logs: { ...s.logs, [id]: `Error fetching logs: ${errMsg(e)}` } }));
    }
  },

  fetchWpInfo: async (id) => {
    try {
      const info = await ipc.wpCliInfo(id);
      set((s) => ({ wpInfo: { ...s.wpInfo, [id]: info } }));
    } catch {
      set((s) => ({ wpInfo: { ...s.wpInfo, [id]: null } }));
    }
  },

  handleEvent: (event) => {
    set({ progress: event });
    const terminal = TERMINAL[event.stage];
    if (terminal) {
      if (progressToastId != null) {
        toast.resolve(progressToastId, terminal, event.message);
        progressToastId = null;
      } else {
        toast[terminal](event.message);
      }
      // Both error and cancel reject the command promise as well; dedupe so
      // the user sees one message, not two.
      if (terminal !== "success") markEventError(event.message);
      void get().refresh();
      return;
    }

    // Only a byte-carrying stage is cancellable: those are the chunked
    // transfers, which stop cleanly between chunks. Offering Cancel on
    // `docker compose up` would be a button that does nothing.
    //
    // Set on every event, not just the first: a push opens its toast on
    // "Bundling wp-content..." (no bytes) and only becomes cancellable once
    // chunks start flowing — and stops being cancellable again when the
    // server moves on to importing.
    const action = event.bytes_total
      ? { label: "Cancel", onClick: () => void ipc.cancelSync(event.id).catch(() => {}) }
      : undefined;
    const title = progressTitle(event);

    if (progressToastId != null) {
      useToast.getState().update(progressToastId, { title, action });
    } else {
      progressToastId = toast.progress(title, action);
    }
  },
}));
