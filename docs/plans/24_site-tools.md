# 24 — Site tools: database GUI, search-replace, debug mode, config editor

Status: ✅ shipped

All four phases landed. Notes where the implementation reconciled the plan
against real behaviour:

- **Adminer login** uses the site's `wordpress` DB user, not `root`: the compose
  template sets `MYSQL_RANDOM_ROOT_PASSWORD`, so root's password is unknowable.
  The "Open database" button opens `?server=db&username=wordpress&db=wordpress`
  and copies the `wordpress` user's password to the clipboard.
- **Adminer port** is `db_port + 1000` (`Site::adminer_port`), mapped in the
  deterministic compose template; `open_site_database` rewrites the compose file
  first so sites created before the feature pick up the service.
- **`db-<slug>.test`** is carried in `render_caddyfile` (+ matching hosts
  entries) for `db_gui` sites; the button opens the domain when local domains are
  on, else `localhost:<adminer_port>`.
- **Search-replace** parses wp-cli's *tab-separated* report (it drops the ASCII
  grid when stdout is a pipe, which is what LocalKit captures).
- **Debug** writes `wp-config.php` via a root wpcli runner (`--user root` +
  `--allow-root`) — the file is root-owned in the wp-data volume.
- **Config editor** reads/writes `wp-config.php` with `docker compose cp`
  (runs as the daemon, so it overwrites the root-owned file; requires the site
  running); `.env` is a plain host file whose save offers a restart
  (`compose up -d`, which recreates services whose env changed).

A "Tools" tab on SiteDetail covering the four things every WP developer
reaches for an external app to do today: browse the database, run a
search-replace, toggle WP_DEBUG and read the debug log, and edit
`wp-config.php` / `.env` without leaving the app.

## Motivation

LocalKit covers the site lifecycle well, but the *inner loop* of WordPress
development still pushes users elsewhere: they install TablePlus/phpMyAdmin
for the database, open a terminal for `wp search-replace` (or worse, run a
serialization-unsafe SQL replace by hand), edit `wp-config.php` in an
editor to turn on debugging, and tail `debug.log` in another window. Each
is a small, well-understood feature that the existing infrastructure
(profile-gated compose services, the wpcli runner, the router, the file
system) already supports. Together they make SiteDetail the single place
the daily work happens.

## Design

### Phase 1 — Database GUI (Adminer sidecar)

- Adminer (single-file PHP, ~0.5 MB — not phpMyAdmin's 50 MB image) as a
  profile-gated `adminer` service in the site compose template
  (`adminer:4-standalone`), off by default, toggled from Tools → Database.
  Gated on the `db_gui` capability (plan 22), so non-WP kinds with a
  database get it too. Port: `db_port + 1000` mapped at create time (deterministic, no
  allocator changes), plus a router host `db-<slug>.test` when domains are
  enabled — `render_caddyfile` gains one conditional block; when the router
  is in fallback-port mode (plan 16) the same port-awareness applies.
- "Open database" button starts the profile service on first use
  (`docker compose --profile tools up -d adminer`), then opens the URL with
  `?server=db&username=root` prefilled (password copied to clipboard with a
  toast — Adminer can't take it in the URL).
- Mock mode: the button opens a fake disabled state with the same copy.

### Phase 2 — Search-replace (`src-tauri/src/wordpress.rs`)

- `search_replace(site_id, from, to, dry_run) -> SearchReplaceResult`
  wrapping `wp search-replace <from> <to> --all-tables --precise
  --report-changed-only [--dry-run]` — the serialization-safe path, never
  raw SQL.
- UI: Tools → Search & Replace: from/to fields, always runs dry-run first
  and shows the per-table change counts, then an explicit Apply. Result
  notes the pre-replace snapshot (plan 17 auto-snapshot, kind
  `pre_search_replace`) with a restore shortcut.
- `lk wp` already covers the CLI case — no new subcommand needed.

### Phase 3 — Debug mode + log viewer

- `set_debug(site_id, enabled)`: `wp config set WP_DEBUG <bool> --raw` +
  `WP_DEBUG_LOG` + `WP_DEBUG_DISPLAY false` (log to file, never to screen)
  via the wpcli runner; status read via `wp config get`.
- Tools → Debug: toggle + an auto-refreshing tail of
  `wp-content/debug.log` rendered in the same mono/log styling as the
  container logs viewer (plain fs read — the file is bind-mounted), with a
  "clear log" button.

### Phase 4 — Config file editor

- Tools → Config: a lightweight editor (existing JetBrains Mono textarea
  styling, no Monaco dependency) for `wp-config.php` and the site `.env`,
  with save → offers to restart the site when `.env` changed (required for
  compose to pick it up; `wp-config` needs nothing). Danger styling and a
  one-line "editing this can break the site" note; no diff/backup machinery
  — snapshots (plan 17) are the safety net.

## Risks

- Adminer on the router adds an attack surface on a dev machine: bound to
  localhost anyway (same trust level as the sites), and off by default.
- Search-replace on big databases can take minutes — run via the wpcli
  runner with the standard site-event progress; the dry-run-first flow
  means the user sees cost before committing.
- The config editor must not fight the compose/env templates: `.env` keys
  LocalKit manages (ports, passwords) get inline "managed by LocalKit"
  markers in the template so the editor can warn on those lines only.

## Verification

- Manual: enable Adminer → log in → browse; dry-run a replace → apply →
  assert serialized widget survives; toggle debug → fatal in a must-use
  test plugin appears in the log viewer; edit `.env` port → prompted
  restart → site answers on the new port.
- Mock mode renders all four tool sections with fake data.
