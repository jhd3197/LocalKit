import { create } from "zustand";

export type Page = { name: "sites" } | { name: "site"; id: string } | { name: "settings" };

interface NavState {
  page: Page;
  navigate: (page: Page) => void;
}

export const useNav = create<NavState>((set) => ({
  page: { name: "sites" },
  navigate: (page) => set({ page }),
}));
