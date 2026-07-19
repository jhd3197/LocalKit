# AGENTS.md — LocalKit

## What this is

Desktop app (Tauri 2) that manages local WordPress sites via per-site Docker
Compose projects. v1 = milestones M1–M4 (local sites + ServerKit push/pull).
Push/pull talks to the `serverkit-localkit` extension on the server
(`/api/v1/localkit`, in the ServerKit repo).

## Project structure

```
src/                     React 18 + TS + Vite frontend
  lib/ipc.ts             typed wrappers for all Tauri commands (invoke + events)
  lib/types.ts           shared TS types mirroring Rust payloads
  stores/                Zustand stores (nav.ts = page state + settings modal +
                         grid/list view pref, sites.ts = data/actions)
  pages/                 Dashboard (grid/list site views), SiteDetail,
                         Settings (modal, opened via sidebar gear — not a page)
  components/            Sidebar, StatusBadge, CopyButton, NewSiteDialog,
                         icons.tsx (inline SVGs, 1.75px rounded strokes)
  assets/logo.png        Vite-bundled brand logo (sidebar); master at assets/logo.png
  mock/                  in-browser mocks of @tauri-apps/* for `vite --mode mock`
                         (data.ts = fictional sites/connections; core.ts mirrors
                         the command names/payloads in lib/ipc.ts)
src-tauri/               Rust backend (also a cargo workspace root)
  src/lib.rs             AppState, Tauri command registration, app entry (run())
  lk/                    `lk` CLI — separate workspace crate (a [[bin]] in the
                         GUI package would break the macOS universal bundler);
                         thin clap wrapper over localkit_lib, shares the GUI's
                         data dir + SQLite DB. Run with `cargo run -p lk -- <cmd>`
  src/db.rs              rusqlite, forward-only migrations via PRAGMA user_version
  src/docker.rs          `docker compose` CLI wrapper (check/up/down/run/ps/logs)
  src/site.rs            Site model + lifecycle + compose/env templates
  src/wordpress.rs       wp-cli via `docker compose run --rm wpcli wp ...`
  src/router.rs          M6 local domains: shared Caddy router (`*.test`),
                         hosts-file block + elevated writer, CA trust, status
  src/tray.rs            M8 system tray: close-to-tray, tray menu with quick
                         site Open/Start/Stop, single-instance focus
  src/serverkit.rs       ServerKit API client (X-API-Key) + connection model
  src/sync.rs            push/pull orchestration + SyncRecord (sync_history)
  tauri.conf.json        v2 schema; capabilities/default.json grants opener plugin
```

## Build / test commands

- `npm install && npm run build` — type-check (tsc) + Vite build
- `cd src-tauri && cargo check` — Rust compile check (no tests exist yet)
- `npm run tauri dev` — full app (opens a GUI window; don't run headless)
- `npm run dev:mock` — Vite in mock mode (port 1426): vite.config.ts aliases
  `@tauri-apps/api/core|event` + `@tauri-apps/plugin-opener` to `src/mock/`,
  so the UI renders with fake data and no Tauri/Docker
- `npm run shots` — regenerate README screenshots into `docs/screenshots/`
  (headless local Chrome/Edge via puppeteer-core; see docs/screenshots/CAPTURE.md)
- `cd src-tauri && cargo run --example smoke -- <create|verify|info|stop|start|delete|cleanup>`
  — end-to-end lifecycle smoke test against real Docker (no Tauri runtime needed);
  uses a scratch data dir under the OS temp dir.
- `cd src-tauri && cargo run --example m4_smoke` — M4 push/pull E2E against a
  mock serverkit-localkit extension (`node examples/mock_localkit_ext.cjs`
  first, port 9872); requires the smoke site to exist.
- `cd src-tauri && cargo test --lib router` — unit tests for the M6 hosts-file
  block logic (insert/replace/remove idempotency, CRLF preservation).
- `cd src-tauri && cargo run --example m6_smoke` — M6 router E2E against the
  smoke site; **interactive only** (hosts-file writes trigger UAC/elevation
  prompts twice). Run `smoke -- create` first, `smoke -- cleanup` after.
- `cd src-tauri && cargo run -p lk -- <cmd>` — headless CLI (`lk list |
  create | start | stop | restart | delete | info | logs | wp | env | doctor`);
  shares the GUI's data dir, so use `--data-dir` (or `LOCALKIT_DATA_DIR`) for
  throwaway tests. See docs/plans/7_cli.md.
- Windows: use the **rustup MSVC toolchain** for cargo. If `cargo` resolves to a
  chocolatey/GNU install you get `dlltool.exe: program not found`; fix with
  `export PATH="$HOME/.cargo/bin:$PATH"`.

## Conventions

- **Docker:** always shell out to the `docker compose` CLI from Rust
  (`docker.rs`); never add a Docker API client (bollard etc.). All compose
  invocations run with `current_dir = <site dir>` so `.env` is picked up.
- **Errors:** commands return `Result<T, String>` with user-displayable
  messages; `docker::friendly_error` maps common "Docker not running" stderr.
- **DB:** forward-only migrations only — bump `user_version` and add an
  `if version < N` block; never edit migration 1.
- **Async:** never hold the `Db` mutex guard across `.await` (futures must be Send);
  lock in a short scope, drop, then await.
- **Ports:** site port = first free from 8081; DB host port = site port + 10000.
- **Versions:** WP/PHP versions come from allowlists in `site.rs`
  (`WP_VERSIONS`, `PHP_VERSIONS`) — the UI reads them via the `app_info` command.
- **wp-cli:** the stock `wordpress` image has no wp-cli; use the profile-gated
  `wpcli` service (`wordpress:cli-php<ver>`) via `docker::compose_run`, and
  always pass `wp` as the first argument (the cli image's `wp` CMD is replaced
  by run args, so omitting it makes the entrypoint exec `core` and fail).
- **Events:** long operations emit `site-event` (`{id, stage, message}`);
  stages: files → containers → waiting → install → done | error. When there
  is no Tauri app handle (CLI, examples), `site::emit` prints
  `[stage] message` to stderr instead of dropping the event.
- **CLI (`lk`):** a thin workspace crate (`src-tauri/lk/`) over
  `localkit_lib` — never add logic to it that belongs in the library; keep
  both frontends (Tauri commands and the CLI) as thin wrappers. Conventions:
  stdout carries data only (chrome/progress/✓ successes → stderr), `--json`
  is per-command and always pretty, errors print `error: <msg>` on stderr
  with exit 1, sites resolve by exact id or case-insensitive slug/name, and
  destructive commands prompt (default No) with `--yes` required on non-TTY.
- **Local domains (M6):** `router.rs` runs one shared Caddy project at
  `<data dir>/router/` (ports 80/443, `host.docker.internal:host-gateway`,
  routes to site host ports — never touch site compose templates). TLD is
  `.test` (RFC 2606; NOT `.local` — LocalWP — and NOT `.localhost`); because
  nothing auto-resolves it, a marked block (`# BEGIN/END LOCALKIT`) is managed
  in the OS hosts file via an elevated one-shot helper (UAC / osascript /
  pkexec) — declining elevation keeps `domains_enabled` off. Block-content
  logic is the pure, unit-tested `update_hosts_content`. Flag/CA-trust/last
  error live in `app_settings` (migration 4). HTTPS = `tls internal`;
  `trust_router_ca` installs Caddy's root cert per-OS (`certutil -user` on
  Windows — no admin) and records success in settings. Caddyfile regenerates
  + reloads on site create/start/stop/delete; hosts sync on create/delete
  only (no UAC spam on start/stop).
- **System tray (M8):** `tray.rs` owns the tray icon/menu (Tauri 2 built-in
  `TrayIconBuilder` — no extra crate) plus the close-to-tray interception in
  `run()`'s `on_window_event`. The `run_in_background` flag lives in
  `app_settings` (KV — no migration; default on, toggle in Settings →
  General). Menu/tooltip come from DB status (never live `docker ps`) and are
  rebuilt via `tray::refresh(&app)` after every lifecycle command — any new
  command that changes site status must call it. Quit from the tray leaves
  Docker containers running on purpose. `tauri-plugin-single-instance`
  focuses the existing window on relaunch. Tray-driven start/stop spawn
  `tauri::async_runtime::spawn` so menu event handlers stay sync.
- **ServerKit (M3/M4):** client in `serverkit.rs` (reqwest rustls,
  `X-API-Key` header). `test_connection` = public `GET /api/v1/system/health`
  (no key sent — ServerKit 401s *any* request carrying an invalid key) + key
  validation via `GET /api/v1/setup-health/account` (`@auth_required`) + a
  `/api/v1/localkit/pair` probe (extension presence). Site listing and
  push/pull go through the `serverkit-localkit` extension
  (`/api/v1/localkit/...`) because the core `/api/v1/wordpress/sites` route is
  bare `@jwt_required()` upstream. Orchestration in `sync.rs`: push code =
  in-memory tar.gz of `wp-content/` (flate2+tar) → multipart POST; push DB =
  `wp db export -` → multipart POST with `local_url`; pull DB = download
  .sql.gz → gunzip → `wp db import -` via `docker::compose_run_stdin` →
  `wp search-replace` remote → local. Ops emit `site-event` stages and record
  rows in `sync_history` (migration 3). Connections live in
  `serverkit_connections` (migration 2); **API keys in plaintext SQLite** —
  accepted for v1, revisit with a keyring later.
- **Design system:** tailwind.config.js remaps the zinc scale to the brand navy
  surfaces (#0D0F16 bg / #151822 surface / #2A2F40 borders / #9097AB muted) and
  violet to brand (#6C5CE7 primary, #7A6BEA hover, #B8AFFA lavender accent);
  radii follow the kit (md 8 / lg 10 / xl 12 / 2xl 16). Keep it stupidly
  simple: no router lib (state-based nav in `stores/nav.ts`), minimal deps,
  match existing dark zinc/violet Tailwind styling (violet = brand/actions,
  emerald only for semantic success/running states, red for danger). Fonts:
  Inter (UI) + JetBrains Mono (technical content) via @fontsource. The real
  logo lives at `assets/logo.png` (transparent PNG) and `src/assets/logo.png`
  (Vite-bundled copy used by the Sidebar).
