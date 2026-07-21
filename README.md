<div align="center">

<img width="160" alt="LocalKit" src="assets/logo.png" />

# LocalKit

**Spin up local WordPress sites in one click.**

A lean desktop app (think LocalWP, but lighter) that runs each WordPress site
as its own isolated Docker Compose project — with `wp-content` bind-mounted to
a plain host folder, so you edit code in your own editor. Also manages
PHP/Laravel stacks and bring-your-own-compose Docker projects under the same
roof.

English | [Español](docs/README.es.md) | [中文版](docs/README.zh-CN.md) | [Português](docs/README.pt.md)

<br>

![Windows](https://img.shields.io/badge/Windows-0078D6?style=for-the-badge&logo=windows&logoColor=white)
![macOS](https://img.shields.io/badge/macOS-000000?style=for-the-badge&logo=apple&logoColor=white)
![Linux](https://img.shields.io/badge/Linux-FCC624?style=for-the-badge&logo=linux&logoColor=black)
![Docker](https://img.shields.io/badge/Docker-2496ED?style=for-the-badge&logo=docker&logoColor=white)
[![Discord](https://img.shields.io/discord/1470639209059455008?style=for-the-badge&logo=discord&logoColor=white&label=Discord&color=5865F2)](https://discord.gg/ZKk6tkCQfG)

[![GitHub Stars](https://img.shields.io/github/stars/jhd3197/LocalKit?style=flat-square&color=f5c542)](https://github.com/jhd3197/LocalKit/stargazers)
[![Downloads](https://img.shields.io/github/downloads/jhd3197/LocalKit/total?style=flat-square)](https://github.com/jhd3197/LocalKit/releases)
[![License](https://img.shields.io/badge/license-MIT-blue.svg?style=flat-square)](LICENSE)
[![Version](https://img.shields.io/badge/version-0.1.2-756ce3?style=flat-square)](https://github.com/jhd3197/LocalKit/releases)
[![Tauri](https://img.shields.io/badge/Tauri-2-24C8D8.svg?style=flat-square&logo=tauri&logoColor=white)](https://tauri.app)
[![React](https://img.shields.io/badge/react-18-61DAFB.svg?style=flat-square&logo=react&logoColor=black)](https://reactjs.org)

<br>

[Quick Start](#-quick-start) · [Screenshots](#-screenshots) · [Features](#-features) · [Architecture](#-architecture) · [Roadmap](#-roadmap) · [Docs](#-documentation) · [Contributing](#-contributing) · [Discord](#-community)

</div>

---

## 🚀 Quick Start

> ⏱️ From clone to a running WordPress site in minutes

### Requirements

- **Docker Desktop** (running) with Compose v2+
- **Node.js 20+** and **Rust** (stable, MSVC toolchain on Windows) — only for building from source
- For sync: a **[ServerKit](https://github.com/jhd3197/ServerKit)** server with the **`serverkit-localkit` extension** installed

### Develop

```bash
git clone https://github.com/jhd3197/LocalKit.git
cd LocalKit
npm install
npm run tauri dev        # starts Vite + the Tauri window
```

### Production build

```bash
npm run tauri build
```

<!-- LOCALKIT:SHOTS:START -->
## 📸 Screenshots

> Captured from a mock-data build — every site, credential, and server below is fictional. See [`docs/screenshots/CAPTURE.md`](docs/screenshots/CAPTURE.md) for the shot list and how to regenerate them with `npm run shots`.

|                            Dashboard                             |                            List View                            |
| :--------------------------------------------------------------: | :------------------------------------------------------------: |
|      ![Dashboard](docs/screenshots/dashboard.png)       |      ![List view](docs/screenshots/dashboard-list.png)       |
|   _All your sites at a glance, with live container status badges_   |   _A denser take on the dashboard for lots of sites_   |

|                             Site Detail                              |                           New Site                            |
| :-------------------------------------------------------------: | :------------------------------------------------------------: |
|         ![Site detail](docs/screenshots/site-detail.png)         |      ![New site](docs/screenshots/new-site.png)     |
| _Credentials, wp-cli info, ServerKit sync & history_ | _Pick a name, WordPress version, and PHP version_ |

|                           Settings                            |                           Local Domains                           |
| :--------------------------------------------------------------: | :-------------------------------------------------------------: |
|            ![Settings](docs/screenshots/settings.png)            |      ![Local domains](docs/screenshots/settings-domains.png)      |
|      _Docker status, data paths, and ServerKit connections_      |     _Serve sites as `http://<slug>.test` via a shared Caddy router_     |
<!-- LOCALKIT:SHOTS:END -->

## 🎯 Features

### 🚀 Sites & Docker

| | |
|---|---|
| **One-Click WordPress Sites**<br>Pick a name, a WordPress version, and a PHP version — done. | **Per-Site Docker Compose Project**<br>`wordpress:<wp>-php<php>-apache` + `mariadb:11`, fully isolated per site. |
| **Automatic WordPress Install**<br>Installed via wp-cli, with generated admin credentials handed to you. | **Unique Host Ports**<br>Sites on `http://localhost:8081+`, databases on `18081+` — no conflicts. |
| **Lifecycle & Logs**<br>Start / stop / delete, live container status badges, and a container log viewer. | **Local Domains**<br>Optional `http(s)://<slug>.test` URLs via a shared Caddy router on ports 80/443, managed hosts-file block (one-time admin approval), and one-click local-CA trust for HTTPS. |
| **Snapshots & One-Click Restore**<br>Point-in-time copies of the database and `wp-content`, restorable from the site page or the CLI. | **Nothing Destructive Is One-Way**<br>A snapshot is taken automatically before every push, pull, delete and restore — deleting a site keeps one unless you opt out. |
| **Clone & Blueprints**<br>Duplicate any site in one click, or save it as a reusable blueprint (content + plugin/theme recipe) and stamp out new sites from it — portable as a single `.lkbp` file. | **More Than WordPress**<br>Import any existing Docker Compose project, or generate a PHP/Laravel stack (php-fpm + nginx + MariaDB) — same domains, terminals, snapshots, and tray. |
| **Site Tools**<br>Built-in database GUI (Adminer), serialization-safe search-replace with dry-run, WP_DEBUG toggle with a live log viewer, and a `wp-config.php` / `.env` editor. | **Always-Honest Status**<br>A reconciler settles site status against Docker itself (never guesses), flags unhealthy containers, and recovers installs interrupted by a crash. |
| **Plays Well With Others**<br>Port pre-flight before claiming 80/443 — if LocalWP or another tool owns them, LocalKit names the process and offers one-click fallback ports so both apps coexist. | **Local Domains That Degrade Gracefully**<br>Configurable router ports (80/443 by default); on fallback ports sites live at `http://<slug>.test:8080` and everything else keeps working. |

### 🔁 ServerKit Sync

| | |
|---|---|
| **Push Code**<br>Push your local `wp-content` straight to a remote site on your ServerKit server. | **Push / Pull Database**<br>Push the DB, or pull a remote DB into your local site with automatic URL search-replace. |
| **Resumable Transfers**<br>Sync in 8 MiB chunks with byte-level progress. Lose the connection at 99% and the retry re-sends only what was missing — no size ceiling, nothing buffered in RAM. | **Cancel Any Transfer**<br>Stop a push or pull mid-flight. The server only applies a payload once its checksum verifies, so a cancelled sync leaves nothing half-written. |
| **Import a Remote Site**<br>Clone any site on your server down as a *new* local site — wp-content, database and URL rewriting in one step, from the app or `lk import`. | **Sync History**<br>Every sync op is recorded per site, with its result. |
| **Connections**<br>Save, test, and delete server connections; browse remote sites and provision new ones — all through the `serverkit-localkit` extension. | **Capability-Aware**<br>The app asks the extension what it supports, so features an older server can't do are disabled with a reason instead of failing halfway. |

### 🖥️ Desktop & CLI

| | |
|---|---|
| **Dashboard Views**<br>Grid or dense list view for the dashboard, remembered between launches. | **Site Detail Page**<br>Open site / wp-admin, copyable admin + DB credentials, wp-cli info (core version, plugins). |
| **System Tray & Notifications**<br>Close to tray with quick site actions in the tray menu; desktop notifications when long operations finish while you're elsewhere. | **OS Keyring & Update Checks**<br>ServerKit API keys live in the OS keyring (Credential Manager / Keychain / Secret Service), and the app tells you when a new release is out. |
| **Command Palette & Shortcuts**<br>mod+K palette over a single command registry, global shortcuts, remappable bindings, cheat-sheet. | **Embedded Terminals**<br>One real PTY per site inside its container, with scrollback that survives navigation, link detection, and ghost-text history. |
| **`lk` CLI**<br>Manage sites from the terminal: `lk create`, `start/stop/restart`, `wp` passthrough, `env` exports, `snapshot`, `clone`, `blueprint`, `connection`, `push`/`pull`, `doctor`, JSON output — shares the app's data dir. | **Bind-Mounted Code**<br>`wp-content` lives in a plain host folder, so you edit themes and plugins in your own editor. |

---

## 🏗️ Architecture

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

---

## 🖥️ The `lk` CLI

Headless companion binary that shares the app's data dir and database:

```bash
cd src-tauri
cargo run -p lk -- list                 # or: cargo build -p lk → target/debug/lk
lk create "My Blog"                     # full site create, prints the site URL
lk create --blueprint starter "Client"  # stamp a new site from a blueprint
lk clone my-blog my-blog-copy           # one-click duplicate
lk wp my-blog plugin list               # wp-cli passthrough
lk env my-blog                          # eval-able exports: eval $(lk env my-blog)
lk snapshot create my-blog              # point-in-time DB + wp-content copy
lk snapshot restore my-blog <id> --yes  # roll back to one
lk connection add Prod https://panel.example.com   # validate + store a server
lk push my-blog --code                  # push wp-content to its linked remote
lk pull my-blog --db                    # pull the remote DB down (URL rewrite)
lk import Production client-blog        # clone a server site down as a new local site
lk completions zsh                      # shell completions (bash/zsh/fish/powershell)
lk doctor                               # diagnose Docker / router ports / connections
lk list --json                          # machine-readable output
```

---

## 🛠️ Develop

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

> **Windows note:** if `cargo` on your PATH is a non-rustup GNU install
> (e.g. from chocolatey) and you hit `dlltool.exe: program not found`,
> put the rustup shims first: `export PATH="$HOME/.cargo/bin:$PATH"`
> (or use `rustup run stable cargo check`).

---

## 📁 Layout

```
src/                     React 18 + TS + Vite frontend
  lib/ipc.ts             typed wrappers for all Tauri commands (invoke + events)
  lib/types.ts           shared TS types mirroring Rust payloads
  stores/                Zustand stores (nav, sites, settings, blueprints, toast)
  pages/                 Dashboard (grid + list views), SiteDetail, Terminal, Settings (modal)
  components/            Sidebar, StatusBadge, KindBadge, NewSiteDialog, SiteTools,
                         SnapshotsPanel, PushPanel, CommandPalette, dialogs, icons
  mock/                  fake @tauri-apps/* modules for `vite --mode mock` (screenshots)
src-tauri/               Rust backend
  src/lib.rs             AppState, command registration, app entry
  src/db.rs              rusqlite, forward-only migrations (PRAGMA user_version)
  src/docker.rs          `docker compose` CLI wrapper
  src/site.rs            Site model, lifecycle, kind/capability model, compose/env templates
  src/dockerapp.rs       generic Docker-app kind (import an existing compose project)
  src/php.rs             PHP/Laravel stack kind (generated php-fpm + nginx + mariadb)
  src/wordpress.rs       wp-cli via `docker compose run --rm wpcli`
  src/dbsync.rs          engine-native DB export/import dispatch (wp-cli / mysqldump / pg_dump)
  src/router.rs          local domains: shared Caddy router + hosts block + CA trust + port probe
  src/reconcile.rs       status reconciler (Docker ground truth, forward-only) + crash recovery
  src/snapshot.rs        snapshots: DB dump + wp-content archive, restore, retention
  src/blueprint.rs       save-site-as-blueprint, create-from-blueprint, .lkbp export/import
  src/keystore.rs        OS keyring for ServerKit API keys
  src/update.rs          GitHub release update checker
  src/serverkit.rs       ServerKit API client (X-API-Key)
  src/sync.rs            push/pull orchestration + remote-site import + sync history
  src/transfer.rs        chunked transfers: chunk planning, resume, hashing, cancel
  lk/                    `lk` CLI (separate workspace crate over localkit_lib)
scripts/                 capture-screenshots.mjs (npm run shots), verify-*.mjs headless checks
docs/
  plans/                 ROADMAP.md + numbered implementation plans
  screenshots/           README screenshots + CAPTURE.md
  images/funding/        donation QR codes
```

---

## 📂 Where Things Live

- App data: `%APPDATA%/LocalKit/` (macOS: `~/Library/Application Support/LocalKit/`, Linux: `~/.local/share/LocalKit/`)
  - `localkit.db` — SQLite database of sites, connections, and sync history
  - `sites/<slug>/` — per-site project: `docker-compose.yml`, `.env`, `wp-content/` (edit your code here)
  - `router/` — shared Caddy router for local domains (compose project + generated Caddyfile), only while local domains are enabled

---

## 🔁 ServerKit Sync Notes

- Auth is via `X-API-Key` (create a key in ServerKit → API settings).
- Connection test = public `/api/v1/system/health` + key validation against `/api/v1/setup-health/account` + a `/api/v1/localkit/pair` probe that detects the extension.
- All sync endpoints live in the `serverkit-localkit` extension (`/api/v1/localkit/...`); without it, LocalKit tells you exactly what's missing.
- **Transfers are chunked and resumable.** Uploads go up in 8 MiB chunks; the server assembles them, verifies a SHA-256 of the whole archive, and only *then* applies anything — so an interrupted push can never leave the remote site half-updated. Retrying re-sends only the chunks that were actually lost. Downloads resume the same way over HTTP `Range`. This lifts the server's 100 MB request limit, and nothing large is held in memory in either direction.
- Progress is reported in bytes ("Pushing wp-content — 148 MB / 312 MB") and any transfer can be cancelled mid-flight.
- Against a server running an older extension, LocalKit falls back to the v1 single-request upload automatically — one client, both servers.
- **Push code** = tar.gz of `wp-content/`. **Push DB** = `wp db export`. **Pull DB** = download dump → `wp db import` → `wp search-replace` remote URL → local URL.
- **Import** provisions a new local site, then lands the remote `wp-content` (via the extension's `pull/code`) and database on it. WordPress is never installed over the imported database — the database *is* the site, so its posts, users and settings come across intact. Log in with the remote's own accounts (`lk login`, or the app's WP Admin button).
- The app gates Import on the capabilities the extension reports from `GET /pair`, so a server running an older extension shows the button disabled with the reason rather than failing mid-import. Multisite installs are refused outright.
- Downloaded archives are extracted under a strict policy — only plain files and directories under `wp-content/`; absolute paths, `..`, and symlinks are rejected.
- Every sync op is recorded in the per-site sync history with its result.
- API keys are stored in the **OS keyring** (Windows Credential Manager, macOS Keychain, Linux Secret Service). Legacy plaintext keys in the local SQLite DB are migrated into the keyring on first read; if the keyring is unavailable (headless Linux, locked keychain), LocalKit degrades to the SQLite column — never a hard failure.

---

## 🩺 Troubleshooting

### "LocalWP / Local by Flywheel is installed" — my `.test` sites show someone else's 404

Only one program on your machine can own ports **80** and **443**, and
LocalKit's local-domains router needs them. LocalWP's nginx router binds both
machine-wide *and* answers every unknown local hostname with its own "Site Not
Found" page — so if it wins the port, `http://mysite.test` renders **Local's**
404 rather than anything from LocalKit.

LocalKit detects this before it can bite:

- Enabling local domains runs a port pre-flight first. If something else holds
  80/443 it names the process and stops, rather than writing hosts entries that
  would point your sites at the other program.
- Settings → **Local domains** shows the conflict with two ways out: **Use
  fallback ports** (one click; the router moves to 8080/8443) or **Retry** after
  you quit the other app.
- `lk doctor` prints the active router mode and who owns the ports.

On fallback ports your sites are reachable at
`http://<slug>.test:8080` — the hosts entries are port-blind, so nothing else
changes and both apps can run side by side. LocalKit deliberately uses the
`.test` TLD (RFC 2606) while Local uses `.local`, so the hostnames themselves
never collide; the fight is only ever over the ports.

Prefer clean `http://<slug>.test` URLs? Quit the other program, set the router
back to 80/443 in Settings → Local domains, and hit Retry.

> **Note:** switching ports restarts the router and rewrites each running
> site's WordPress `home`/`siteurl`, so bookmarks and absolute URLs follow.

---

## 🗺️ Roadmap

- **M1 — Local site lifecycle** ✅ create/start/stop/delete, compose projects, port allocation
- **M2 — WordPress install & detail** ✅ wp-cli install, credentials, logs, wp info
- **M3 — ServerKit connection** ✅ save/test connections, extension detection, browse remote sites
- **M4 — Push / pull** ✅ push code, push DB, pull DB with URL rewrite, sync history, import a remote site as a new local site, chunked resumable transfers with byte progress and cancel
- **M5 — Release polish** ✅ update checker, OS keyring for API keys, OS notifications, real test suite; installers ship via the release workflow
- **M6 — Local domains** ✅ `http(s)://<slug>.test` via a shared Caddy router, managed hosts block + local CA trust, port-conflict pre-flight + fallback ports (plan 6, 16)
- **M7 — CLI (`lk`)** ✅ headless companion binary: lifecycle, wp passthrough, `env`, `doctor`, JSON output — plus connections, push/pull, blueprints, completions (plan 7, 21)
- **M8 — System tray** ✅ close-to-tray, quick site actions, single-instance focus (plan 8)
- **M9 — Multi-stack** ✅ kind/capability model, generic Docker-app import, PHP/Laravel stack with engine-native DB sync (plan 22, 26)

Everything after the milestones is tracked per plan — snapshots (17), remote-site import (18), sync v2 (19), clone & blueprints (20), status reconciliation (23), site tools (24), and more: [`docs/plans/ROADMAP.md`](docs/plans/ROADMAP.md).

---

## 📖 Documentation

| Document | Description |
|----------|-------------|
| [Roadmap](docs/plans/ROADMAP.md) | Milestones, per-plan phases, and build order |
| [Screenshot Capture](docs/screenshots/CAPTURE.md) | Shot list and how to regenerate screenshots with `npm run shots` |
| [Implementation Plans](docs/plans/) | Numbered per-feature implementation plans |

---

## 🧱 Tech Stack

| Layer | Technology |
|-------|------------|
| App Shell | Tauri 2, Rust |
| Frontend | React 18, TypeScript, Vite, Tailwind CSS v3, Zustand |
| Database | rusqlite (bundled SQLite, forward-only migrations) |
| Containers | Docker Compose CLI (no Docker API client) |
| Sync | reqwest (rustls) + flate2/tar (sync archives) |

---

## 🤝 Contributing

Contributions are welcome!

```
fork → feature branch → commit → push → pull request
```

---

## 💛 Support LocalKit

LocalKit is free and open source. If it saves you time, you can help keep it going:

- ⭐ [Star the repo](https://github.com/jhd3197/LocalKit) — it costs nothing and helps a lot
- 💖 [GitHub Sponsors](https://github.com/sponsors/jhd3197)
- ☕ [Buy Me a Coffee](https://buymeacoffee.com/jhd3197)

### 💎 Crypto

| | Asset | Network | Address |
|:---:|---|---|---|
| <img src="docs/images/funding/usdt-trc20.png" width="110" alt="QR code for the USDT TRC-20 donation address" /> | **USDT** | **TRC-20** · Tron | `TTiCtqLauF1iSW2YGB3b78KmRxRqoLCgeL` |
| <img src="docs/images/funding/usdt-erc20.png" width="110" alt="QR code for the USDT and ETH ERC-20 donation address" /> | **USDT / ETH** | **ERC-20** · Ethereum | `0xD13D5355Fa214e8317fea2ff192a065BaeC13527` |
| <img src="docs/images/funding/btc.png" width="110" alt="QR code for the Bitcoin donation address" /> | **BTC** | **Bitcoin** | `bc1qatx67n3qxdvuv3arc9j8aytk34f22g02k9c7vr` |
| <img src="docs/images/funding/sol.png" width="110" alt="QR code for the Solana donation address" /> | **SOL** | **Solana** | `AWXzqtBEgUfteHPQtDegsZ6D5y57M3GGdKPD8rR7h6xu` |

TRC-20 has the lowest fees — usually under a dollar — so it's the friendliest
option for a small donation. ERC-20 gas can cost more than the donation itself.

<sub>QR codes are generated locally by [`scripts/generate-funding-qr.mjs`](scripts/generate-funding-qr.mjs), which checksum-validates every address before encoding.</sub>

---

## 🔭 Related Projects

**[ServerKit](https://github.com/jhd3197/ServerKit)** — A lightweight, modern server control panel for managing web apps, databases, Docker containers, and security — without the complexity of Kubernetes or the cost of managed platforms. Pair it with LocalKit through the `serverkit-localkit` extension to push code and push/pull databases between local and remote sites.

**[Faro](https://github.com/jhd3197/faro)** — A modern desktop client for SFTP, FTP, SSH, and S3-compatible storage, from the same author. Save a server once, then browse its files in a dual-pane view and open a terminal against the same SSH session — plus drag-and-drop transfers, one-way directory sync, and edit-in-place. It even has an **Agent Bridge** that lets Claude Code (or any MCP agent) run commands on a box through your authenticated session, with per-command approval and no shared credentials.

**[DeviceKit](https://github.com/jhd3197/DeviceKit)** — A unified Android device fleet & test-automation platform. Control a fleet of devices from one dashboard — run automations, stream screens in real time, catch visual regressions, and debug failures with AI-powered analysis.

---

## 💬 Community

[![Discord](https://img.shields.io/badge/Discord-Join_Us-5865F2?style=for-the-badge&logo=discord&logoColor=white)](https://discord.gg/ZKk6tkCQfG)

Join the Discord to ask questions, share feedback, or get help with your setup.

---

## 📄 License

MIT — see [LICENSE](LICENSE).

---

<div align="center">

**LocalKit** — Local WordPress development, without the bloat.

[Report Bug](https://github.com/jhd3197/LocalKit/issues) · [Request Feature](https://github.com/jhd3197/LocalKit/issues)

Made with ❤️ by [Juan Denis](https://juandenis.com)

</div>
