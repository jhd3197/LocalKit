# Plan 4 — ServerKit Push / Pull (M4) ✅

## Context

The product's reason to exist: move a site between local and a server. Local
state = `wp-content/` bind mount + MariaDB. Remote state lives on a ServerKit
host. The blocker from M3: ServerKit's core WordPress endpoints are JWT-only,
so API keys can't drive them.

## Approach

### Upstream: `serverkit-localkit` extension

A ServerKit API extension exposing API-key-aware endpoints under
`/api/v1/localkit/...`: pair probe, site listing, site provisioning, code
upload, DB upload, DB download. LocalKit detects it during `test_connection`
via the pair probe and shows a precise error (404 on `/api/v1/localkit/...`)
when it's missing.

### LocalKit: sync engine — `src-tauri/src/sync.rs`

- **Push code**: in-memory tar.gz of `wp-content/` (flate2 + tar) → multipart
  POST.
- **Push DB**: `wp db export -` via the `wpcli` service → multipart POST with
  the `local_url` so the server can rewrite URLs.
- **Pull DB**: download .sql.gz → gunzip → `wp db import -` via
  `docker::compose_run_stdin` → `wp search-replace` remote URL → local URL.
- Ops emit `site-event` stages and record rows in `sync_history`
  (migration 3), shown per site in the UI.

### UI

Site detail page: ServerKit sync section (connection + remote site pickers,
Push code / Push DB / Pull DB buttons) and a sync history table. Settings can
also provision a new remote site from the connections list.

## Phases

1. `serverkit-localkit` extension upstream (auth + `/api/v1/localkit` endpoints) ✅
2. `sync.rs` — push code archive ✅
3. Push DB + pull DB with `wp search-replace` ✅
4. Sync history (migration 3) + UI ✅
5. Remote site provisioning (`create_remote_site`) ✅
6. E2E: `m4_smoke` example against `examples/mock_localkit_ext.cjs` ✅

## Integration points

`src-tauri/src/sync.rs`, `src-tauri/src/serverkit.rs` (extension client),
`src-tauri/src/db.rs` (migration 3), `src/pages/SiteDetail.tsx`,
`src/pages/Settings.tsx`, `src-tauri/examples/m4_smoke.rs`

## Risks

- Large `wp-content/` on slow uplinks → in-memory archive keeps it simple;
  chunked/resumable upload deferred
- DB charset/collation mismatches between MariaDB versions → dump flags are
  pinned in the export command
- URL rewrites breaking serialized data → always `wp search-replace`, never sed

## Verification

`cargo run --example m4_smoke` with `node examples/mock_localkit_ext.cjs`
running (port 9872): push code, push DB, pull DB round-trip against a smoke
site, then check `sync_history` rows.
