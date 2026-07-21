import { create } from "zustand";
import { ipc } from "../lib/ipc";
import type { RouterStatus } from "../lib/types";

interface RouterState {
  status: RouterStatus | null;
  busy: boolean;
  refresh: () => Promise<void>;
  setEnabled: (enabled: boolean) => Promise<void>;
  /** Change the router's host ports (fallback mode); returns an error string. */
  setPorts: (http: number, https: number) => Promise<string | null>;
  trustCa: () => Promise<string | null>;
}

export const useRouter = create<RouterState>((set) => ({
  status: null,
  busy: false,

  refresh: async () => {
    try {
      const status = await ipc.routerStatus();
      set({ status });
    } catch {
      // Docker/backend unavailable — leave status as-is; UI falls back to
      // localhost:<port> URLs when status is null.
    }
  },

  setEnabled: async (enabled) => {
    set({ busy: true });
    try {
      const status = await ipc.setDomainsEnabled(enabled);
      set({ status });
    } catch {
      await useRouter.getState().refresh();
    } finally {
      set({ busy: false });
    }
  },

  setPorts: async (http, https) => {
    set({ busy: true });
    try {
      const status = await ipc.setRouterPorts(http, https);
      set({ status });
      return null;
    } catch (e) {
      await useRouter.getState().refresh();
      return typeof e === "string" ? e : e instanceof Error ? e.message : String(e);
    } finally {
      set({ busy: false });
    }
  },

  trustCa: async () => {
    set({ busy: true });
    try {
      const status = await ipc.trustRouterCa();
      set({ status });
      return null;
    } catch (e) {
      await useRouter.getState().refresh();
      return typeof e === "string" ? e : e instanceof Error ? e.message : String(e);
    } finally {
      set({ busy: false });
    }
  },
}));
