# 26 — PHP/Laravel stack + per-kind ServerKit sync parity

Status: ⬜ planned

The second multi-stack kind: a generated **PHP/Laravel** site template with
database sync that doesn't depend on wp-cli — plus the ServerKit extension
changes that make push/pull/import work per site kind. Depends on plan 22
(kind/capability core), plan 17 (snapshots), plan 18 (import flow), and
plan 19 (sync v2).

## Motivation

Plan 22 makes LocalKit stack-aware and covers ad-hoc Docker projects, but
the most common non-WP case on the server side deserves a first-class
template: plain PHP/Laravel apps. Today syncing one means hand-running
`mysqldump` and rsync. With the capability core in place, a `php` kind is
an additive increment: a compose template, engine-native DB sync, and
per-kind dispatch on both ends of the sync protocol — no new architecture.
Node/Python kinds remain deliberately out of scope; the capability model
makes them a follow-up plan of the same shape when there's demand.

## Design

### Phase 1 — PHP/Laravel stack template (`src-tauri/src/site.rs`)

- New `kind: "php"` in the capability matrix: everything except the
  WP-specific trio (`one_click_login`, `wp_tools`, `search_replace`);
  `db_gui` true (plan 24's Adminer tooling applies), `db_sync` and
  `code_sync` true.
- Generated compose, mirroring the WP template's conventions: `app`
  (php-fpm, version from the existing `PHP_VERSIONS` allowlist), `web`
  (nginx with a static + fastcgi config template), `db` (mariadb,
  `db_port` allocation unchanged), profile-gated `adminer`.
- Creation dialog: empty docroot skeleton (Laravel-ready `public/` webroot)
  or import existing code into the site dir (same ignore-list copy as
  plan 22's Docker import). No framework installer inside the app — the
  terminal is right there for `composer create-project`.

### Phase 2 — Engine-native DB sync (`src-tauri/src/sync.rs`)

- DB export/import per kind, dispatched on capability instead of wp-cli:
  `php`/`docker`-with-db → `mysqldump` in-container for export, `mysql <
  dump` via `compose_run_stdin` for import (postgres services: `pg_dump` /
  `psql` — same dispatch table).
- No search-replace for `php`: URL config is the app's own concern. The
  import/pull flow offers a best-effort `APP_URL` patch in the project's
  `.env` (Laravel convention), off by default, clearly labeled
  best-effort.
- Push/pull orchestration, snapshots (plan 17 kinds `pre_push`/`pre_pull`),
  sync_history records, and site-event stages are kind-agnostic already
  after plan 22 — this phase is dispatch + templates, not new flow.

### Phase 3 — ServerKit parity (both repos)

- Extension (`serverkit-localkit`, ServerKit repo):
  - `/sites` payload gains `kind`; push/pull endpoints accept non-WP site
    ids and dispatch per kind: code = tar of the app's project dir (not
    `wp-content`), db = engine dump/restore instead of the WP container
    assumptions.
  - `/pair` `features` advertises supported kinds (e.g. `"kinds":
    ["wordpress", "php"]`); LocalKit disables sync/import UI for kinds the
    server's extension version doesn't know — never fails mid-flow.
- Import flow (plan 18) extends to `php`/`docker` kinds with the same
  orchestration minus WP install steps; `lk import` gains the kinds
  transparently.
- Sync v2 (plan 19) chunked protocol is kind-agnostic by design — only the
  server-side processing step in `finish` dispatches per kind.

## Risks

- PHP matrix drift (8.1/8.2/8.3 extensions): keep the image's default
  extension set; document that exotic extensions mean customizing the
  imported compose (which plan 22 makes a supported path).
- `docker` kind + ServerKit sync: arbitrary compose projects can't be
  matched to server apps reliably — v1 sync parity covers `php` only;
  `docker` sites keep local-only sync (snapshots).
- Two repo lockstep again: the `kinds` advertisement keeps old client ↔
  new server and new client ↔ old server combinations safe in both
  directions.

## Verification

- Extend `mock_localkit_ext.cjs` with a fake `php` site: full import →
  push db → pull db cycle through `m4_smoke`, asserting engine-native dump
  commands were used (mock logs them).
- `cargo test --lib sync`: per-kind dispatch table tests (every kind ×
  operation has an explicit, tested handler or a clean unsupported error).
- WP regression: all existing smoke examples pass unmodified.
