# 18 — Import a ServerKit site as a new local site

Status: ✅ shipped

One-click "clone to local" for any site on a connected ServerKit server:
provision a fresh local site, pull down the remote `wp-content` and database,
rewrite URLs, and land the user on a working local copy. Closes the last
open item in Track B (today pull only targets an *existing* local site).

## Motivation

The most common real workflow — "client's site is on the server, I need to
work on it locally" — currently requires: create a local site by hand,
delete its stock content, pull the DB, and somehow get the remote
`wp-content` (which LocalKit cannot fetch at all today: the extension has
push endpoints only). Each step is manual and the URL/plugin/theme mismatch
failure modes are unforgiving. This plan adds the missing download direction
for code and orchestrates the whole flow behind one button.

## Design

### Phase 1 — Server side: `GET /api/v1/localkit/pull/code` (ServerKit repo)

- New endpoint in the `serverkit-localkit` extension mirroring `pull/db`:
  `site_id` param → tar.gz of the remote site's `wp-content/` (streamed,
  `after_this_request` temp cleanup, same admin RBAC decorators).
- Reuse the extension's existing `_resolve_wp_content_dir` knowledge of the
  container layout; create the archive with `tar czf - -C <wp-content
  parent> wp-content` via `docker exec` so symlinks/perms survive, stream
  the file back.
- Extend `GET /pair` with a `features` array (`["sites", "push", "pull-db",
  "pull-code"]`) so LocalKit can gate the Import button on extension
  capability instead of failing mid-flow. Older extension = feature absent =
  button disabled with a tooltip.
- Also extend `GET /sites` payloads with `wp_version`, `php_version`, and
  `site_url` if not already present — the import flow needs them to pick
  local versions and run search-replace.

### Phase 2 — Orchestration (`src-tauri/src/sync.rs`)

- `pull_new_site(connection_id, remote_site_id, local_name?) ->
  Result<Site, String>`:
  1. Read remote metadata (name, wp/php version, URL).
  2. `site::create_site_files` equivalent: new record, unique slug from the
     remote name (or `local_name`), fresh ports, compose + `.env` with the
     closest matching PHP version from `PHP_VERSIONS` (record the mismatch
     in a warning event if not exact).
  3. Pre-pull images (existing `pulling` stage path).
  4. Download `pull/code` → extract into the site's `wp-content` bind mount
     (safe-extract policy: reject absolute paths, `..`, symlinks escaping
     the target — the client-side mirror of the server's tar policy).
  5. Start containers, wait healthy (existing stages).
  6. Download `pull/db` → `wp db import` via `compose_run_stdin` →
     `wp search-replace <remote_url> <local_url> --all-tables` (serialization-
     safe; `local_url` from port-aware `site_public_url`) → `rewrite flush`.
  7. Skip `wp core install` entirely — the imported DB *is* the site. The
     local `admin_user` record comes from the first administrator in the
     imported users table (`site_wp_users` logic), falling back to the
     remote admin email; the one-click login MU plugin is written by the
     existing `ensure_login_plugin` on first login.
  8. Record a `SyncRecord` (`kind: "import"`) and emit per-stage
     `site-event`s throughout, ending in `done`.
- A `pre_import` guard: refuse if a local site with the same slug exists and
  was itself created by an import linked to the same remote site — offer
  "pull into existing" instead. Store `remote_site_id` + `connection_id` on
  the local site (new columns, **migration 5**) so future pulls default to
  the right remote.

### Phase 3 — UI + CLI

- Connection detail (ServerKit page): each remote site row gets an "Import"
  button next to the existing push/pull targets → dialog with local name
  override, PHP/WP version readout (with mismatch warning), and the progress
  toast doing the rest.
- Dashboard: imported sites show a subtle link icon with the connection
  name (from the migration-5 columns).
- `lk import <connection> <remote-site> [--name <n>]` — same site/connection
  resolution rules as the rest of the CLI, `--json` prints the created site.

## Risks

- Large sites: archive is streamed but still monolithic — the 100 MB
  `MAX_CONTENT_LENGTH` on the server bounds downloads too. Plan 19 (chunked
  sync) generalizes this; the Import button should warn when the remote
  reports a huge `wp-content`.
- PHP/WP version drift: importing a PHP 8.3 site onto an 8.1 image usually
  works but not always — the mismatch warning event + sync-history note is
  enough for v1; don't attempt image-matrix matching.
- Multisite and custom `WP_CONTENT_DIR` remotes: detect and refuse with a
  clear error rather than producing a half-broken copy.

## Verification

- Extend `examples/m4_smoke.rs` (and `mock_localkit_ext.cjs`) with an
  `import` path: seed the mock server with a fake site → run import →
  assert local site runs, URLs rewritten, sync_history row written.
- `cargo test --lib sync`: safe-extract unit tests (traversal, absolute
  paths, symlink escapes) against fixture archives.
- Manual E2E against a real ServerKit box: import → one-click login works →
  edit a theme file locally → push code back.

## What shipped

All three phases, plus `scripts/verify-import.mjs` (headless UI check against
the mock server, mirroring the other plans' `verify-*.mjs`).

Deviations from the plan above, and why:

- **Feature names.** `GET /pair` reports `["sites", "push-code", "push-db",
  "pull-db", "pull-code"]` — hyphenated and split per direction, rather than
  the sketch's `["sites", "push", "pull-db", "pull-code"]`. A single `push`
  could not express a server that gained one direction but not the other.
- **`/sites` enrichment.** `url` and `wp_version` were already in the hub
  payload; only `php_version` (regexed off the compose image tag, not a
  per-site container shell) and an explicit `site_url` alias were added.
  `multisite` was already there and is what the refusal reads.
- **Version matching returns a warning, not just an event.** `match_version`
  is a pure, unit-tested function shared by the backend, and mirrored in the
  Import dialog so the user sees the mismatch *before* committing.
- **`pre_import` is stricter than sketched.** It refuses a second import from
  the same remote outright rather than offering "pull into existing" inline —
  the error names the local site to pull into, which is the same guidance
  without a second flow to build.

Two things found by running it that the plan did not anticipate:

- **`wait_for_port` is not a readiness signal.** Docker publishes the host
  port when the container is *created*, so the first wp-cli call raced the
  image entrypoint still writing wp-config.php and died with "'wp-config.php'
  not found". `site::create` never noticed because its install step retries
  for a minute. Fixed with `wordpress::wait_for_config`.
- **A hung `docker compose run` can discard a finished import.** Observed a
  container Docker reported as "Up" with no processes inside it. The optional
  post-import steps (permalink/cache flush, admin lookup) are now bounded by
  `optional()`, since they run after the data has already landed.

Deferred, deliberately:

- **Large-site warning.** The plan wanted the Import button to warn when the
  remote reports a huge `wp-content`; the extension does not report a size,
  and adding one belongs with plan 19's chunked transfer work.
- **A killed import leaves a `creating` row with live containers.** In-process
  failures clean up, but a SIGKILL cannot. That is plan 23's (reconciliation)
  job, not a second half-measure here.
