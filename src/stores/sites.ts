import { create } from "zustand";
import { ipc } from "../lib/ipc";
import { errMsg, markEventError, toastError } from "../lib/errors";
import { toast, useToast } from "./toast";
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
  start: (id: string) => Promise<void>;
  stop: (id: string) => Promise<void>;
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
    if (event.stage === "done" || event.stage === "error") {
      const kind = event.stage === "done" ? "success" : "error";
      if (progressToastId != null) {
        toast.resolve(progressToastId, kind, event.message);
        progressToastId = null;
      } else {
        toast[kind](event.message);
      }
      if (kind === "error") markEventError(event.message);
      void get().refresh();
    } else if (progressToastId != null) {
      useToast.getState().update(progressToastId, { title: event.message });
    } else {
      progressToastId = toast.progress(event.message);
    }
  },
}));
