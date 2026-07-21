# AGENTS.md — LocalKit

## What this is

Desktop app (Tauri 2) that manages local WordPress sites via per-site Docker
Compose projects. Since plan 22 it also manages generic bring-your-own-compose
**Docker projects** via a capability-gated `kind` model (WordPress is the
reference kind). v1 = milestones M1–M4 (local sites + ServerKit push/pull).
Push/pull talks to the `serverkit-localkit` extension on the server
(`/api/v1/localkit`, in the ServerKit repo).

## Project structure

```
src/                     React 18 + TS + Vite frontend
  lib/ipc.ts             typed wrappers for all Tauri commands (invoke + events)
  lib/types.ts           shared TS types mirroring Rust payloads
  lib/terminalRegistry.ts  xterm.js instances living outside React (one PTY per
                         site; scrollback survives page switches; attach/detach)
  lib/termSuggest.ts       ghost-text history suggestions (plan 14): per-site MRU
                         in localStorage, →/End accepts, frontend-only
  lib/commands.tsx         single command registry (plan 15): static + per-site
                         commands feeding the palette, shortcuts, cheat-sheet
  lib/shortcuts.ts         keyCombo canonicalizer (mod = Ctrl/Cmd) + comboLabel
  lib/keybindings.ts       shortcut override resolver over the settings KV
                         (shortcut.<id>; absent = default, "none" = unbound)
  lib/fuzzy.ts             fuzzy scorer for the command palette
  hooks/                   useShortcuts.ts (global keydown dispatcher with
                         editable-target guard), useDialog.ts (shared modal
                         Escape/outside-click, topmost-only stack)
  stores/                Zustand stores (nav.ts = page state + settings modal +
                         palette/new-site/cheat-sheet dialog flags,
                         settings.ts = unified prefs over the app_settings KV —
                         seeded pre-paint from window.__LOCALKIT_SETTINGS__,
                         sites.ts = data/actions, blueprints.ts = plan-20
                         template data/actions, toast.ts = global toasts +
                         module-level toast.* helpers)
  pages/                 Dashboard (grid/list site views), SiteDetail,
                         Terminal (one tab per site, shell in the wordpress
                         container), Settings (modal, opened via sidebar gear —
                         not a page)
  components/            Sidebar, StatusBadge, CopyButton, NewSiteDialog,
                         CommandPalette, KeyboardSettings,
                         KeyboardShortcutsDialog, SnapshotsPanel,
                         DeleteSiteDialog, ImportSiteDialog, CloneSiteDialog,
                         SaveBlueprintDialog (plan 20),
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
  src/docker.rs          `docker compose` CLI wrapper (check/up/down/run/ps/logs
                         + config-json inspect for plan-22 docker apps)
  src/site.rs            Site model + lifecycle + compose/env templates; the
                         plan-22 kind/capability model (SiteConfig, Capabilities)
  src/dockerapp.rs       plan-22 generic Docker app kind: inspect a compose
                         project (services/ports/DB engine) + import (copy the
                         folder, record app service/port, bring it up)
  src/wordpress.rs       wp-cli via `docker compose run --rm wpcli wp ...`
  src/router.rs          M6 local domains: shared Caddy router (`*.test`),
                         hosts-file block + elevated writer, CA trust, status
  src/tray.rs            M8 system tray: close-to-tray, tray menu with quick
                         site Open/Start/Stop, single-instance focus
  src/terminal.rs        per-site interactive terminals: `portable-pty` PTY
                         running `docker compose exec wordpress bash`; events
                         `terminal://data` / `terminal://exit`
  src/serverkit.rs       ServerKit API client (X-API-Key) + connection model
  src/sync.rs            push/pull orchestration + SyncRecord (sync_history) +
                         plan 18 import (clone a remote site to a new local one)
  src/transfer.rs        plan 19 chunked transfers: chunk planning + resume
                         subtraction, sha256 hashing writer, self-deleting
                         staged/temp files, per-site cancel registry
  src/snapshot.rs        plan 17 snapshots: DB dump + wp-content archive on
                         disk (no DB table), restore, retention; also
                         restore_archives_into (seed a fresh site from a
                         snapshot's archives — the shared core of clone/blueprint)
  src/blueprint.rs       plan 20 blueprints: save a site as a reusable template
                         (<data>/blueprints/<slug>/ = blueprint.json + db.sql.gz
                         + wp-content.tar.gz, plugin/theme recipe), create new
                         sites from one, and export/import a portable .lkbp.
                         Clone itself lives in site.rs (clone_site)
  tauri.conf.json        v2 schema; capabilities/default.json grants opener plugin;
                         the main window is built in code (`lib.rs run()`), not
                         from the config `windows` array, so the settings init
                         script can attach pre-paint
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
- `node scripts/verify-shortcuts.mjs` — headless runtime check of the plan-15
  keyboard system against the mock server (palette, shortcuts, editable
  guard, rebinding/conflicts/persistence)
- `node scripts/verify-router-conflict.mjs` — headless runtime check of the
  plan-16 port-conflict UX against the mock server (named conflict, fallback
  one-click recovery, port-bearing site URLs, SiteDetail banner). The mock
  fakes a LocalWP holding 80/443; `window.__LOCALKIT_MOCK__` (mock builds
  only) lets the script reach states the UI can't drive on its own.
- `cd src-tauri && cargo run --example smoke -- <create|verify|info|stop|start|reconcile|recover|clone|blueprint|tools|config|adminer|delete|cleanup>`
  — end-to-end lifecycle smoke test against real Docker (no Tauri runtime needed);
  uses a scratch data dir under the OS temp dir. `reconcile` (plan 23) stops the
  containers behind LocalKit's back and asserts the reconciler settles
  running→stopped, then stopped→running, and that the grace window shields a
  fresh command write. `recover` (plan 23) removes the completion marker + forces
  `creating` to simulate a killed create, asserts the site reads `incomplete`,
  then resumes it back to running/complete. `clone` and `blueprint` (plan 20)
  create a marker post on the smoke site, then assert a one-click clone / a
  save-then-create-from-blueprint carries the content across with fresh
  ports/secrets; both clean up after themselves. `tools` (plan 24) exercises the
  site-tools backend against the smoke site: a search-replace dry-run finds the
  baked-in home/siteurl without writing, Apply (with a `pre_search_replace`
  snapshot) rewrites them and the URL is restored afterward, and the WP_DEBUG
  toggle round-trips through wp-config.php (via the root-capable wpcli runner).
  `config` (plan 24, split out so it stays fast) reads wp-config.php out of the
  running container via `compose cp`, round-trips a write without breaking the
  site, and reads/writes the `.env`. `adminer` (plan 24) rewrites the compose
  file to add the profile-gated `adminer` service, starts it on demand, and
  asserts it serves its login page on db_port + 1000.
- `cd src-tauri && cargo run --example docker_smoke [-- run|clean]` — plan-22
  E2E for the generic Docker app kind against real Docker (scratch data dir):
  writes a trivial nginx+mariadb compose fixture, inspects it, imports it as a
  `docker` site (asserting `.git` is excluded and `.env` gets a
  COMPOSE_PROJECT_NAME), checks the app answers HTTP on its published port, the
  chosen service is exec-able (the terminal target), a code-only snapshot
  (db_bytes 0), then stop/start/delete.
- `node scripts/verify-snapshots.mjs` — headless runtime check of the plan-17
  snapshot UX against the mock server (listing + kind badges, take-with-note,
  restore taking a pre-restore snapshot first, delete, a DB pull leaving a
  `pre_pull` snapshot, and both delete-site dialog paths).
- `cd src-tauri && cargo run --example snapshot_smoke [-- run|clean]` — plan-17
  E2E against real Docker: snapshots the `smoke` site, deletes post 1 and a
  canary file in wp-content, restores, asserts both are back. Run
  `smoke -- create` first.
- `cd src-tauri && cargo run --example m4_smoke` — M4 push/pull E2E against a
  mock serverkit-localkit extension (`node examples/mock_localkit_ext.cjs`
  first, port 9872); requires the smoke site to exist. Since plan 18 it also
  imports a remote site as a real new local site (containers and all) and
  deletes it again, and asserts the multisite refusal provisions nothing.
  Since plan 19 it also drives the chunked path: it writes a 110 MB
  incompressible filler into the smoke site's wp-content, has the mock refuse
  chunks after two land, and asserts the retry re-sends only the missing ones,
  that the same >100 MB archive is refused over v1, and that v1 still works
  when `/pair` withholds `sync-v2`. Since plan 21 the mock also serves the
  ServerKit core probes (public `GET /api/v1/system/health` →
  `service: serverkit-api`, and the key-gated `GET /api/v1/setup-health/account`)
  so `lk connection add`/`test` and `lk doctor` can validate against it.
- `node scripts/verify-sync-progress.mjs` — headless runtime check of the
  plan-19 transfer UX against the mock server (the byte readout advancing
  monotonically against a fixed total, the Cancel button appearing only while
  bytes move, a cancel resolving neutrally and really stopping the transfer,
  `cancelled` in sync history, and an uninterrupted transfer still finishing
  green). Its click helper reports disabled buttons instead of silently
  no-opping — that is what catches "a terminal stage nobody handled left the
  buttons stuck".
- `node scripts/verify-import.mjs` — headless runtime check of the plan-18
  import UX against the mock server (per-row Import buttons, the multisite
  refusal and its tooltip, the version-match readout and mismatch warning,
  the progress stages, the dashboard origin badge, the duplicate refusal).
- `node scripts/verify-blueprints.mjs` — headless runtime check of the plan-20
  clone + blueprint flows against the mock server (the New Site "From blueprint"
  section with plugin/theme chips, selecting one to create-from, a one-click
  Clone under a new name, and Save-as-blueprint round-tripping into the dialog).
- `node scripts/verify-site-tools.mjs` — headless runtime check of the plan-24
  Tools tab against the mock server (a WordPress site's Tools tab switching from
  the overview; the Database GUI's "Open database" toasting the login; Search &
  Replace previewing per-column change counts then applying with the snapshot
  shortcut; the Debug toggle seeding the log viewer and Clear emptying it; the
  Config editor loading wp-config.php and switching to the .env; a code-only
  docker site having no Tools tab).
- `node scripts/verify-multistack.mjs` — headless runtime check of the plan-22
  capability gating against the mock server (the WP/Docker kind badges, a docker
  site's SiteDetail hiding WP Admin / credentials / database / wp-cli / clone /
  blueprint / push while keeping snapshots+logs+terminal, the WP detail
  unchanged, and the New Site "Docker project" tab's inspect→import flow).
- `cd src-tauri && cargo test --lib router` — unit tests for the M6 hosts-file
  block logic (insert/replace/remove idempotency, CRLF preservation) plus the
  plan-16 port probe, listener-table parsing, compose port mapping and
  `site_url` formatting.
- `cd src-tauri && cargo test --lib snapshot` — plan-17 retention rules
  (per-kind cap, manual never pruned) + manifest/gzip round-trips.
- `cd src-tauri && cargo run --example m6_smoke` — M6 router E2E against the
  smoke site; **interactive only** (hosts-file writes trigger UAC/elevation
  prompts twice). Run `smoke -- create` first, `smoke -- cleanup` after.
- `cd src-tauri && cargo run -p lk -- <cmd>` — headless CLI (`lk list |
  create [--blueprint <id|name>] | clone <site> <new-name> | start | stop |
  restart | resume | delete | info | logs | wp | env | login |
  snapshot list|create|restore|delete |
  blueprint list|save|delete|export|import | import |
  connection add|list|test|remove | sites --remote <conn> |
  push <site> --code|--db | pull <site> --db |
  completions <bash|zsh|fish|powershell> | doctor`); shares the GUI's
  data dir, so use `--data-dir` (or
  `LOCALKIT_DATA_DIR`) for throwaway tests. See docs/plans/7_cli.md and
  docs/plans/21_cli-serverkit.md (the ServerKit surface).
- `node scripts/verify-cli-serverkit.mjs` — headless runtime check of the
  plan-21 `lk` ServerKit surface against `examples/mock_localkit_ext.cjs`:
  it shells out to the compiled `lk` binary (build it first with
  `cargo build -p lk`) with a throwaway data dir and asserts exit codes and
  `--json` shapes for connection add/list/test/remove, `sites --remote`, the
  bad-key refusal, the push/pull argument errors, and completions for all four
  shells. The Docker-backed push/pull path is covered by `m4_smoke`.
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
  `if version < N` block; never edit migration 1. (Latest is migration 7:
  plan-23 `status_updated_at` — every command status write stamps it, and
  `settle_status` compare-and-swaps on it so the reconciler never clobbers a
  newer write. Migration 6 was plan-22 `kind` + `config_json`.)
- **Async:** never hold the `Db` mutex guard across `.await` (futures must be Send);
  lock in a short scope, drop, then await.
- **Ports:** site port = first free from 8081; DB host port = site port + 10000.
- **Versions:** WP/PHP versions come from allowlists in `site.rs`
  (`WP_VERSIONS`, `PHP_VERSIONS`) — the UI reads them via the `app_info` command.
- **Site kinds & capabilities (plan 22):** every site has a `kind`
  (`wordpress` | `docker`; `php` arrives with plan 26) and a `SiteConfig`
  (`config_json`, migration 6) carrying the de-hardcoded WordPress assumptions:
  `service` (terminal/logs), `sync_path` (code archive), `app_port` (router
  upstream), and a detected `db_engine`/`db_service`. `Site::capabilities` is
  **derived** from kind+config on every read (never stored), and every feature
  gates on it instead of assuming WordPress: WP claims all of
  `domains, terminal, logs, snapshots, db_gui, db_sync, code_sync,
  one_click_login, wp_tools, search_replace`; docker claims
  `domains, terminal, logs, snapshots, code_sync`. WordPress is the zero-change
  path — the config defaults ARE the old literals. Backend commands refuse a
  missing capability via `Site::require(cap, action)` ("… not supported for
  <kind> sites"); both frontends **hide** rather than error (gate on
  `site.capabilities.*` / `app_info.kinds`). A new kind ships only when every
  capability it claims works — docker is code-only until engine-native DB dumps
  land (so `db_sync` is off), and clone/blueprints/ServerKit sync stay
  WordPress-only (plan 26). Docker apps are **copied** into the managed site dir
  (`dockerapp.rs`), never referenced.
- **Status reconciliation (plan 23):** site status is otherwise write-path only,
  so `reconcile.rs` settles the DB against Docker's ground truth — **inspect,
  settle forward, never guess.** `spawn_loop` (started in `lib.rs run()`) runs
  `reconcile_once` at startup and every 60 s; each pass does one
  `docker::project_container_states` call (a single `docker ps`, grouped by the
  `com.docker.compose.project=localkit-<slug>` label — never N per-site
  `compose_ps`) and applies the pure, unit-tested `classify` × `decide` table.
  **Forward-only:** it settles via `db.settle_status`, a compare-and-swap on
  `status_updated_at`, so a newer command/event write always wins; a 60 s grace
  window shields a just-started site from a running→stopped downgrade. Sites with
  an in-flight lifecycle command are skipped via `AppState.in_flight`
  (`reconcile::InFlight`, an RAII refcounted guard held by every lifecycle path —
  start/stop/delete/create/clone/import/blueprint/restore). **No ground truth
  (Docker down) → zero settles**, so a Docker Desktop restart never mass-flaps
  sites. `degraded` (amber, up-but-unhealthy) is a real status the reconciler and
  `site::list` both produce — touchpoints: `StatusBadge`, dashboard/SiteDetail/
  palette (treated as "up" for Open/Stop), tray dot ◐, `lk list`. After a settle:
  `tray::refresh` + a `sites-changed` event the frontend re-fetches on. **Docker
  health:** `docker::check_cached` (30 s TTL) backs the sidebar's global "Docker
  unavailable" pill (the `useDocker` store polls it). **Crash recovery:** each
  successful create/import/clone/blueprint writes a `.localkit-install-complete`
  marker as its last step (`site::mark_complete`); a startup backfill marks
  known-complete sites so legacy rows aren't flagged. A dir without the marker
  (and not in flight) reports `incomplete` on `SiteWithStatus`/`SiteDetail`; the
  dashboard shows "Setup incomplete" + Resume / Clean up, `site::resume` re-runs
  the create tail, `lk resume` / `lk list` mirror it.
- **wp-cli:** the stock `wordpress` image has no wp-cli; use the profile-gated
  `wpcli` service (`wordpress:cli-php<ver>`) via `docker::compose_run`, and
  always pass `wp` as the first argument (the cli image's `wp` CMD is replaced
  by run args, so omitting it makes the entrypoint exec `core` and fail).
- **Site tools (plan 24):** the Tools tab on `SiteDetail` (`SiteTools.tsx`,
  shown only when the kind claims a tool) hosts four capability-gated panels.
  **Database** (`db_gui`): a profile-gated `adminer` service in the compose
  template on `db_port + 1000` (`Site::adminer_port`), started on demand
  (`docker::compose_up_profile_service`, `--profile tools up -d adminer`) —
  `open_site_database` rewrites the deterministic compose file first so sites
  created before the feature get it, opens Adminer prefilled with the
  `wordpress` DB user (root's password is random/unknowable), and the frontend
  copies the password to the clipboard. `render_caddyfile` carries a
  `db-<slug>.test` route for `db_gui` sites, with matching `db-<slug>` hosts
  entries (`site_slugs`). **Search & Replace** (`search_replace`):
  `wordpress::search_replace_report` runs the serialization-safe
  `wp search-replace --all-tables --precise --report-changed-only [--dry-run]`
  and parses the report (tab-separated in practice, not the ASCII grid — wp-cli
  drops the grid when stdout is a pipe); Apply takes a `pre_search_replace`
  snapshot first. **Debug** (`wp_tools`): `WP_DEBUG`/`WP_DEBUG_LOG` toggle (log
  to file, never screen) + a tail of the bind-mounted `wp-content/debug.log`.
  **Config** (`wp_tools`): edits `.env` (a plain host file; save offers
  `site::restart` = `compose up -d`, which recreates services whose `.env`
  changed) and `wp-config.php` (in the wp-data volume — read/written with
  `docker compose cp` against the running container, which runs as the daemon so
  it can overwrite the root-owned file; the command gates it on the site
  running). Writing `wp-config.php` via a piped `sh -c 'cat > …'` was tried and
  abandoned — docker's stdin EOF didn't reach `cat`, hanging the run. Any wp-cli
  that must write `wp-config.php` (e.g. `wp config set` for debug) runs through
  `docker::compose_run_root` + `--allow-root` (`wordpress::wp_root`), since the
  cli image's `www-data` user can't write the root-owned file.
- **Events:** long operations emit `site-event`
  (`{id, stage, message, bytes_done?, bytes_total?}`);
  create stages: files → pulling → containers → waiting → install (re-emitted
  per attempt) → done | error. The `pulling` stage pre-pulls all images
  including the profile-gated wpcli (`docker::compose_pull`) so first-run
  downloads are a labeled stage, not a silent stall. When there is no Tauri
  app handle (CLI, examples), `site::emit` prints `[stage] message` to stderr
  instead of dropping the event. On the frontend, `sites.ts handleEvent`
  renders these as a single pinned progress toast that resolves on any
  terminal stage (`done` | `error` | `cancelled` — see `isTerminalStage`).
  The byte fields are present only during a chunked transfer (`emit_bytes`):
  the backend sends raw counters and a bare label, and the frontend composes
  "Pushing wp-content — 148 MB / 312 MB". Never format the readout backend-side.
- **Settings store (plan 13):** all frontend preferences flow through
  `stores/settings.ts` over the `app_settings` KV — reads seed synchronously
  from `window.__LOCALKIT_SETTINGS__` (published by
  `build_settings_init_script` in `lib.rs` via `WebviewWindowBuilder::
  initialization_script`, before first paint — that's why the main window is
  built in code), with an async `settings_get_all` fallback for mock mode.
  Writes are optimistic + fire-and-forget `set_app_setting` and mirrored to
  localStorage (`localkit.settings.*`) so pure-web mock/dev keeps prefs.
  New prefs need zero new Tauri commands — add a typed accessor in
  `settings.ts`; parsing (`"true"` → bool, numbers) lives there. The old
  `localkit.siteView` localStorage key is one-time migrated on store creation.
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
- **CLI ServerKit (plan 21):** connections resolve by exact id or
  case-insensitive label, same shape as sites (`pick_connection` sits next to
  `pick`). `connection add` validates (health + key + `/pair`) *before*
  storing and refuses a key that doesn't work; the key comes from a hidden
  `rpassword` prompt, `--key`, or `LOCALKIT_API_KEY` (never prompts on a
  non-TTY). `connection list` is local-only (no network — `test` does the live
  probe) and its `--json` uses a redacted `ConnectionView` so the plaintext
  API key never reaches stdout. `push`/`pull` default their target to the
  site's linked remote (plan-18 migration-5 `connection_id`/`remote_site_id`);
  `--connection`/`--remote-site` override, and are required when the site has
  no link. Push/pull exit **2** when the *server* rejects the operation
  (`remote_rejected` heuristic over the library's error strings) vs 1 for local
  failures, and `--json` prints the resulting `SyncRecord` (read back from
  history). `doctor` runs the same connection probe per stored connection but
  keeps it informational — a down remote is not a local misconfig, so it never
  flips the exit code. `completions` is `clap_complete`.
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
- **Router coexistence (plan 16):** host ports are configurable via the
  `app_settings` keys `router_http_port` / `router_https_port` (default
  80/443; `router::router_ports`). Container ports stay 80/443 — only the
  host mapping moves — so the Caddyfile and the hosts block stay port-blind.
  `site_url` is therefore **port-aware**: default ports give the clean
  `https://<slug>.test`, any other pair gives `http://<slug>.test:<port>`
  and deliberately stays on http (a non-standard https port re-prompts for a
  cert exception even with the CA trusted). **`router::site_public_url` is
  the single source of truth** for "where does this site live" — tray menu,
  WP install URL, one-click login and sync's `local_url` all funnel through
  it; never hand-roll the domain-vs-localhost rule again. Frontend mirror:
  `lib/domains.ts` (`siteUrl`, `isDefaultPorts`).
  Before enabling (and on every `status()` where `enabled && !running`),
  `probe_ports` checks who owns the ports. **Probing must consult the OS
  listener table** (`Get-NetTCPConnection` / `lsof`), not just a trial bind:
  on Windows a socket bound with SO_REUSEADDR (Docker's port publisher does
  this) lets you re-bind the same wildcard address, so bind-only probing
  reports a busy port as free. Conflicts surface as `RouterStatus.conflicts`
  and drive the Settings callout, the SiteDetail banner and `lk doctor`.
  Note a failed *enable* leaves `domains_enabled` off (the backend
  short-circuits before setting it), so UI must not gate conflict reporting
  on the enabled flag.
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
- **Terminal quick wins (plan 14):** the registry also loads
  `@xterm/addon-web-links` (Ctrl-click URLs open via the opener plugin) and
  copy-on-select (hardcoded on; empty selections never clobber the
  clipboard). Ghost-text suggestions live in `lib/termSuggest.ts` — MRU
  history per site in localStorage (`localkit.termHistory.<siteId>`), →/End
  accepts, frontend-only (mock needs nothing). Two xterm-6 gotchas: the
  XTerm options need `allowProposedApi: true` (decorations are still
  proposed API), and the echo-check marker must be pinned when the input
  line STARTS — v6 delivers input asynchronously, so at Enter the shell's
  echo has often already moved the cursor off the command row. Settings →
  Terminal exposes `terminalFontSize` (11–16, live-applied via a
  `useSettings.subscribe` in the registry) and `terminalScrollback`
  (1k/5k/10k, read at terminal creation only — noted in the UI).
- **Command palette + shortcuts (plan 15):** `lib/commands.tsx` is the single
  registry (`buildCommands()` reads stores via getState so the dispatcher
  always sees fresh state; `useCommands()` is the reactive wrapper) — static
  commands + per-site Open/Start|Stop/WP Admin/Terminal rebuilt from the
  sites store. `hooks/useShortcuts.ts` is the one global keydown listener;
  the editable-target guard means shortcuts never fire in
  inputs/selects/contenteditable/xterm unless the combo has `mod`.
  Combos are canonical (`lib/shortcuts.ts`: `mod` = Ctrl/Cmd, shifted
  punctuation like `?` carries its own shift). Remappable bindings
  (phase 3) live in the app_settings KV as `shortcut.<commandId>` —
  absent = default, `"none"` = explicitly unbound — resolved by the one
  `effectiveCombo()` in `lib/keybindings.ts` shared by dispatcher, palette,
  cheat-sheet and Settings → Keyboard; resets go through the
  `delete_app_setting` command (settings store `remove()`), not a sentinel.
  Dialog flags (`paletteOpen`, `newSiteOpen`, `cheatsheetOpen`) live in
  nav.ts and the dialogs render globally in App.tsx, so commands work from
  any page. Modals share `hooks/useDialog.ts` (Escape closes the topmost
  dialog only, via a module-level stack; outside-click; focus stays
  declarative via `autoFocus`) — never hand-roll per-modal Escape
  listeners.
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
  accepted for v1, revisit with a keyring later. `sync::emit` delegates to
  `site::emit`, so sync progress prints to stderr in the CLI/examples instead
  of vanishing. Bulk transfers use `serverkit::transfer_client` (30 min), not
  the 15 s probe client — reqwest's `timeout` is a *total* request budget, so
  the short one aborts any payload bigger than a fast link can move in 15 s.
  Since plan 19 this v1 path only runs against servers without `sync-v2`.
- **Sync v2 — chunked transfers (plan 19):** `transfer.rs` holds the
  substrate (no HTTP in it, so the offset math is unit-testable);
  `serverkit::push_chunked` / `download_resumable` are the wire protocol;
  `sync.rs` picks between them and v1 **once, at the top** of each operation
  via `supports_v2` (`GET /pair` → `sync-v2`). **Keep v1 as one isolated
  function per operation** — never sprinkle `if v2` through a shared flow; a
  failed capability probe must fall back to v1, because falling back always
  works. Uploads: `CHUNK_SIZE` is 8 MiB and is a **const, not a setting**;
  each chunk is one request, so reqwest's total-request `timeout` *is* the
  per-chunk timeout (the operation is bounded by liveness, not duration).
  Resume is nothing but `transfer::remaining` subtracting the offsets `init`
  reports — the client persists no state, and offsets it never planned are
  ignored rather than trusted. The server processes only inside `finish`,
  **after** the whole-file sha256 verifies, which is why an abandoned transfer
  can never half-apply; the safe-extract policy still applies to the verified
  archive (verified means intact, not friendly). Downloads use HTTP `Range` +
  `If-Range` with a client-generated `?session=` that pins one materialized
  export server-side — `pull/db` and `pull/code` build their payload per
  request, so ranges from two different `mysqldump`/`tar` runs would splice
  into garbage; a `200` answering a ranged request means "start over".
  **Nothing large is held in memory anymore**: `snapshot::write_wp_content_tgz`
  tars into a staging file, `docker::compose_run_reader` streams a dump
  decompress→pipe→`wp db import`, and the import untars straight off disk.
  Cancel: `AppState.transfers` (`transfer::CancelRegistry`) hands each op a
  token checked between chunks; `cancel_sync(site_id)` sets it. **A cancel is
  not a failure** — it emits the `cancelled` stage and records
  `status: "cancelled"`. Frontend listeners must use `isTerminalStage` from
  `stores/sites.ts` rather than hardcoding `done | error`, or they silently
  stop resetting when a stage is added.
- **Import (plan 18):** `sync::import_site` clones a remote site into a *new*
  local site. Order is the design: `pre_import` checks everything knowable
  before provisioning (extension advertises `pull-code`, remote exists, not
  multisite, not already imported from that same remote via the migration-5
  `connection_id`/`remote_site_id` columns), so a predictable failure leaves
  no half-built site; after `site::reserve` any failure runs `site::cleanup`.
  **`wp core install` is never run** — the imported database IS the site, and
  `admin_user` is read back from its first administrator (no password stored;
  one-click login does not need one). `extract_wp_content` treats the archive
  as hostile: plain files/dirs under `wp-content/` only — absolute paths,
  `..`, symlinks and hardlinks are refused, never sanitized. Version drift is
  a warning, not an error (`match_version` drops the remote patch level and
  matches `major.minor` against the allowlist, falling back to newest).
  Permalinks are flushed after import or every imported page 404s. Optional
  post-import wp-cli steps are wrapped in `optional()` (2 min timeout): they
  run after the data has landed, so hanging on one would discard a completed
  import.
- **Extension capabilities:** `GET /pair` returns a `features` array; probe it
  with `serverkit::has_feature`. **Absent means unsupported, not unknown** —
  gate the UI on it rather than discovering a 404 mid-operation. Add new
  server capabilities to `FEATURES` in the extension's `localkit.py` (append
  only; never rename an entry, clients match the literal string).
- **`site::create` is split** into `reserve` (validate + unique slug + free
  ports + insert the `creating` row) and `write_project_files`, so the import
  flow allocates through the same race-free path instead of a parallel copy.
  `wordpress::wait_for_config` exists because `site::wait_for_port` is not a
  readiness signal: Docker publishes the host port when the container is
  *created*, so wp-cli can race the image entrypoint still writing
  wp-config.php. Anything shelling into wp-cli right after `compose up` must
  wait on it (`site::create` only survives because its install step retries).
- **Snapshots (plan 17):** `snapshot.rs`. A snapshot is a *directory*, not a
  DB row — no migration: `<data dir>/snapshots/<site_id>/<ts>/` holding
  `manifest.json` + `db.sql.gz` + `wp-content.tar.gz`. The manifest carries
  `site_name`/`site_slug` so it stays meaningful after the site row is gone.
  Payloads are written **before** the manifest, so a half-written snapshot has
  no manifest and `list` skips it instead of offering a broken restore.
  `build_wp_content_tgz` lives here and is what `sync::push_code` uploads —
  one archive format, so snapshots untar by hand. `create` emits only
  `snapshot`-stage events, never `done`/`error`, because it nests inside
  push/pull/delete whose progress toast must not resolve early; standalone
  callers (the Tauri command) emit the terminal stage themselves. Restore
  swaps wp-content's *contents*, never the directory (it is bind-mounted —
  removing it breaks the mount), and auto-starts a stopped site for the DB
  import. **Every destructive flow snapshots first:** `push_db`/`pull_db`
  abort if it fails (never mutate without a net), `site::delete` takes a
  `pre_delete` one best-effort (a broken site must stay deletable) and keeps
  the snapshot dir unless the caller passes `delete_snapshots`. Retention is
  the pure, unit-tested `prunable()`: newest 5 per site per auto kind,
  `manual` never pruned. Two transient kinds (`clone_source`,
  `blueprint_source`) back the plan-20 flows and are hidden from the
  user-facing `list()` (retention still caps orphans via `list_all`).
- **Clone + blueprints (plan 20):** both build on the snapshot engine and share
  `snapshot::restore_archives_into` (lay a `(db.sql.gz, wp-content.tar.gz)` pair
  onto a fresh site, then rewrite the source URL to the clone's). `site::
  clone_site` snapshots the live source, provisions a target (fresh slug/ports,
  fresh DB password + WP salts — **secrets are never copied**; `admin_user`/
  `admin_pass` DO carry over because the copied DB holds them), seeds it, and
  deletes the transient snapshot. `blueprint.rs` is the same shape but the source
  is a saved recipe under `<data>/blueprints/<slug>/`: `save` hardlinks the
  snapshot's artifacts across (`hardlink_or_copy`, copy fallback) so bytes aren't
  duplicated, captures the plugin/theme list as **display-only** metadata, and
  drops the snapshot; `create_site` reserves a site (versions matched to the
  current allowlist via the shared `sync::match_version`), lays the archives
  down, and rewrites the URL read back out of the imported DB — **no `wp core
  install`**, the recorded database IS the site, `admin_user` comes from its
  first administrator with no stored password (mirrors import). A blueprint is
  portable as a single `.lkbp` (tar.gz of the three artifacts); `import` is
  safe-extract — only the three known filenames, each via `io::copy` so a
  crafted link/path can't escape the staging dir. Both flows emit the standard
  create/import `site-event` stages and call `router::refresh_routes` +
  `refresh_hosts` — clones and blueprint-sites are ordinary sites.
- **Port allocation:** `site::free_port` checks the OS listener table
  (`router::listening_ports`), not just a trial bind, and checks the DB port
  (site port + 10000) as well as the site port. Bind-only probing is the
  plan-16 SO_REUSEADDR trap: a port published by a running container reads as
  free, and creation then dies at `compose up` *after* the image pull.
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
