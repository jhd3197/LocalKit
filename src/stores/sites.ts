import { create } from "zustand";
import { ipc } from "../lib/ipc";
import type { Site, SiteEvent, SiteWithStatus, WpInfo } from "../lib/types";

interface SitesState {
  sites: SiteWithStatus[];
  loading: boolean;
  error: string | null;
  creating: boolean;
  busyId: string | null;
  progress: SiteEvent | null;
  logs: Record<string, string>;
  wpInfo: Record<string, WpInfo | null>;
  refresh: () => Promise<void>;
  createSite: (name: string, wpVersion: string, phpVersion: string) => Promise<Site>;
  start: (id: string) => Promise<void>;
  stop: (id: string) => Promise<void>;
  remove: (id: string) => Promise<void>;
  fetchLogs: (id: string) => Promise<void>;
  fetchWpInfo: (id: string) => Promise<void>;
  handleEvent: (event: SiteEvent) => void;
  dismissProgress: () => void;
  clearError: () => void;
}

function errMsg(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : String(e);
}

export const useSites = create<SitesState>((set, get) => ({
  sites: [],
  loading: false,
  error: null,
  creating: false,
  busyId: null,
  progress: null,
  logs: {},
  wpInfo: {},

  refresh: async () => {
    set({ loading: true });
    try {
      const sites = await ipc.listSites();
      set({ sites, error: null, loading: false });
    } catch (e) {
      set({ error: errMsg(e), loading: false });
    }
  },

  createSite: async (name, wpVersion, phpVersion) => {
    set({ creating: true, progress: null, error: null });
    try {
      const site = await ipc.createSite(name, wpVersion, phpVersion);
      await get().refresh();
      return site;
    } catch (e) {
      set({ error: errMsg(e) });
      throw e;
    } finally {
      set({ creating: false });
    }
  },

  start: async (id) => {
    set({ busyId: id, error: null });
    try {
      await ipc.startSite(id);
      await get().refresh();
    } catch (e) {
      set({ error: errMsg(e) });
    } finally {
      set({ busyId: null });
    }
  },

  stop: async (id) => {
    set({ busyId: id, error: null });
    try {
      await ipc.stopSite(id);
      await get().refresh();
    } catch (e) {
      set({ error: errMsg(e) });
    } finally {
      set({ busyId: null });
    }
  },

  remove: async (id) => {
    set({ busyId: id, error: null });
    try {
      await ipc.deleteSite(id);
      await get().refresh();
    } catch (e) {
      set({ error: errMsg(e) });
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
      void get().refresh();
    }
  },

  dismissProgress: () => set({ progress: null }),
  clearError: () => set({ error: null }),
}));
