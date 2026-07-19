import { create } from "zustand";

export type Page = { name: "sites" } | { name: "site"; id: string } | { name: "terminal"; siteId?: string };
export type SiteView = "grid" | "list";

interface NavState {
  page: Page;
  navigate: (page: Page) => void;
  /** Settings is a modal, not a page. */
  settingsOpen: boolean;
  setSettingsOpen: (open: boolean) => void;
  /** Dashboard layout; persisted in localStorage. */
  siteView: SiteView;
  setSiteView: (view: SiteView) => void;
}

const storedView = localStorage.getItem("localkit.siteView");

export const useNav = create<NavState>((set) => ({
  page: { name: "sites" },
  navigate: (page) => set({ page }),
  settingsOpen: false,
  setSettingsOpen: (open) => set({ settingsOpen: open }),
  siteView: storedView === "list" ? "list" : "grid",
  setSiteView: (view) => {
    localStorage.setItem("localkit.siteView", view);
    set({ siteView: view });
  },
}));
