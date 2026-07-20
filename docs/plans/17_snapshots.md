# 17 — Local site snapshots & one-click restore

Status: ⬜ planned

Point-in-time copies of a site (DB dump + `wp-content` archive) with
one-click restore, taken automatically before every destructive operation
(push, pull, delete) and manually from the UI/CLI. This is the safety net
that makes plan 18 (import) and plan 19 (sync v2) safe to build on.

## Motivation

Every mutating operation in LocalKit is currently one-way: pull DB overwrites
the local database, push DB overwrites the *remote* database, delete is
forever. `sync_history` records that something happened but cannot undo it.
A bad search-replace or a pull against the wrong connection means data loss.
Snapshots turn all of these into reversible operations and give users a
cheap "checkpoint before I try something" habit.

## Design

### Phase 1 — Snapshot engine (`src-tauri/src/snapshot.rs`)

- Layout: `<data dir>/snapshots/<site_id>/<ts>/` containing `db.sql.gz`
  (`wp db export -` via `docker::compose_run`, gzipped with flate2 — same
  stack as `sync.rs`), `wp-content.tar.gz` (in-memory tar of the
  bind-mounted `wp-content/`, same code path as push code), and
  `manifest.json`.
- Manifest: `{site_id, created_at, kind, note, db_bytes, code_bytes,
  wp_version}` where `kind` is `manual | pre_push | pre_pull | pre_delete |
  pre_restore`. Yes, snapshot before restore too — restoring is destructive.
- Commands: `list_snapshots(site_id)`, `create_snapshot(site_id, kind,
  note?)`, `restore_snapshot(snapshot_id)`, `delete_snapshot(snapshot_id)`.
  Restore = `wp db import` via `compose_run_stdin` + extract the tar into the
  site's `wp-content` dir (plain fs write, bind-mounted) + `wp cache flush`.
  Site must be running to import the DB; restore auto-starts a stopped site
  and reports that it did.
- Long operations emit `site-event` stages (`snapshot` / `restore`) so the
  pinned progress toast pattern keeps working.
- Retention: auto-kinds are pruned to the newest 5 per site per kind after
  each create; `manual` snapshots are never auto-pruned. Deleting a site
  keeps its snapshots directory unless the delete dialog's new "also delete
  snapshots" checkbox is checked.

### Phase 2 — Wiring into destructive flows

- `sync.rs`: `push_db` and `pull_db` take a local pre-sync snapshot first
  (kind `pre_push` / `pre_pull`, note = connection name + remote URL).
  Failure to snapshot aborts the sync with a clear error — never mutate
  without a net.
- `site.rs::delete_site`: kind `pre_delete` snapshot, blocking, before any
  container teardown. Delete dialog copy: "A restorable snapshot will be
  kept."
- Snapshot-before-delete is also the foundation for a future "restore deleted
  site" flow (out of scope here, but the manifest keeps enough metadata).

### Phase 3 — UI + CLI

- SiteDetail → new "Snapshots" tab: table (created, kind badge, note, sizes,
  DB/code presence), actions Restore / Delete / (manual) Create with an
  optional note field. Restore confirms with a dialog that names the
  snapshot time and mentions the pre-restore snapshot.
- `lk snapshot list|create|restore|delete <site>` following CLI conventions
  (stdout data only, `--json`, restore prompts with default No, `--yes` on
  non-TTY).
- Command palette: "Create snapshot" per-site command via the existing
  `buildCommands()` per-site command block.

## Risks

- Disk usage: `wp-content` archives can be large (uploads). Mitigate with
  the retention cap + sizes visible in the UI + the delete-site checkbox.
  A future plan can add exclude-paths for `uploads/cache`-style dirs.
- Restore while the user has the terminal open mid-write is racy in theory;
  in practice `wp db import` is atomic enough and the site stays up. Not
  worth a maintenance-mode flag for v1.
- Snapshot of a site whose containers are stopped: DB export needs the db
  container — auto-start just the db service (`docker compose up -d db`),
  wait healthy, export. Reuse the create flow's wait loop.

## Verification

- New `cargo run --example snapshot_smoke` — create site → snapshot → break
  the DB (`wp post delete 1 --force`) → restore → assert the post is back;
  pre-delete snapshot survives site deletion.
- Unit tests: retention pruning (pure function over manifest list), manifest
  serde round-trip.
- `npm run dev:mock`: mock snapshots in `src/mock/data.ts` + `core.ts` so the
  tab renders without Docker.
