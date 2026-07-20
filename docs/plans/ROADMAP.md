# LocalKit Roadmap

LocalKit is a desktop app that manages local WordPress sites as per-site Docker
Compose projects. v1 = milestones M1–M4: local sites plus ServerKit push/pull
through the `serverkit-localkit` extension (`/api/v1/localkit/...`).

The file numbers ARE the build order — each plan leans on the ones before it.

| # | Plan file | Status | Why here |
|---|-----------|--------|----------|
| 1 | `1_local-site-lifecycle` | ✅ shipped | Foundation: compose projects, ports, start/stop/delete. |
| 2 | `2_wordpress-install-and-detail` | ✅ shipped | Sites aren't useful until WP is installed and credentials are visible. |
| 3 | `3_serverkit-connection` | ✅ shipped | Validates the ServerKit API surface before building sync on it. |
| 4 | `4_serverkit-push-pull` | ✅ shipped | The point of the product: push code/DB, pull DB, sync history. |
| 5 | `5_release-polish` | ⬜ | Installers, updates, keyring, tests — last because it assumes feature-freeze. |
| 6 | `6_local-domains` | ✅ shipped | `http(s)://<slug>.test` via a shared Caddy router, managed hosts block, local CA trust. |
| 7 | `7_cli` | ✅ shipped | Headless CLI companion (`lk`) — same data dir as the GUI, scriptable output. |
| 8 | `8_system-tray` | ✅ shipped | Tray icon + close-to-tray so sites keep running while the window is closed. |
| 9 | `9_windows-console-and-install-hang` | ✅ shipped | Bugfix: hide Windows console windows on subprocess spawns; make first-run install visible (pre-pull + per-attempt progress) so it never looks hung. |
| 10 | `10_one-click-login` | ✅ shipped | One-click WP Admin login via one-time token MU plugin + user picker. |
| 11 | `11_terminal` | ✅ shipped | Embedded per-site terminals: xterm.js + PTY shelling into each site's wordpress container (Faro's PtyManager pattern). |
| 12 | `12_toasts` | ✅ | Global toast store + viewport (from Faro); success/error feedback for every action. |
| 13 | `13_settings-store` | ✅ | Unified settings store on `app_settings` KV + pre-paint injection; substrate for terminal settings and themes. |
| 14 | `14_terminal-quick-wins` | ✅ shipped | Web-links, copy-on-select, ghost-text history, terminal font/scrollback settings (needs 13). |
| 15 | `15_command-palette-shortcuts` | ✅ shipped | Command registry + palette (mod+K), global shortcuts, remappable bindings in Settings (needs 13). |
| 16 | `16_router-coexistence` | ✅ shipped | Port-80/443 conflict pre-flight + configurable router ports so domains survive alongside LocalWP & co. |
| 17 | `17_snapshots` | ⬜ | DB + wp-content snapshots with one-click restore; automatic before push/pull/delete. Safety net for 18–20. |
| 18 | `18_import-remote-site` | ⬜ | Clone a ServerKit site down as a *new* local site (needs the extension's missing pull/code endpoint). |
| 19 | `19_sync-v2-chunked` | ⬜ | Chunked resumable push/pull with byte progress + cancel (breaks the 100 MB / in-memory limits). |
| 20 | `20_clone-and-blueprints` | ⬜ | One-click site clone + save-site-as-blueprint creation flows (needs 17). |
| 21 | `21_cli-serverkit` | ⬜ | `lk connection/push/pull` + remote listing + shell completions (Track D). |
| 22 | `22_multi-stack-core` | ⬜ | Kind/capability site model + bring-your-own-compose Docker apps — before 23–25 so new features are capability-aware from day one. |
| 23 | `23_reconciliation` | ⬜ | Settle DB site status against Docker ground truth; recover half-created sites; Docker-health gating. |
| 24 | `24_site-tools` | ⬜ | Tools tab: Adminer sidecar, serialization-safe search-replace, WP_DEBUG + log viewer, config editor. |
| 25 | `25_release-polish-completion` | ⬜ | M5 remainder: update checker, OS keyring for API keys, OS notifications, real test suite. |
| 26 | `26_php-laravel-stack` | ⬜ | Generated PHP/Laravel stack + per-kind ServerKit sync parity (needs 22, 17–19). |

Status glyphs: ✅ shipped · 🔄 partial · ⬜ not started · 🅿️ deferred

## Track A — Local sites (M1–M2)

- ✅ Compose project generation (`docker-compose.yml` + `.env` per site)
- ✅ Port allocation (site from 8081, DB = site + 10000)
- ✅ wp-cli install via profile-gated `wpcli` service
- ✅ Credentials, logs, container status in the UI
- ✅ Embedded per-site terminals (plan 11): Terminal page with one tab per
  site, xterm.js + PTY running `docker compose exec wordpress bash`
- ✅ One-click WP Admin login via one-time-token MU plugin + user picker
  (plan 10)
- ✅ Windows polish: hide subprocess console windows, visible first-run
  install progress (plan 9)
- ⬜ Site duplication / clone (plan 20, with blueprints)

## Track B — ServerKit (M3–M4)

- ✅ Connection model + `X-API-Key` client (migration 2)
- ✅ Health check + key validation + `/api/v1/localkit/pair` extension probe
- ✅ Remote site listing + provisioning via the `serverkit-localkit` extension
- ✅ Push code (in-memory tar.gz of `wp-content/`), push DB (`wp db export`),
  pull DB (download → `wp db import` → `wp search-replace`)
- ✅ Sync history per site (migration 3)
- ⬜ Pull a remote site down as a *new* local site (plan 18; today pull
  targets an existing local site)

## Track C — Product (M5–M6)

- ⬜ `npm run tauri build` installers per platform
- ⬜ Update awareness (plan 25 — checker first, Tauri updater if releases get signed)
- ⬜ OS keyring for ServerKit API keys (plan 25; plaintext SQLite accepted for v1)
- ⬜ Real test suite (plan 25; today: `cargo check` + router hosts-block unit tests +
  the `smoke` / `m4_smoke` / `m6_smoke` examples)
- ✅ Local domains: `http(s)://<slug>.test` via a shared Caddy router +
  managed hosts block + local CA trust (plan 6), layered on top of the
  always-working `localhost:<port>` URLs
- ✅ Router coexistence (plan 16): port pre-flight that names the process
  holding 80/443, configurable router ports with one-click fallback to
  8080/8443, port-aware `site_public_url`, conflict UX in Settings +
  SiteDetail + `lk doctor`
- ✅ System tray + background mode: close-to-tray, tray menu with quick site
  actions, single-instance focus (plan 8)

## Track D — CLI (M7)

- ✅ `lk` binary (`src-tauri/src/bin/lk.rs`, clap): thin wrapper over
  `localkit_lib`, shares the GUI's data dir + SQLite DB
- ✅ Lifecycle: `list` / `create` / `start` / `stop` / `restart` / `delete`
  / `info` / `logs`
- ✅ `lk wp <site> <args...>` wp-cli passthrough, `lk env` (eval-able
  exports), `lk doctor`, `-o json` / `--quiet` / `--data-dir` global flags
- ⬜ ServerKit from the CLI: `lk connection add/list`, `lk push`, `lk pull`
  (plan 21; library calls already exist)
- ⬜ Shell completions (plan 21), self-update (future)

## Track F — Multi-stack (M9)

- ⬜ Kind/capability site model (`wordpress` | `docker`, `config_json`,
  capability-gated features in both frontends) — plan 22, placed before the
  remaining feature plans so they're capability-aware from day one
- ⬜ Generic Docker apps: import an existing compose project → lifecycle,
  logs, terminal, local domain, snapshots (plan 22)
- ⬜ PHP/Laravel generated stack + engine-native DB sync + per-kind
  ServerKit push/pull/import parity (plan 26)
- 🅿️ Node/Python kinds (unplanned; same capability shape when there's demand)

## Track E — UX ports from Faro (M12–M14)

Features ported from Faro's proven implementations (see the port survey;
Faro paths referenced in each plan):

- ✅ Toast notifications (plan 12): global toast store + viewport,
  `toast.success/error` callable from stores — replaces the ad-hoc
  progress/error toasts in `App.tsx`
- ✅ Settings store (plan 13): unified frontend store over `app_settings`
  KV, pre-paint injection via `initialization_script` — substrate for
  terminal settings, themes, notification prefs
- ✅ Terminal quick wins (plan 14): web-links addon, copy-on-select,
  ghost-text per-site command history, font-size/scrollback settings
- ✅ Command palette + shortcuts (plan 15): one command registry feeding a
  fuzzy palette (mod+K), global shortcuts with editable-target guards,
  remappable bindings in Settings → Keyboard, cheat-sheet, shared
  `useDialog` for modals
- ⬜ Later candidates from the survey (unplanned): OS desktop
  notifications, auto-updater (Track C), context menus, structured
  `{kind, message}` IPC errors, snippets, light theme (needs a CSS-var
  token layer first)
