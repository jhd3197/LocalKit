import { useMemo } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ipc } from "./ipc";
import { toastError } from "./errors";
import { toast } from "../stores/toast";
import { useNav } from "../stores/nav";
import { useSettings } from "../stores/settings";
import { useSites } from "../stores/sites";
import type { SiteWithStatus } from "./types";

/**
 * The single command registry (plan 15) — one source of truth feeding the
 * command palette, the global shortcut dispatcher, the cheat-sheet and
 * Settings → Keyboard. Static commands are always present; per-site commands
 * are rebuilt whenever the sites store changes (Faro's per-profile pattern).
 */

export type CommandContext = "global" | "terminal";

export interface Command {
  id: string;
  title: string;
  group: string;
  /** Default key combo (see lib/shortcuts.ts); user overrides live in settings. */
  defaultCombo?: string;
  /** `global` (default) fires anywhere; `terminal` only while a terminal has focus. */
  context?: CommandContext;
  run: () => void;
}

function staticCommands(): Command[] {
  const nav = useNav.getState();
  return [
    {
      id: "go-sites",
      title: "Go to Sites",
      group: "Navigation",
      defaultCombo: "mod+1",
      run: () => nav.navigate({ name: "sites" }),
    },
    {
      id: "go-terminal",
      title: "Go to Terminal",
      group: "Navigation",
      defaultCombo: "mod+2",
      run: () => nav.navigate({ name: "terminal" }),
    },
    {
      id: "new-site",
      title: "New site",
      group: "Sites",
      defaultCombo: "mod+n",
      run: () => {
        nav.navigate({ name: "sites" });
        nav.setNewSiteOpen(true);
      },
    },
    {
      id: "refresh-sites",
      title: "Refresh sites",
      group: "Sites",
      defaultCombo: "mod+r",
      run: () => void useSites.getState().refresh(),
    },
    {
      id: "toggle-view",
      title: "Toggle grid/list view",
      group: "Sites",
      run: () => {
        const s = useSettings.getState();
        s.set("siteView", s.values.siteView === "list" ? "grid" : "list");
      },
    },
    {
      id: "toggle-palette",
      title: "Command palette",
      group: "App",
      defaultCombo: "mod+k",
      run: () => nav.setPaletteOpen(!useNav.getState().paletteOpen),
    },
    {
      id: "open-settings",
      title: "Open settings",
      group: "App",
      defaultCombo: "mod+,",
      run: () => nav.setSettingsOpen(true),
    },
    {
      id: "show-cheatsheet",
      title: "Keyboard shortcuts",
      group: "App",
      defaultCombo: "?",
      run: () => nav.setCheatsheetOpen(true),
    },
  ];
}

function siteCommands(site: SiteWithStatus): Command[] {
  const nav = useNav.getState();
  const running = site.live_status === "running";
  const cmds: Command[] = [
    {
      id: `site.${site.id}.open`,
      title: "Open",
      group: site.name,
      run: () => nav.navigate({ name: "site", id: site.id }),
    },
    running
      ? {
          id: `site.${site.id}.stop`,
          title: "Stop",
          group: site.name,
          run: () => void useSites.getState().stop(site.id),
        }
      : {
          id: `site.${site.id}.start`,
          title: "Start",
          group: site.name,
          run: () => void useSites.getState().start(site.id),
        },
    {
      id: `site.${site.id}.terminal`,
      title: "Terminal",
      group: site.name,
      run: () => nav.navigate({ name: "terminal", siteId: site.id }),
    },
  ];
  cmds.push({
    id: `site.${site.id}.snapshot`,
    title: "Create snapshot",
    group: site.name,
    run: () => {
      void ipc
        .createSnapshot(site.id)
        .then(() => toast.success("Snapshot taken", site.name))
        .catch((e) => toastError(e, "Create snapshot"));
    },
  });
  if (running) {
    cmds.push({
      id: `site.${site.id}.wp-admin`,
      title: "WP Admin",
      group: site.name,
      run: () => {
        void ipc
          .loginSite(site.id)
          .then((url) => openUrl(url))
          .catch((e) => toastError(e, "WP Admin login"));
      },
    });
  }
  return cmds;
}

/** Fresh command list reading current store state (dispatcher uses this). */
export function buildCommands(): Command[] {
  return [...staticCommands(), ...useSites.getState().sites.flatMap(siteCommands)];
}

/** Commands that can appear in Settings → Keyboard (static, one per binding). */
export function bindableCommands(): Command[] {
  return staticCommands();
}

/** Reactive command list for UI (palette, cheat-sheet) — rebuilds on site changes. */
export function useCommands(): Command[] {
  const sites = useSites((s) => s.sites);
  return useMemo(() => buildCommands(), [sites]);
}
