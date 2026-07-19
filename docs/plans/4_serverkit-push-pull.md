# Plan 4 — ServerKit Push / Pull (M4)

## Context

The product's reason to exist: move a site between local and a server. Local
state = `wp-content/` bind mount + MariaDB. Remote state lives on a ServerKit
host. Today's blocker: ServerKit's WordPress endpoints are JWT-only, so API
keys can't drive them.

## Approach

### Upstream: `serverkit-localkit` extension

A ServerKit API extension exposing API-key-aware endpoints for the operations
LocalKit needs: list WP sites, export a site (files archive + DB dump),
import a site, and job-status polling for long transfers. Until it exists,
LocalKit keeps showing the M3 "needs M4 extension" message.

### LocalKit: sync engine — `src-tauri/src/sync.rs` (new)

- **Push**: `wp db export` via the `wpcli` service → archive `wp-content/` +
  dump → stream to the extension → remote import job → poll status.
- **Pull**: request remote export → download → create local site (reuse plan 1
  lifecycle) → import DB via `wp db import` → search-replace URL via
  `wp search-replace`.
- Progress over the existing `site-event` channel with sync-specific stages.

### UI

Site detail gets Push/Pull actions per saved connection; a sync progress view
mirrors the create-site progress.

## Phases

1. `serverkit-localkit` extension upstream (auth + export/import endpoints)
2. `sync.rs` — local export (db dump + wp-content archive)
3. Push path + job polling
4. Pull path (creates a local site through the existing lifecycle)
5. URL search-replace + post-sync health check
6. UI actions + progress

## Integration points

`src-tauri/src/sync.rs` (new; reuses `docker.rs::compose_run`,
`wordpress.rs`, `serverkit.rs`), `src/pages/SiteDetail.tsx`,
`src/stores/sites.ts`

## Risks

- Large sites / slow uplinks → chunked upload, resumable if cheap, else clear
  failure + retry
- DB charset/collation mismatches between MariaDB versions → pin dump flags,
  test matrix
- URL rewrites breaking serialized data → always use `wp search-replace`,
  never sed

## Verification

Round-trip: create local site with content → push to a staging ServerKit host
→ verify remote → pull back under a new name → diff content and options.
