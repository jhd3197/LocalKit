# 13 — Settings store + pre-paint injection

Status: ⬜ not started

One unified frontend settings store backed by the existing `app_settings` KV
table, injected into the window before first paint. This is the substrate
for terminal settings (plan 14), themes later, and notification prefs — not
a user-facing feature on its own.

## Motivation

Settings today are scattered: `localkit.siteView` lives in localStorage
(`stores/nav.ts`), `run_in_background` / domains flags live in Rust-side
`app_settings` with one command pair per key (`get_app_setting` /
`set_app_setting`), and plan 11 hardcodes terminal font/theme. Every new
preference currently re-invents persistence. Faro's answer (its plan 12
Phase 2): a KV table + generic commands + a Zustand store seeded by an
`initialization_script` so settings apply **before first paint** (no flash)
— LocalKit's `app_settings` KV already exists, so this is mostly frontend
work.

## Design

**Backend (small — mostly generalizing what exists).**

- Keep the `app_settings` KV table (no migration needed).
- Add generic bulk commands in `lib.rs` next to the existing per-key ones:
  `settings_get_all() -> Map<String, String>` and keep
  `set_app_setting(key, value)` for writes (fire-and-forget from the
  frontend). The per-key getter can stay for Rust-internal callers
  (`run_in_background` etc.).
- `tauri::Builder` `.initialization_script()` on the main window (Faro:
  `src-tauri/src/lib.rs` `build_settings_init_script`): reads the KV pairs
  synchronously at window build and publishes
  `window.__LOCALKIT_SETTINGS__ = {...}` before any frontend JS runs.

**Frontend — `src/stores/settings.ts` (Zustand).**

- Seed synchronously from `window.__LOCALKIT_SETTINGS__` at store creation;
  fall back to async `settings_get_all()` when the injection is absent
  (mock mode — where the mock just answers from its in-memory
  `data.appSettings`, which already exists).
- `set(key, value)`: optimistic local update + fire-and-forget invoke;
  also mirrors to localStorage so pure-web mock/dev keeps working.
- Typed accessors for known keys (Faro keeps an allow-list `SETTINGS_KEYS`
  for its migration; we just export typed getters):
  `siteView` (migrate `localkit.siteView` out of `stores/nav.ts` into here),
  `terminalFontSize`, `terminalScrollback` (plan 14 consumes these).
- Unknown keys pass through as strings; parsing (`"true"` → boolean) lives
  in the typed getters, same as Rust's `app_settings` conventions today.

**First consumer.** Move `siteView` from `nav.ts` localStorage to the new
store (one-time migrate: read old localStorage key, write to settings,
delete). Settings modal sections keep working unchanged — they already go
through `get/set_app_setting`.

## Implementation notes

- Faro references: `Faro/src/stores/settingsStore.ts` (store shape,
  injection seeding, `persistKey`), `Faro/src-tauri/src/lib.rs`
  (`build_settings_init_script`, `.initialization_script(...)`).
- Do NOT port Faro's localStorage→DB migration machinery — LocalKit has
  exactly one localStorage pref (`siteView`) and it's handled above.
- The injection must be mock-mode-safe: store creation checks
  `typeof window.__LOCALKIT_SETTINGS__` and simply hydrates async when
  missing (vite dev / mock build never runs the Rust init script).
- No UI changes in this plan beyond the `siteView` move; terminal settings
  UI lands with plan 14, theme UI whenever themes happen.

## Definition of done

- All preferences flow through `stores/settings.ts`; `nav.ts` no longer
  touches localStorage.
- Grid/list view pref survives reload in both real and mock mode, and
  applies without a flash on cold start (injection path verified via
  `npm run tauri dev`).
- New settings keys require zero new Tauri commands.
- `cargo check` + `npm run build` clean.
