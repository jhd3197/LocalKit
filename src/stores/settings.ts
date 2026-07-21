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
  /** Delete a key entirely (back to the accessor's default — plan 15 resets). */
  remove: (key: string) => void;
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
    remove: (key) => {
      set((s) => {
        const values = { ...s.values };
        delete values[key];
        return { values };
      });
      try {
        localStorage.removeItem(LS_PREFIX + key);
      } catch {
        /* ignore */
      }
      void ipc.deleteAppSetting(key).catch(() => {});
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

/** Terminal font size in px (11–16, default 13). */
export const TERMINAL_FONT_SIZE_DEFAULT = 13;
/** Terminal scrollback lines (default 5k). */
export const TERMINAL_SCROLLBACK_DEFAULT = 5000;

function parsePositiveInt(raw: string | undefined, fallback: number): number {
  const n = Number(raw);
  return Number.isFinite(n) && n > 0 ? Math.round(n) : fallback;
}

/** Non-hook read (terminal registry creates terminals outside React). */
export function getTerminalFontSize(): number {
  return parsePositiveInt(
    useSettings.getState().values.terminalFontSize,
    TERMINAL_FONT_SIZE_DEFAULT
  );
}

/** Non-hook read (terminal registry creates terminals outside React). */
export function getTerminalScrollback(): number {
  return parsePositiveInt(
    useSettings.getState().values.terminalScrollback,
    TERMINAL_SCROLLBACK_DEFAULT
  );
}

/** Terminal font size in px (plan 14; matches the historical hardcode). */
export function useTerminalFontSize(): [number, (px: number) => void] {
  const size = useSettings((s) =>
    parsePositiveInt(s.values.terminalFontSize, TERMINAL_FONT_SIZE_DEFAULT)
  );
  const set = useSettings((s) => s.set);
  return [size, (px) => set("terminalFontSize", String(px))];
}

/** Terminal scrollback lines (plan 14; applies to newly opened terminals). */
export function useTerminalScrollback(): [number, (lines: number) => void] {
  const lines = useSettings((s) =>
    parsePositiveInt(s.values.terminalScrollback, TERMINAL_SCROLLBACK_DEFAULT)
  );
  const set = useSettings((s) => s.set);
  return [lines, (n) => set("terminalScrollback", String(n))];
}

// ---------------------------------------------------------------------------
// Update awareness (plan 25) — throttle + per-version snooze in the KV
// ---------------------------------------------------------------------------

const UPDATE_LAST_CHECKED = "update.lastChecked";
const UPDATE_SNOOZED = "update.snoozed";
/** Check GitHub for a new release at most once a day on launch. */
export const UPDATE_CHECK_INTERVAL_MS = 24 * 60 * 60 * 1000;

/** Epoch ms of the last launch update check (0 = never). */
export function getUpdateLastChecked(): number {
  const raw = useSettings.getState().values[UPDATE_LAST_CHECKED];
  const n = Number(raw);
  return Number.isFinite(n) && n > 0 ? n : 0;
}
export function markUpdateChecked(nowMs: number): void {
  useSettings.getState().set(UPDATE_LAST_CHECKED, String(nowMs));
}

/** The version whose launch toast was already shown (nudge once per version). */
export function getSnoozedUpdate(): string | undefined {
  return useSettings.getState().values[UPDATE_SNOOZED] || undefined;
}
export function snoozeUpdate(version: string): void {
  useSettings.getState().set(UPDATE_SNOOZED, version);
}
