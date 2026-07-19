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
  stores/                Zustand stores (nav.ts = page state, sites.ts = data/actions)
  pages/                 Dashboard (site list), SiteDetail, Settings
  components/            Sidebar, StatusBadge, CopyButton, NewSiteDialog
src-tauri/               Rust backend
  src/lib.rs             AppState, Tauri command registration, app entry (run())
  src/db.rs              rusqlite, forward-only migrations via PRAGMA user_version
  src/docker.rs          `docker compose` CLI wrapper (check/up/down/run/ps/logs)
  src/site.rs            Site model + lifecycle + compose/env templates
  src/wordpress.rs       wp-cli via `docker compose run --rm wpcli wp ...`
  src/serverkit.rs       ServerKit API client (X-API-Key) + connection model
  src/sync.rs            push/pull orchestration + SyncRecord (sync_history)
  tauri.conf.json        v2 schema; capabilities/default.json grants opener plugin
```

## Build / test commands

- `npm install && npm run build` — type-check (tsc) + Vite build
- `cd src-tauri && cargo check` — Rust compile check (no tests exist yet)
- `npm run tauri dev` — full app (opens a GUI window; don't run headless)
- `cd src-tauri && cargo run --example smoke -- <create|verify|info|stop|start|delete|cleanup>`
  — end-to-end lifecycle smoke test against real Docker (no Tauri runtime needed);
  uses a scratch data dir under the OS temp dir.
- `cd src-tauri && cargo run --example m4_smoke` — M4 push/pull E2E against a
  mock serverkit-localkit extension (`node examples/mock_localkit_ext.cjs`
  first, port 9872); requires the smoke site to exist.
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
  stages: files → containers → waiting → install → done | error.
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
- Keep it stupidly simple: no router lib (state-based nav in `stores/nav.ts`),
  minimal deps, match existing dark zinc/emerald Tailwind styling.
