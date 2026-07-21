# 22 — Multi-stack core: kind/capability model + generic Docker apps

Status: ✅ shipped

**Shipped as migration 6** (the plan said "migration 7" before the actual
numbering settled; 6 was the next free `user_version`). Docker apps ship
**code-only**: `config.db_engine`/`db_service` are detected and stored, but
`db_sync` stays off until engine-native dumps land — a kind must not claim a
capability it can't deliver (see Risks). Clone, blueprints and ServerKit
push/pull stay WordPress-only (per-kind support is plan 26) and reject a docker
site with a clean error. The import dialog takes a **typed folder path** rather
than a native picker (a `tauri-plugin-dialog` folder picker is a follow-up).

Generalize LocalKit's core from "WordPress site manager" to "local project
manager" in two steps: a `kind` + capability model that makes every feature
stack-aware, then the first non-WP kind — bring-your-own-compose **Docker
apps**. Deliberately placed before plans 23–25 so everything built after
this is capability-aware from day one instead of retrofitted. The
PHP/Laravel stack and per-kind ServerKit sync are plan 26.

## Motivation

LocalKit assumes WordPress everywhere it matters: the terminal shells into
a hardcoded `wordpress` service, sync tars a hardcoded `wp-content/`, DB
ops go through wp-cli, one-click login uses a WP MU plugin, and the UI
shows WP affordances unconditionally. Yet most of the machinery — per-site
Compose projects, the shared router, terminals, logs, snapshots, tray — is
stack-agnostic in principle. A developer with a Laravel API or a stray
dockerized tool alongside their WP sites gets zero value today. Meanwhile
every plan we ship before this one adds more WP-shaped code to unwind
later. The goal: one capability system that every feature checks, with
WordPress as the polished reference implementation — not an `if` branch.

## Design

### Phase 1 — Kind + capability core (migration 7)

- `sites.kind` column: `"wordpress" | "docker"` (default `"wordpress"` —
  existing rows migrate cleanly; `"php"` arrives with plan 26). Sites also
  gain `config_json` (per-kind settings: service names, sync path, app
  port).
- Capability table in `src-tauri/src/site.rs` (const per kind, exposed via
  `app_info` and on each `Site` payload):
  `domains, terminal, logs, snapshots, db_gui, db_sync, code_sync,
  one_click_login, wp_tools, search_replace`. WordPress = all true;
  docker = `domains, terminal, logs, snapshots, code_sync`.
- De-hardcode the WP assumptions:
  - `terminal.rs` execs into `config.service` (default `wordpress`);
  - `sync.rs` code archives tar `config.sync_path` (default `wp-content/`);
  - `router.rs` upstream reads `config.app_port` (default = site port);
  - one-click login, Tools tab, WP Admin button, `lk wp` gate on
    capability in both frontends — Tauri commands return a clean
    "not supported for this site kind" error; the UI hides rather than
    errors.
- **Grep-audit gate:** checklist of every `wordpress` / `wp-content` /
  `wpcli` literal in `src-tauri/src` with a verdict (capability-gated,
  config-driven, or legitimately WP-only). `cargo check` + the full WP
  smoke example must pass unmodified before Phase 2 starts — WordPress is
  the zero-change path by construction.

### Phase 2 — Generic Docker app kind

- Creation flow: "Import a Docker project" in NewSiteDialog — pick a
  directory containing a compose file; LocalKit **copies** it into the
  managed site dir (owned, not referenced — external dirs are a
  backup/locking nightmare), asks which service is the app + its port,
  writes `.env` and the record. Copy excludes `.git`, `node_modules`,
  `vendor` via a default ignore list with an opt-out.
- Gets for free: start/stop/restart/delete, logs viewer, terminal (exec
  into the chosen service), local domain (`<slug>.test` → app port, all
  plan-16 conflict/fallback behavior included), tray actions, `lk`
  lifecycle commands.
- Snapshots (plan 17): code-only by default; if a recognized db image
  (`mysql`/`mariadb`/`postgres`) is among the services, `db_sync`
  capability flips on and DB snapshots/dumps use the engine's native dump
  tool.
- No WP tooling, no ServerKit sync (plan 26), no admin login — the value
  is "all my local projects in one place, with domains and a tray".

### Phase 3 — Frontend capability gating

- `Site` type in `src/lib/types.ts` gains `kind` + `capabilities`;
  SiteDetail renders tabs/sections from the capability list (Tools tab and
  WP Admin button hidden for `docker`), Dashboard cards get a small kind
  badge (WP / Docker), `buildCommands()` skips capability-less per-site
  commands.
- Mock mode: one fake site per kind so gated UI is reviewable in
  `npm run dev:mock`.

## Risks

- Scope creep — the guardrail: a kind ships only when every capability it
  claims works; partial kinds are worse than no kinds. WordPress
  regressions block merge, full stop.
- The de-hardcoding touches `terminal.rs`, `sync.rs`, `router.rs`,
  `wordpress.rs` — hence the Phase 1 grep-audit gate; no "we'll catch it
  later".
- Users importing huge compose projects: the ignore list covers the common
  cases; the import dialog shows the copied size before confirming.

## Verification

- WP regression: existing `smoke` / `m4_smoke` examples pass unmodified.
- New `docker_smoke` example: import a trivial two-service compose fixture
  → start → domain resolves → terminal opens in the right service →
  stop → delete.
- `cargo test --lib site`: capability matrix tests (every kind × every
  capability is an explicit, tested decision), compose-copy ignore list,
  `config_json` serde defaults.

## Phase 1 grep-audit gate (verdicts)

Every `wordpress` / `wp-content` / `wpcli` literal in `src-tauri/src`, with a
verdict. `cargo check` + the full WP smoke (`create`/`verify`/`info`/`clone`)
and `snapshot_smoke` all pass unmodified — WordPress is the zero-change path.

- **config-driven** (now read from `SiteConfig`, WP default = the old literal):
  - `terminal.rs` shell service — `config.service` (default `wordpress`).
  - `site.rs`/`snapshot.rs`/`sync.rs` archive + restore path — `config.sync_path`
    (default `wp-content`), threaded through `build/write_wp_content_tgz` and
    `restore_wp_content`.
  - `router.rs` upstream port — `config.upstream_port(site.port)`.
  - every "is the app running" `c.service == "wordpress"` check —
    `c.service == site.app_service()` (site.rs `list`, lib.rs `login`/`terminal`,
    snapshot.rs `is_running`).
- **capability-gated** (WP-only; a non-WP site gets a clean refusal, UI hides):
  `wp_cli_info`, `login`/`site_wp_users`, `lk wp`, `lk login` (via `require`),
  the router WP-URL rewrite (`search_replace`), clone / blueprint save /
  ServerKit push+pull (kind guard), and the snapshot DB dump (`db_sync`).
- **legitimately WP-only** (left as literals — these ARE WordPress by nature):
  `site.rs render_compose`/`render_env` (the generated WP compose + `.env`);
  the whole of `wordpress.rs` (wp-cli, the MU login plugin, `wp db`/
  `search-replace`); `sync.rs` `safe_entry_path`/`extract_wp_content` (the WP
  ServerKit import archive contract, plan 26 for other kinds); `blueprint.rs`
  wp-cli steps; the `snapshot.rs` `wp cache flush` (gated on `wp_tools`).
