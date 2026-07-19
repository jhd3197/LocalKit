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
| 10 | `10_one-click-login` | ⬜ | One-click WP Admin login via one-time token MU plugin; multi-user picker later. |
| 11 | `11_terminal` | ✅ shipped | Embedded per-site terminals: xterm.js + PTY shelling into each site's wordpress container (Faro's PtyManager pattern). |

Status glyphs: ✅ shipped · 🔄 partial · ⬜ not started · 🅿️ deferred

## Track A — Local sites (M1–M2)

- ✅ Compose project generation (`docker-compose.yml` + `.env` per site)
- ✅ Port allocation (site from 8081, DB = site + 10000)
- ✅ wp-cli install via profile-gated `wpcli` service
- ✅ Credentials, logs, container status in the UI
- ✅ Embedded per-site terminals (plan 11): Terminal page with one tab per
  site, xterm.js + PTY running `docker compose exec wordpress bash`
- ⬜ One-click WP Admin login via one-time-token MU plugin (plan 10;
  multi-user picker as phase 2)
- ✅ Windows polish: hide subprocess console windows, visible first-run
  install progress (plan 9)
- ⬜ Site duplication / clone (nice-to-have, unplanned)

## Track B — ServerKit (M3–M4)

- ✅ Connection model + `X-API-Key` client (migration 2)
- ✅ Health check + key validation + `/api/v1/localkit/pair` extension probe
- ✅ Remote site listing + provisioning via the `serverkit-localkit` extension
- ✅ Push code (in-memory tar.gz of `wp-content/`), push DB (`wp db export`),
  pull DB (download → `wp db import` → `wp search-replace`)
- ✅ Sync history per site (migration 3)
- ⬜ Pull a remote site down as a *new* local site (today pull targets an
  existing local site)

## Track C — Product (M5–M6)

- ⬜ `npm run tauri build` installers per platform
- ⬜ Auto-update (Tauri updater)
- ⬜ OS keyring for ServerKit API keys (plaintext SQLite accepted for v1)
- ⬜ Real test suite (today: `cargo check` + router hosts-block unit tests +
  the `smoke` / `m4_smoke` / `m6_smoke` examples)
- ✅ Local domains: `http(s)://<slug>.test` via a shared Caddy router +
  managed hosts block + local CA trust (plan 6), layered on top of the
  always-working `localhost:<port>` URLs
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
  (library calls already exist; future)
- ⬜ Shell completions, self-update (future)
