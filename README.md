<div align="center">

<img width="160" alt="LocalKit" src="assets/logo.png" />

# LocalKit

**Spin up local WordPress sites in one click — each site an isolated Docker Compose project.**

[![Version](https://img.shields.io/badge/version-0.1.0-756ce3?style=flat-square)](https://github.com/jhd3197/localkit/releases)
[![License](https://img.shields.io/badge/license-MIT-756ce3?style=flat-square)](LICENSE)
[![Built with Tauri](https://img.shields.io/badge/built%20with-Tauri%202-756ce3?style=flat-square)](https://tauri.app)

*Local WordPress development, without the bloat.*

</div>

---

## What it is

LocalKit is a desktop app (think LocalWP, but leaner) that runs each WordPress site as its own Docker Compose project — WordPress + MariaDB, with `wp-content` bind-mounted to a plain host folder so you can edit code in your own editor. Pick a name, a WordPress version and a PHP version; LocalKit writes the compose files, boots the containers, installs WordPress via wp-cli, and hands you the admin credentials.

v1 is milestones M1–M4 of the [roadmap](docs/plans/ROADMAP.md): local sites plus ServerKit push/pull. Push code, push the database, or pull a remote database straight into your local site through the `serverkit-localkit` extension on your [ServerKit](https://github.com/) server.

<!-- LOCALKIT:SHOTS:START -->
## 📸 Screenshots

> Captured from a mock-data build — every site, credential, and server below is fictional. See [`docs/screenshots/CAPTURE.md`](docs/screenshots/CAPTURE.md) for the shot list and how to regenerate them with `npm run shots`.

<details open>
<summary><strong>Dashboard</strong> — all your sites at a glance, with live container status badges</summary>

![Dashboard](docs/screenshots/dashboard.png)

</details>

<details>
<summary><strong>List view</strong> — a denser take on the dashboard for lots of sites</summary>

![Dashboard list view](docs/screenshots/dashboard-list.png)

</details>

<details>
<summary><strong>Site detail</strong> — credentials, wp-cli info, ServerKit sync &amp; history</summary>

![Site detail](docs/screenshots/site-detail.png)

</details>

<details>
<summary><strong>New site</strong> — pick a name, WordPress version, and PHP version</summary>

![New site](docs/screenshots/new-site.png)

</details>

<details>
<summary><strong>Settings</strong> — a modal with Docker status, data paths, and ServerKit connections</summary>

![Settings](docs/screenshots/settings.png)

</details>

<details>
<summary><strong>Local domains</strong> — serve sites as <code>http://&lt;slug&gt;.test</code> via a shared Caddy router</summary>

![Local domains settings](docs/screenshots/settings-domains.png)

</details>
<!-- LOCALKIT:SHOTS:END -->

## ✨ Features (v1)

- **One-click WordPress sites** — name, WP version, PHP version, done
- **Per-site Docker Compose project** — `wordpress:<wp>-php<php>-apache` + `mariadb:11`
- **Automatic WordPress install** via wp-cli, with generated admin credentials
- **Unique host ports per site** — sites on `http://localhost:8081+`, DB on `18081+`
- **Start / stop / delete**, live container status badges, container log viewer
- **Grid or dense list view** for the dashboard, remembered between launches
- **Site detail page** — open site / wp-admin, copyable admin + DB credentials, wp-cli info (core version, plugins)
- **ServerKit sync** — push `wp-content`, push the DB, or pull a remote DB into your local site (with automatic URL search-replace), plus a per-site sync history
- **ServerKit connections** — save/test/delete server connections, browse remote sites, provision new ones
- **Local domains** — optional `http(s)://<slug>.test` URLs via a shared Caddy router on ports 80/443, managed hosts-file block (one-time admin approval), and one-click local-CA trust for HTTPS
- **`lk` CLI** — manage sites from the terminal: `lk create`, `start/stop/restart`, `wp` passthrough, `env` exports, `doctor`, JSON output; shares the app's data dir

## Requirements

- **Docker Desktop** (running) with Compose v2+
- **Node.js 20+** and **Rust** (stable, MSVC toolchain on Windows)
- For sync: a **ServerKit** server with the **`serverkit-localkit` extension** installed

## Develop

```bash
npm install
npm run tauri dev        # starts Vite + the Tauri window
```

Frontend only (for UI iteration without the shell):

```bash
npm run dev              # Vite on http://localhost:1420
npm run dev:mock         # Vite with mock data, no Docker/Tauri (port 1426)
npm run shots            # regenerate docs/screenshots/*.png via headless Chrome
npm run build            # tsc + vite build
```

Rust backend:

```bash
cd src-tauri
cargo check
cargo build
```

Headless CLI (shares the app's data dir and database):

```bash
cd src-tauri
cargo run -p lk -- list                 # or: cargo build -p lk → target/debug/lk
lk create "My Blog"                     # full site create, prints the site URL
lk wp my-blog plugin list               # wp-cli passthrough
lk env my-blog                          # eval-able exports: eval $(lk env my-blog)
lk doctor                               # diagnose Docker / compose / data dir
lk list --json                          # machine-readable output
```

> **Windows note:** if `cargo` on your PATH is a non-rustup GNU install
> (e.g. from chocolatey) and you hit `dlltool.exe: program not found`,
> put the rustup shims first: `export PATH="$HOME/.cargo/bin:$PATH"`
> (or use `rustup run stable cargo check`).

## Production build

```bash
npm run tauri build
```

## Architecture

```
React frontend (Zustand stores)
        │  invoke / events
        ▼
Tauri commands (src-tauri/src/lib.rs)
        │
        ├─► SQLite (rusqlite, forward-only migrations)
        ├─► docker compose CLI  ──► per-site project dir (compose + .env + wp-content/)
        └─► ServerKit API (reqwest, X-API-Key) ──► serverkit-localkit extension (push/pull)
```

The backend shells out to the `docker compose` CLI — no Docker API client, no admin rights needed. Long operations stream `site-event` progress events (`files → containers → waiting → install → done`) to the UI.

## Layout

```
src/                     React 18 + TS + Vite frontend
  lib/ipc.ts             typed wrappers for all Tauri commands (invoke + events)
  lib/types.ts           shared TS types mirroring Rust payloads
  stores/                Zustand stores (nav, sites)
  pages/                 Dashboard (grid + list views), SiteDetail, Settings (modal)
  components/            Sidebar, StatusBadge, CopyButton, NewSiteDialog, icons
  mock/                  fake @tauri-apps/* modules for `vite --mode mock` (screenshots)
src-tauri/               Rust backend
  src/lib.rs             AppState, command registration, app entry
  src/db.rs              rusqlite, forward-only migrations (PRAGMA user_version)
  src/docker.rs          `docker compose` CLI wrapper
  src/site.rs            Site model, lifecycle, compose/env templates
  src/wordpress.rs       wp-cli via `docker compose run --rm wpcli`
  src/router.rs          local domains: shared Caddy router + hosts block + CA trust
  src/serverkit.rs       ServerKit API client (X-API-Key)
  src/sync.rs            push/pull orchestration + sync history
scripts/                 capture-screenshots.mjs (npm run shots)
docs/
  plans/                 ROADMAP.md + numbered implementation plans
  screenshots/           README screenshots + CAPTURE.md
```

## Where things live

- App data: `%APPDATA%/LocalKit/` (macOS: `~/Library/Application Support/LocalKit/`, Linux: `~/.local/share/LocalKit/`)
  - `localkit.db` — SQLite database of sites, connections, and sync history
  - `sites/<slug>/` — per-site project: `docker-compose.yml`, `.env`, `wp-content/` (edit your code here)
  - `router/` — shared Caddy router for local domains (compose project + generated Caddyfile), only while local domains are enabled

## ServerKit sync notes

- Auth is via `X-API-Key` (create a key in ServerKit → API settings).
- Connection test = public `/api/v1/system/health` + key validation against `/api/v1/setup-health/account` + a `/api/v1/localkit/pair` probe that detects the extension.
- All sync endpoints live in the `serverkit-localkit` extension (`/api/v1/localkit/...`); without it, LocalKit tells you exactly what's missing.
- **Push code** = in-memory tar.gz of `wp-content/` → multipart POST. **Push DB** = `wp db export` → multipart POST. **Pull DB** = download dump → `wp db import` → `wp search-replace` remote URL → local URL.
- Every sync op is recorded in the per-site sync history with its result.
- API keys are stored in **plaintext** in LocalKit's local SQLite DB — accepted for v1, keyring storage is on the roadmap.

## Tech stack

Tauri 2 (Rust) · React 18 + TypeScript + Vite · Tailwind CSS v3 · Zustand · rusqlite (bundled SQLite) · reqwest (rustls) · flate2/tar (sync archives)

## 🗺️ Roadmap

- **M1 — Local site lifecycle** ✅ create/start/stop/delete, compose projects, port allocation
- **M2 — WordPress install & detail** ✅ wp-cli install, credentials, logs, wp info
- **M3 — ServerKit connection** ✅ save/test connections, extension detection, browse remote sites
- **M4 — Push / pull** ✅ push code, push DB, pull DB with URL rewrite, sync history
- **M5 — Release polish** ⬜ installers, auto-update, OS keyring for API keys, test suite
- **M6 — Local domains** ✅ `http(s)://<slug>.test` via a shared Caddy router, managed hosts block + local CA trust (plan 6)
- **M7 — CLI (`lk`)** ✅ headless companion binary: lifecycle, wp passthrough, `env`, `doctor`, JSON output (plan 7)

Full details, per-plan phases, and build order: [`docs/plans/ROADMAP.md`](docs/plans/ROADMAP.md).

## License

MIT — see [LICENSE](LICENSE).
