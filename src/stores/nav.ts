import { create } from "zustand";

/** Sections of the site detail page (plan 28) — deep-linkable from the rail. */
export type SiteTab = "overview" | "tools" | "snapshots" | "logs";

export type Page =
  | { name: "home" }
  | { name: "sites" }
  | { name: "site"; id: string; tab?: SiteTab }
  | { name: "terminal"; siteId?: string };

/** Sections of the settings modal (mirrors `SectionId` in pages/Settings.tsx). */
export type SettingsSection = "general" | "terminal" | "keyboard" | "domains" | "serverkit";

interface NavState {
  page: Page;
  navigate: (page: Page) => void;
  /** Settings is a modal, not a page. */
  settingsOpen: boolean;
  setSettingsOpen: (open: boolean) => void;
  /** Section the modal opens on (deep-link, e.g. the router-conflict banner). */
  settingsSection: SettingsSection;
  openSettings: (section?: SettingsSection) => void;
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
  // Home is the landing page (plan 28) — status first, management second.
  page: { name: "home" },
  navigate: (page) => set({ page }),
  settingsOpen: false,
  setSettingsOpen: (open) => set({ settingsOpen: open }),
  settingsSection: "general",
  openSettings: (section = "general") => set({ settingsOpen: true, settingsSection: section }),
  newSiteOpen: false,
  setNewSiteOpen: (open) => set({ newSiteOpen: open }),
  paletteOpen: false,
  setPaletteOpen: (open) => set({ paletteOpen: open }),
  cheatsheetOpen: false,
  setCheatsheetOpen: (open) => set({ cheatsheetOpen: open }),
}));
