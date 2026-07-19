import { create } from "zustand";
import { ipc } from "../lib/ipc";

/**
 * Unified settings store backed by the Rust-side `app_settings` KV table.
 * Seeded synchronously from `window.__LOCALKIT_SETTINGS__` (injected by the
 * backend before first paint) so preferences apply without a flash; in
 * mock/pure-web mode the injection is absent and the store hydrates async
 * from `settings_get_all`. Writes are optimistic + fire-and-forget, and
 * mirrored to localStorage so pure-web mock/dev keeps prefs across reloads.
 *
 * Adding a new preference = read/write a key here — no new Tauri commands.
 */

declare global {
  interface Window {
    __LOCALKIT_SETTINGS__?: Record<string, string>;
  }
}

const LS_PREFIX = "localkit.settings.";
/** Pre-settings-store home of the grid/list pref; migrated once, then deleted. */
const LEGACY_SITEVIEW_KEY = "localkit.siteView";

interface SettingsState {
  values: Record<string, string>;
  set: (key: string, value: string) => void;
}

function lsMirror(): Record<string, string> {
  const out: Record<string, string> = {};
  try {
    for (let i = 0; i < localStorage.length; i++) {
      const k = localStorage.key(i);
      if (k?.startsWith(LS_PREFIX)) out[k.slice(LS_PREFIX.length)] = localStorage.getItem(k) ?? "";
    }
  } catch {
    /* localStorage unavailable */
  }
  return out;
}

function seed(): { values: Record<string, string>; injected: boolean } {
  const injected = typeof window !== "undefined" && !!window.__LOCALKIT_SETTINGS__;
  // localStorage mirrors first so mock/dev renders correctly before the
  // async hydration lands; the injection (real app) always wins.
  const values = { ...lsMirror(), ...(injected ? window.__LOCALKIT_SETTINGS__ : {}) };

  // One-time migration: localkit.siteView (pre-plan-13 localStorage pref).
  try {
    const legacy = localStorage.getItem(LEGACY_SITEVIEW_KEY);
    if (legacy) {
      values.siteView = legacy;
      localStorage.removeItem(LEGACY_SITEVIEW_KEY);
      localStorage.setItem(LS_PREFIX + "siteView", legacy);
      void ipc.setAppSetting("siteView", legacy).catch(() => {});
    }
  } catch {
    /* ignore */
  }
  return { values, injected };
}

export const useSettings = create<SettingsState>((set) => {
  const { values, injected } = seed();
  if (!injected) {
    // Mock / pure-web mode: hydrate from the (mock) backend.
    ipc
      .settingsGetAll()
      .then((all) => set((s) => ({ values: { ...s.values, ...all } })))
      .catch(() => {});
  }
  return {
    values,
    set: (key, value) => {
      set((s) => ({ values: { ...s.values, [key]: value } }));
      try {
        localStorage.setItem(LS_PREFIX + key, value);
      } catch {
        /* ignore */
      }
      void ipc.setAppSetting(key, value).catch(() => {});
    },
  };
});

// ---------------------------------------------------------------------------
// Typed accessors (parsing lives here; unknown keys pass through as strings)
// ---------------------------------------------------------------------------

export type SiteView = "grid" | "list";

/** Dashboard layout pref (moved out of nav.ts localStorage in plan 13). */
export function useSiteView(): [SiteView, (view: SiteView) => void] {
  const view = useSettings((s) => (s.values.siteView === "list" ? "list" : "grid"));
  const set = useSettings((s) => s.set);
  return [view, (v) => set("siteView", v)];
}

/** Terminal font size in px (plan 14 consumes; matches the current hardcode). */
export function useTerminalFontSize(): number {
  return useSettings((s) => {
    const n = Number(s.values.terminalFontSize);
    return Number.isFinite(n) && n > 0 ? n : 13;
  });
}

/** Terminal scrollback lines (plan 14 consumes). */
export function useTerminalScrollback(): number {
  return useSettings((s) => {
    const n = Number(s.values.terminalScrollback);
    return Number.isFinite(n) && n > 0 ? n : 5000;
  });
}
