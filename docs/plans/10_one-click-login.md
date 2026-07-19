# 10 — One-click WP Admin login (magic login link)

Status: ⬜ not started

Add a **"WP Admin"** button that logs the user straight into the site's
`wp-admin` — no copy-pasting the password from the credentials card. One
site can have multiple WP users over time, but v1 of this feature logs in as
the default admin user created at install.

## Motivation

Every local-WP tool has this (LocalWP's "WP Admin" button, Studio's "WP
admin" link). Today LocalKit shows the credentials and makes the user log in
manually every browser session — the single most repeated action in the app.
The admin credentials already exist in the DB (`sites.admin_user` /
`admin_pass`), so this is pure UX glue.

## Design

**Mechanism: one-time login token + tiny must-use plugin.** Plugin-free
alternatives (application passwords, `wp_set_auth_cookie` via `wp eval`)
can't establish a browser session from the CLI, so we ship a ~30-line
MU plugin:

- `wp-content/mu-plugins/localkit-login.php` — dropped in by `site.rs` at
  create time (the site dir already bind-mounts `./wp-content` into the
  container, so it's a plain `std::fs` write — no `compose cp`, works
  stopped or running; add it to the create flow next to the compose/env
  templates, and to existing sites lazily on first login use).
- The MU plugin hooks `init` and only acts when
  `$_GET['localkit-login']` is present on `wp-login.php`: it reads the
  stored token from a WP option, compares with `hash_equals`, checks the
  TTL (~120 s), deletes the option (one-time use), `wp_set_current_user` +
  `wp_set_auth_cookie` for the requested user id, then `wp_safe_redirect`
  to `admin_url()`. Any failure → normal wp-login form, no error detail.

**Flow.**

1. UI: **WP Admin** button on SiteDetail (primary action next to "Open
   site"; disabled with tooltip unless the site is running).
2. Frontend calls a new `login_site(id: string, user_id?: number) -> string`
   Tauri command (typed wrapper in `src/lib/ipc.ts`, opener plugin opens the
   returned URL).
3. Backend (`wordpress.rs` + a thin command in `lib.rs`): generates a
   32-byte hex token, stores it via
   `wp option update localkit_login_token '<hex>' --autoload=no` (plus a
   `localkit_login_exp` timestamp option), then returns
   `<site_url>/wp-login.php?localkit-login=<hex>&uid=<id>`.
4. Browser opens the URL, MU plugin consumes the token, user lands in
   wp-admin logged in.

**Security posture.** This is a localhost dev tool: token is one-time,
~2-minute TTL, generated per click, never logged (don't echo the full URL
into `site-event` messages or `lk` output history), and `hash_equals`
compared. The MU plugin is clearly named and removable. Fine for v1; revisit
if sites ever become reachable off-machine.

**Default user.** `user_id` omitted → the site's `admin_user` (resolved to a
user id via `wp user get <login> --field=ID`). 

## Multi-user (phase 2 — plan for it, build after the happy path)

- `wp user list --format=json` → a small user picker (dropdown on the WP
  Admin button, defaulting to admin). One site, many users — the token
  endpoint already takes `uid`, so this is UI + one wp-cli call.
- Creating additional WP users from LocalKit (role picker etc.) stays manual
  via `lk wp` / wp-admin for now — not part of this plan.

## CLI

`lk login <site> [--user <id|login|email>]` — prints the one-time URL on
stdout (scriptable, per CLI conventions); `--open` opens it in the default
browser via the `open`/`opener` crate equivalent used elsewhere. Thin
wrapper over the same `wordpress::` function — no logic in the CLI crate.

## Implementation notes

- New `wordpress::login_url(dir, site, user) -> Result<String, String>`; the
  Tauri command and `lk login` both call it.
- MU plugin content is a `const &str` in `site.rs` (or
  `wordpress.rs`) next to the compose templates; write it idempotently
  (skip if identical file exists) so upgrades can replace it later.
- Works for both URL modes: `localhost:<port>` and `https://<slug>.test`
  (M6) — build the base URL exactly like the Open button does today.
- After a **pull DB** from ServerKit (M4), local users/passwords are
  overwritten by remote ones — the magic login still works (it doesn't need
  the password), which is a nice side benefit; make sure the default-user
  resolution handles `admin_user` not existing remotely (fall back to first
  administrator from `wp user list --role=administrator`).

## Definition of done

- Running site → click **WP Admin** → land in wp-admin logged in as admin,
  on both URL modes, without touching the password.
- Token is single-use (second visit of the same URL shows the login form).
- `lk login test` prints a working URL; `--open` opens it.
- User picker lists WP users and logs in as the selected one (phase 2).
- `cargo check` + `npm run build` clean; covered manually via the smoke
  site.
