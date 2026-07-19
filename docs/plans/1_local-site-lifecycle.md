# Plan 1 — Local Site Lifecycle (M1)

## Context

LocalKit needs its core loop before anything else: create a local WordPress
site, start it, stop it, delete it. No Docker API client exists in the
project — everything shells out to the `docker compose` CLI. Sites must be
isolated from each other and editable from a host folder.

## Approach

### Site model + persistence — `src-tauri/src/site.rs`, `src-tauri/src/db.rs`

`Site` rows live in SQLite (rusqlite, forward-only migrations via
`PRAGMA user_version`). Each site gets a directory under
`<data dir>/sites/<slug>/` containing a generated `docker-compose.yml`
(`wordpress:<wp>-php<php>-apache` + `mariadb:11`), a `.env`, and a
bind-mounted `wp-content/` folder.

### Docker wrapper — `src-tauri/src/docker.rs`

Thin wrapper around `docker compose` subcommands (`check`, `up`, `down`,
`ps`, `logs`). All invocations run with `current_dir = <site dir>` so `.env`
is picked up. `friendly_error` maps common "Docker not running" stderr to
user-displayable messages.

### Ports

Site port = first free from 8081; DB host port = site port + 10000.

### Events

Long operations emit `site-event` (`{id, stage, message}`); stages:
`files → containers → waiting → install → done | error`.

## Phases

1. `db.rs` — SQLite open + migration 1 (`sites` table)
2. `docker.rs` — compose CLI wrapper + Docker availability check
3. `site.rs` — compose/env templates, create/delete site files, port allocation
4. Tauri commands in `lib.rs` — `create_site`, `start_site`, `stop_site`,
   `delete_site`, `list_sites`, container status
5. Frontend — Dashboard site list, `NewSiteDialog`, `StatusBadge`, Zustand
   `sites` store

## Integration points

`src-tauri/src/{lib,db,docker,site}.rs`, `src/stores/sites.ts`,
`src/pages/Dashboard.tsx`, `src/components/NewSiteDialog.tsx`

## Risks

- Docker Desktop not running → mitigated by `friendly_error` + Settings page
  Docker status check
- Port collisions with non-LocalKit processes → first-free scan, but race is
  possible; acceptable for v1
- Windows paths in bind mounts → compose handles them; verified on Windows

## Verification

`cargo check` + `npm run build`; end-to-end via
`cargo run --example smoke -- create|verify|stop|start|delete|cleanup`
against real Docker (scratch data dir under OS temp).
