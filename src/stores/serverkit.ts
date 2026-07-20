import { create } from "zustand";
import { ipc } from "../lib/ipc";
import { toastError } from "../lib/errors";
import { useSites } from "./sites";
import { FEATURE_PULL_CODE } from "../lib/types";
import type { RemoteWpSite, ServerKitConnection, ServerKitInfo, Site } from "../lib/types";

function errMsg(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : String(e);
}

export interface RemoteSitesState {
  loading: boolean;
  sites: RemoteWpSite[] | null;
  error: string | null;
  /**
   * Whether this server's extension can serve `pull/code`. `null` while
   * unknown (the probe rides along with the site listing) — the Import button
   * stays disabled until it is explicitly `true`, because finding out
   * mid-import means a half-built local site.
   */
  canImport: boolean | null;
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
  /** Connection id + remote site currently open in the Import dialog. */
  importing: { connectionId: string; site: RemoteWpSite } | null;
  importBusy: boolean;
  refresh: () => Promise<void>;
  test: (url: string, apiKey: string) => Promise<ServerKitInfo | null>;
  save: (label: string, url: string, apiKey: string) => Promise<boolean>;
  remove: (id: string) => Promise<void>;
  fetchRemoteSites: (id: string) => Promise<void>;
  openImport: (connectionId: string, site: RemoteWpSite) => void;
  closeImport: () => void;
  importSite: (name?: string) => Promise<Site | null>;
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
  importing: null,
  importBusy: false,

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
    set((s) => ({
      remote: {
        ...s.remote,
        [id]: { loading: true, sites: null, error: null, canImport: null },
      },
    }));
    try {
      const sites = await ipc.listRemoteWpSites(id);
      // Capability probe rides along with the listing: the Import buttons
      // render in the same pass, so they must already know whether this
      // server's extension is new enough to serve wp-content.
      const conn = get().connections.find((c) => c.id === id);
      let canImport = false;
      if (conn) {
        try {
          const info = await ipc.testServerkitConnection(conn.url, conn.api_key);
          canImport = info.features.includes(FEATURE_PULL_CODE);
        } catch {
          // A failed probe is not a failed listing — just no Import.
          canImport = false;
        }
      }
      set((s) => ({
        remote: { ...s.remote, [id]: { loading: false, sites, error: null, canImport } },
      }));
    } catch (e) {
      set((s) => ({
        remote: {
          ...s.remote,
          [id]: { loading: false, sites: null, error: errMsg(e), canImport: null },
        },
      }));
    }
  },

  openImport: (connectionId, site) => set({ importing: { connectionId, site } }),
  closeImport: () => set({ importing: null }),

  importSite: async (name) => {
    const target = get().importing;
    if (!target) return null;
    set({ importBusy: true });
    try {
      const site = await ipc.importRemoteSite(target.connectionId, target.site.id, name);
      // The dashboard is where the new site lives; the progress toast is
      // already telling the story, so no extra success toast here.
      await useSites.getState().refresh();
      set({ importing: null });
      return site;
    } catch (e) {
      toastError(e, "Import site");
      return null;
    } finally {
      set({ importBusy: false });
    }
  },

  clearTestResult: () => set({ testResult: null }),
}));
