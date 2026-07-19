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
  lib/terminalRegistry.ts  xterm.js instances living outside React (one PTY per
                         site; scrollback survives page switches; attach/detach)
  stores/                Zustand stores (nav.ts = page state + settings modal +
                         grid/list view pref, sites.ts = data/actions,
                         toast.ts = global toasts + module-level toast.* helpers)
  pages/                 Dashboard (grid/list site views), SiteDetail,
                         Terminal (one tab per site, shell in the wordpress
                         container), Settings (modal, opened via sidebar gear —
                         not a page)
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
  src/terminal.rs        per-site interactive terminals: `portable-pty` PTY
                         running `docker compose exec wordpress bash`; events
                         `terminal://data` / `terminal://exit`
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
  create | start | stop | restart | delete | info | logs | wp | env | login |
  doctor`); shares the GUI's data dir, so use `--data-dir` (or
  `LOCALKIT_DATA_DIR`) for throwaway tests. See docs/plans/7_cli.md.
- CI: `.github/workflows/ci.yml` runs on push/PR to `main`/`dev` — `npm run
  build`, `cargo check --workspace --all-targets`, `cargo test --workspace`
  (matches Faro's CI shape).
- Releases: `.github/workflows/release.yml` — every push to `main` (i.e. a
  dev→main merge) auto-bumps the patch version, tags `vX.Y.Z`, builds the
  desktop app (macOS universal / Windows / Linux) **and** the `lk` CLI for all
  platforms, and publishes a GitHub Release (unsigned; notes include the
  xattr/SmartScreen caveats). Put `[skip ci]` in the commit message to push
  to main without releasing; manual run with a pinned version is available
  via workflow_dispatch.
- App icons: generated from `assets/logo.png` (non-square master) via
  `python scripts/make-square-logo.py` → `npx @tauri-apps/cli icon
  assets/logo-square.png`; the bundle.icon list in tauri.conf.json is
  maintained by hand.
- Windows: use the **rustup MSVC toolchain** for cargo. If `cargo` resolves to a
  chocolatey/GNU install you get `dlltool.exe: program not found`; fix with
  `export PATH="$HOME/.cargo/bin:$PATH"`.

## Conventions

- **Docker:** always shell out to the `docker compose` CLI from Rust
  (`docker.rs`); never add a Docker API client (bollard etc.). All compose
  invocations run with `current_dir = <site dir>` so `.env` is picked up.
  Every subprocess spawn (`Command::new`, any module) must go through
  `docker::no_window` (CREATE_NO_WINDOW) so the installed GUI app never
  flashes a console window on Windows.
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
  create stages: files → pulling → containers → waiting → install (re-emitted
  per attempt) → done | error. The `pulling` stage pre-pulls all images
  including the profile-gated wpcli (`docker::compose_pull`) so first-run
  downloads are a labeled stage, not a silent stall. When there is no Tauri
  app handle (CLI, examples), `site::emit` prints `[stage] message` to stderr
  instead of dropping the event. On the frontend, `sites.ts handleEvent`
  renders these as a single pinned progress toast that resolves into
  success/error on done/error.
- **Toasts:** global feedback lives in `stores/toast.ts` — call
  `toast.success/info/error(title, message?)` or `toast.progress`/`resolve`
  from stores (never per-component plumbing); the viewport is
  `components/Toasts.tsx` mounted once in `App.tsx`. For command failures use
  `toastError(e, "Action name")` from `lib/errors.ts` — it unwraps the
  `string` rejection and dedupes against the `error`-stage toast the
  site-event stream already showed (create/push/pull both emit an error
  event AND reject the promise).
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
- **Terminals:** `terminal.rs` (`PtyManager` on `AppState.terminals`) spawns a
  real PTY via `portable-pty` (ConPTY on Windows) running `docker compose exec
  wordpress bash` in the site dir — `terminal_open` first checks
  `docker::compose_ps` that the wordpress container is running. Commands:
  `terminal_open/write/resize/close`; output streams on `terminal://data`,
  exit on `terminal://exit` (same event names as Faro, whose PtyManager this
  mirrors). Frontend: xterm instances live in `lib/terminalRegistry.ts`
  OUTSIDE React keyed by site id — pages only `attach`/`detach`, disposal is
  explicit (`restartTerminal` after exit), so scrollback survives navigation.
  Any `AppState` constructor (GUI, `lk`, examples) must pass
  `terminal::PtyManager::new()`. Mock mode keeps fake shells in
  `mock/core.ts` (`mockShells`); `terminal_resize` must be a no-op there (the
  FitAddon fires one right after open).
- **One-click login (plan 10):** `wordpress::login_url(dir, site, user,
  base_url)` mints a one-time token (`wp option update localkit_login_token`
  + `_exp`, ~120 s TTL) consumed by the MU plugin
  `wp-content/mu-plugins/localkit-login.php` (`LOGIN_PLUGIN` const, written
  idempotently by `ensure_login_plugin` at create and lazily on login — the
  bind-mounted `wp-content` makes it a plain fs write). Base URL comes from
  `router::site_public_url` (mirrors the frontend's `siteUrl`). Never log the
  full login URL into events/history. Frontends: `login_site` /
  `site_wp_users` Tauri commands (WP Admin button + user picker on
  SiteDetail), `lk login [--user <id|login|email>] [--open]`. Default user =
  the site's `admin_user`, falling back to the first administrator (pull DB
  can overwrite local users).
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
