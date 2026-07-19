import { create } from "zustand";

export type Page = { name: "sites" } | { name: "site"; id: string } | { name: "terminal"; siteId?: string };

interface NavState {
  page: Page;
  navigate: (page: Page) => void;
  /** Settings is a modal, not a page. */
  settingsOpen: boolean;
  setSettingsOpen: (open: boolean) => void;
}

export const useNav = create<NavState>((set) => ({
  page: { name: "sites" },
  navigate: (page) => set({ page }),
  settingsOpen: false,
  setSettingsOpen: (open) => set({ settingsOpen: open }),
}));
