import { create } from "zustand";

export type Page = { name: "sites" } | { name: "site"; id: string } | { name: "terminal"; siteId?: string };

interface NavState {
  page: Page;
  navigate: (page: Page) => void;
  /** Settings is a modal, not a page. */
  settingsOpen: boolean;
  setSettingsOpen: (open: boolean) => void;
  /** New-site dialog (opened from the dashboard button or the mod+N command). */
  newSiteOpen: boolean;
  setNewSiteOpen: (open: boolean) => void;
  /** Command palette (mod+K). */
  paletteOpen: boolean;
  setPaletteOpen: (open: boolean) => void;
  /** Keyboard-shortcuts cheat-sheet (?). */
  cheatsheetOpen: boolean;
  setCheatsheetOpen: (open: boolean) => void;
}

export const useNav = create<NavState>((set) => ({
  page: { name: "sites" },
  navigate: (page) => set({ page }),
  settingsOpen: false,
  setSettingsOpen: (open) => set({ settingsOpen: open }),
  newSiteOpen: false,
  setNewSiteOpen: (open) => set({ newSiteOpen: open }),
  paletteOpen: false,
  setPaletteOpen: (open) => set({ paletteOpen: open }),
  cheatsheetOpen: false,
  setCheatsheetOpen: (open) => set({ cheatsheetOpen: open }),
}));
