import { create } from "zustand";
import { ipc } from "../lib/ipc";
import type { RemoteWpSite, ServerKitConnection, ServerKitInfo } from "../lib/types";

function errMsg(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : String(e);
}

export interface RemoteSitesState {
  loading: boolean;
  sites: RemoteWpSite[] | null;
  error: string | null;
}

interface ServerKitState {
  connections: ServerKitConnection[];
  testing: boolean;
  testResult: { ok: boolean; message: string } | null;
  saving: boolean;
  busyId: string | null;
  error: string | null;
  /** Per-connection remote site lists, keyed by connection id. */
  remote: Record<string, RemoteSitesState>;
  refresh: () => Promise<void>;
  test: (url: string, apiKey: string) => Promise<ServerKitInfo | null>;
  save: (label: string, url: string, apiKey: string) => Promise<boolean>;
  remove: (id: string) => Promise<void>;
  fetchRemoteSites: (id: string) => Promise<void>;
  clearTestResult: () => void;
}

export const useServerKit = create<ServerKitState>((set, get) => ({
  connections: [],
  testing: false,
  testResult: null,
  saving: false,
  busyId: null,
  error: null,
  remote: {},

  refresh: async () => {
    try {
      const connections = await ipc.listServerkitConnections();
      set({ connections, error: null });
    } catch (e) {
      set({ error: errMsg(e) });
    }
  },

  test: async (url, apiKey) => {
    set({ testing: true, testResult: null });
    try {
      const info = await ipc.testServerkitConnection(url, apiKey);
      const where = info.canonical_origin ?? info.canonical_domain ?? url;
      const ext = info.localkit_extension
        ? "push/pull ready"
        : "serverkit-localkit extension not detected (push/pull unavailable)";
      set({
        testing: false,
        testResult: { ok: true, message: `Connected to ServerKit at ${where} (API key valid, ${ext})` },
      });
      return info;
    } catch (e) {
      set({ testing: false, testResult: { ok: false, message: errMsg(e) } });
      return null;
    }
  },

  save: async (label, url, apiKey) => {
    set({ saving: true, error: null });
    try {
      await ipc.saveServerkitConnection(label, url, apiKey);
      await get().refresh();
      set({ saving: false });
      return true;
    } catch (e) {
      set({ saving: false, error: errMsg(e) });
      return false;
    }
  },

  remove: async (id) => {
    set({ busyId: id, error: null });
    try {
      await ipc.deleteServerkitConnection(id);
      set((s) => {
        const remote = { ...s.remote };
        delete remote[id];
        return { remote };
      });
      await get().refresh();
    } catch (e) {
      set({ error: errMsg(e) });
    } finally {
      set({ busyId: null });
    }
  },

  fetchRemoteSites: async (id) => {
    set((s) => ({ remote: { ...s.remote, [id]: { loading: true, sites: null, error: null } } }));
    try {
      const sites = await ipc.listRemoteWpSites(id);
      set((s) => ({ remote: { ...s.remote, [id]: { loading: false, sites, error: null } } }));
    } catch (e) {
      set((s) => ({
        remote: { ...s.remote, [id]: { loading: false, sites: null, error: errMsg(e) } },
      }));
    }
  },

  clearTestResult: () => set({ testResult: null }),
}));
