# 8 — System tray & background mode

Status: ✅ shipped (v1 core — autostart stretch deferred)

Keep LocalKit alive in the OS system tray so local sites (and the M6 Caddy
router) keep running while the window is closed. Closing the window hides it
to the tray instead of quitting; a real quit only happens from the tray menu.

## Motivation

Local sites are only useful while Docker containers are up, and the containers
are independent of the GUI — but today closing the LocalKit window kills the
app, so there's no lightweight "running in the background" presence. A tray
icon makes the app feel like other dev tools (Docker Desktop, LocalWP): out of
the way, one click to reopen, quick site actions without opening the window.

## Design

- **Tauri 2 built-in tray.** Use `tauri::tray::TrayIconBuilder` +
  `tauri::menu::{MenuBuilder, MenuItemBuilder, SubmenuBuilder}` — no extra
  crate needed (tray support ships in the `tauri` crate with default
  features). All tray code lives in a new `src-tauri/src/tray.rs`; `lib.rs`
  `run()` just calls `tray::setup(&app)` inside `.setup()`.
- **Icon.** Start with the existing brand logo (`src-tauri/icons/`), resized
  to a small PNG embedded via `include_bytes!`. On Windows/macOS a
  template/monochrome variant renders better in dark+light taskbars — ship the
  colored logo first, add a template variant as polish.
- **Close-to-tray.** In `on_window_event`, intercept
  `WindowEvent::CloseRequested` on the `main` window: `api.prevent_close()` +
  `window.hide()`. The `run_in_background` flag in `app_settings` (KV table —
  no migration needed) controls this; default **on**. When off, close quits
  as today. Toggle lives in the existing Settings modal, read/written via a
  tiny `get_setting`/`set_setting` command pair (generic, like the router
  settings).
- **Tray menu.**
  - `Show LocalKit` — unhide + focus the main window
    (`show()` + `set_focus()` on the main thread).
  - `Sites ▸` submenu — one item per site, rebuilt on open
    (`MenuEvent`/`about_to_show`) or on `site-event`: running sites get
    `Open in browser` (opener plugin, already a dependency) + `Stop`; stopped
    sites get `Start`. Labels show `● slug` / `○ slug`.
  - `Quit LocalKit` — real quit (`app.exit(0)`). Containers are **left
    running** (same as LocalWP); document this. A "stop all sites on quit"
    option is future work.
- **Left-click** on the tray icon = same as `Show LocalKit` (Windows/Linux;
  macOS convention is menu-only — accept the platform default there).
- **Tooltip.** `LocalKit — N sites running`, refreshed whenever site status
  changes (hook into the same place `site-event` is emitted, or just rebuild
  cheaply on menu open + after each start/stop command).
- **Single instance.** Add `tauri-plugin-single-instance` so relaunching the
  app while it's in the tray focuses the existing window instead of spawning a
  second process that fights over the same SQLite DB.
- **Start at login (stretch, same plan).** `tauri-plugin-autostart` with a
  Settings toggle "Launch LocalKit at login (minimized to tray)". Only do it
  if it stays a few lines; otherwise defer.

## Frontend changes

- Settings modal: new "Background" section with the `run_in_background`
  toggle (and the autostart toggle if the stretch lands). Follows the
  existing zinc/violet styling; copy explains that closing the window keeps
  sites running and quit happens from the tray.
- `src/lib/ipc.ts`: typed wrappers for the new get/set setting command.

## Known trade-offs (accepted)

- Quitting from the tray does not stop Docker containers — the app is a
  manager, not a supervisor. Stopping everything on quit would surprise users
  who expect `slug.test` to keep working.
- Tray menu is not a full site manager (no create/delete, no sync actions) —
  deliberately minimal; the window is one click away.
- Menu rebuilds are naive (query sites on open). Site counts are tiny; no
  caching needed.

## Future work (not planned)

- Native notifications when long ops finish while hidden (site created, push/
  pull done) via `tauri-plugin-notification`.
- "Stop all sites & quit" menu item.
- Status dot / badge baked into the tray icon when sites are running.
- Per-site error surfacing in the tray menu.

## Verification

- `cargo check --all-targets` clean; `npm run build` clean.
- Manual on Windows (primary dev platform):
  - App starts → tray icon appears with tooltip.
  - Close window → process stays alive, icon remains; `slug.test` still
    resolves for running sites.
  - Tray `Show LocalKit` and left-click restore + focus the window.
  - Sites submenu reflects live status; `Stop`/`Start` from the tray work and
    the Dashboard (when reopened) agrees.
  - `Quit LocalKit` exits the process; containers keep running
    (`docker ps` still shows site stacks).
  - Settings toggle off → close quits the app (old behavior).
  - Second launch while in tray focuses the existing window.
- Manual on macOS if available: icon renders in menu bar, menu-only behavior
  acceptable.
