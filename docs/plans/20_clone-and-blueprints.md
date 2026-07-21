# 20 — Site clone + reusable blueprints

Status: ✅ shipped

Two related creation flows: **clone** an existing local site in one click,
and save any site as a named **blueprint** (content + config recipe) that
new sites can be created from. Builds directly on the plan-17 snapshot
engine.

## Motivation

Track A's open item ("site duplication / clone") covers the daily case:
"I need a throwaway copy of this site to test a plugin update." The wider
case is just as common: developers who spin up client sites keep
re-installing the same starter stack — same theme, same five plugins, same
settings. Today that muscle memory lives outside the app. Blueprints make a
configured site a first-class, reusable template, and both flows share 90%
of their machinery with snapshots, so the marginal cost is low.

## Design

### Phase 1 — Clone (`src-tauri/src/site.rs`)

- `clone_site(id, new_name) -> Result<Site, String>`:
  1. Snapshot the source via the plan-17 engine (`kind: "clone_source"`,
     pruned aggressively — it's an implementation detail, not a user
     snapshot).
  2. Create the target record: `unique_slug(new_name)`, fresh ports, fresh
     DB passwords + WP salts in `.env` / `wp-config` — never copy secrets.
  3. Restore the snapshot into the target's dirs/containers.
  4. `wp search-replace <source_url> <target_url> --all-tables`
     (both URLs from `site_public_url`, port-aware per plan 16) +
     `rewrite flush` + `cache flush`.
  5. Rewrite the login MU plugin (`ensure_login_plugin`) and default the
     clone's `admin_user` from the source.
- Emits `site-event` stages (`snapshot → files → containers → import →
  done`) so the progress toast works unchanged.
- UI: Clone button in SiteDetail header + dashboard card context area
  (name dialog, then progress). CLI: `lk clone <site> <new-name>`.

### Phase 2 — Blueprints

- Storage: `<data dir>/blueprints/<slug>/` = `blueprint.json` + `db.sql.gz`
  + `wp-content.tar.gz` (the snapshot artifacts, copied) —
  `blueprint.json` adds `{name, description, wp_version, php_version,
  plugins: [...], theme, created_at, source_site_name}`. Plugin/theme lists
  are captured via `wp plugin list --format=json` at save time — display
  metadata only, v1 does not re-resolve them.
- Commands: `save_blueprint(site_id, name, description?)`,
  `list_blueprints()`, `delete_blueprint(id)`,
  `create_site_from_blueprint(blueprint_id, name, ...)` — the create flow
  with steps 3–5 of clone (fresh creds/salts, restore, search-replace from
  the recorded source URL to the new site URL).
- NewSiteDialog gains a "From blueprint" section (list with plugin/theme
  chips + a description line); Dashboard empty-state suggests saving a
  blueprint once a site exists. CLI: `lk blueprint list|save|delete`,
  `lk create --blueprint <name>`.
- Portability: `lk blueprint export <name> -o site.lkbp` (single tar.gz of
  the blueprint dir) and `import` — enough to share blueprints in a team
  without building a registry.

### Phase 3 — Polish

- Blueprint thumbnails: optional; capture the dashboard screenshot pipeline
  (`scripts/capture-screenshots.mjs`) is dev-only, so v1 = a generated
  initial-letter tile, not site screenshots.
- Router integration: clones/blueprint-sites are ordinary sites — Caddyfile
  regen + hosts sync already hook site create/delete.

## Risks

- Cloning a running site: the snapshot reads a live DB — `wp db export` is
  consistent enough for dev; document that cloning quiesces nothing.
- Blueprint staleness: a blueprint's WP core version is whatever the image
  provides (content only stores `wp-content` + DB) — the create dialog shows
  the recorded `wp_version` and warns if the local allowlist no longer has
  it (falls back to nearest).
- Disk: blueprints duplicate snapshot bytes. `save_blueprint` hardlinks the
  snapshot artifacts when the fs allows, copies otherwise.

## Verification

- Extend `examples/smoke.rs` with a `clone` subcommand: create → add a post
  → clone → assert the post exists at the clone's URL and passwords differ.
- Unit tests: blueprint manifest serde, hardlink-or-copy fallback, slug
  uniqueness against existing sites.
- Mock mode: sample blueprints in `src/mock/data.ts` so the NewSiteDialog
  section is reviewable without Docker.
