import { create } from "zustand";
import { ipc } from "../lib/ipc";
import { toastError } from "../lib/errors";
import { toast } from "./toast";
import type { Blueprint, Site } from "../lib/types";

/**
 * Reusable site templates (plan 20). Data + actions only; the save and
 * create-from flows stream progress through the shared site-event toast, so
 * this store never plumbs its own progress.
 */
interface BlueprintsState {
  blueprints: Blueprint[];
  loading: boolean;
  refresh: () => Promise<void>;
  save: (siteId: string, name: string, description?: string) => Promise<Blueprint>;
  remove: (id: string) => Promise<void>;
  createSite: (blueprintId: string, name?: string) => Promise<Site>;
}

export const useBlueprints = create<BlueprintsState>((set, get) => ({
  blueprints: [],
  loading: false,

  refresh: async () => {
    set({ loading: true });
    try {
      const blueprints = await ipc.listBlueprints();
      set({ blueprints, loading: false });
    } catch (e) {
      set({ loading: false });
      toastError(e, "List blueprints");
    }
  },

  save: async (siteId, name, description) => {
    try {
      const bp = await ipc.saveBlueprint(siteId, name, description);
      await get().refresh();
      return bp;
    } catch (e) {
      toastError(e, "Save blueprint");
      throw e;
    }
  },

  remove: async (id) => {
    const name = get().blueprints.find((b) => b.id === id)?.name;
    try {
      await ipc.deleteBlueprint(id);
      await get().refresh();
      toast.success("Blueprint deleted", name);
    } catch (e) {
      toastError(e, "Delete blueprint");
    }
  },

  // The new site refreshes into the sites store on the create flow's `done`
  // event; this store only holds the templates, so it needs no refresh here.
  createSite: async (blueprintId, name) => {
    try {
      return await ipc.createSiteFromBlueprint(blueprintId, name);
    } catch (e) {
      toastError(e, "Create from blueprint");
      throw e;
    }
  },
}));
