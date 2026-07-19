# Plan 2 — WordPress Install & Site Detail (M2)

## Context

After M1 a site is running containers but no usable WordPress: no install, no
admin credentials, no insight into the site. M2 makes a created site actually
log-in-able and gives it a detail page.

## Approach

### wp-cli — `src-tauri/src/wordpress.rs`

The stock `wordpress` image has no wp-cli, so a profile-gated `wpcli` service
(`wordpress:cli-php<ver>`) runs via `docker::compose_run`. **Always pass `wp`
as the first argument** — the cli image's `wp` CMD is replaced by run args, so
omitting it makes the entrypoint exec `core` and fail.

### Install flow

After containers are up, wait for WordPress to respond, then
`wp core install` with generated admin credentials stored on the site row and
shown (copyable) in the UI.

### Site detail page — `src/pages/SiteDetail.tsx`

Open site / wp-admin buttons (opener plugin), copyable admin + DB credentials
(`CopyButton`), container log viewer, and wp-cli info (`wp core version`,
`wp plugin list`).

## Phases

1. `wpcli` service in the compose template (profile-gated)
2. `wordpress.rs` — install + info wrappers over `compose_run`
3. Wait-for-WordPress stage in the create flow (`waiting` event stage)
4. Credentials on the `Site` model (migration stays v1-compatible)
5. SiteDetail page + `CopyButton` + log viewer

## Integration points

`src-tauri/src/wordpress.rs`, `src-tauri/src/site.rs` (compose template),
`src/pages/SiteDetail.tsx`, `src/components/CopyButton.tsx`

## Risks

- First `docker compose up` pulls large images → progress events keep the UI
  honest; install only runs after the `waiting` stage succeeds
- Generated credentials shown once vs stored — stored in SQLite so the detail
  page can always show them (local-only app, accepted)

## Verification

`cargo run --example smoke -- create verify info` — `verify` checks the site
responds, `info` exercises the wp-cli info path.
