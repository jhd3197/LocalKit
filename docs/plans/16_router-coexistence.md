# 16 — Router coexistence: port-conflict pre-flight + configurable router ports

Status: ⬜ planned

Make local domains survive alongside other tools that also claim ports 80/443
and the `.test`/`.local` hosts space (LocalWP's nginx router is the canonical
case: it binds 80/443 machine-wide and answers *every* unknown local host with
its own "Site Not Found" 404 page, so a LocalKit site at `http://test.test/`
silently hits Local's router instead of LocalKit's Caddy).

## Motivation

Today the router only finds out about a port conflict *after* the fact:
`router::port_conflict_hint` string-matches the failed `docker compose up`
stderr and guesses "LocalWP's router, IIS, Skype, or another web server".
Worse, if Local is already bound to 80/443, *its* router keeps answering while
LocalKit's Caddy is down — the user sees a foreign 404 page, not a LocalKit
error, and has no idea the two apps are fighting. There is no recovery path
short of quitting the other app. Hosts-file entries coexist fine (both tools
manage their own marked blocks), so the entire conflict is about who owns
80/443.

## Design

### Phase 1 — Port pre-flight probe (`src-tauri/src/router.rs`)

- `probe_ports(http: u16, https: u16) -> Vec<PortConflict>`: try
  `std::net::TcpListener::bind(("0.0.0.0", port))` for each router port; a
  failed bind means something else owns it. Pure std, no Docker round-trip.
- `identify_port_owner(port) -> Option<String>`: best-effort process name for
  the conflict message. Windows: PowerShell
  `Get-NetTCPConnection -LocalPort <p> -State Listen` → `OwningProcess` →
  `Get-Process -Id` (spawn via `docker::no_window`); macOS/Linux:
  `lsof -nP -iTCP:<p> -sTCP:LISTEN`. Failure to identify is fine — the message
  falls back to the generic hint list.
- `PortConflict { port, process: Option<String> }` — serializable, surfaced in
  `RouterStatus` as `conflicts: Vec<PortConflict>`.
- `set_enabled` runs the probe *before* touching the hosts file; a conflict
  short-circuits with a named error ("port 80 is held by `httpd.exe`
  (LocalWP's router) — stop it or switch LocalKit to fallback ports") instead
  of writing hosts entries that would point at a foreign router.
- `status()` probes whenever `enabled && !running` so reopening the app while
  the conflict persists shows the same diagnosis instead of a bare "not
  running".

### Phase 2 — Configurable router ports (fallback mode)

- New `app_settings` keys (KV, no migration): `router_http_port` (default 80),
  `router_https_port` (default 443). Settings → Domains gets two validated
  number fields; changing them regenerates compose + Caddyfile, restarts the
  router, and calls `rewrite_site_urls` (the same path the enable toggle
  already uses).
- `render_compose` binds `<http>:80` / `<https>:443` (container ports stay
  80/443 — only the host mapping moves). `render_caddyfile` is unchanged.
- `site_url(slug, ca_trusted)` becomes port-aware: default ports → clean
  `https://slug.test`; fallback ports → `http://slug.test:8080`. All consumers
  already funnel through `site_url` / `site_public_url` (frontend `siteUrl`
  mirror, one-click login, `site.rs` install-time URL) — extend the mirror in
  `src/lib/types.ts` and the settings store accessors.
- The hosts block is port-blind (`127.0.0.1 slug.test`), so no hosts changes;
  the browser appends the port from the URL.
- HTTPS in fallback mode still works (`https://slug.test:8443`, `tls
  internal`), but the UI should default fallback URLs to http to avoid a
  second cert prompt on a non-standard port.

### Phase 3 — Conflict UX

- Settings → Domains: when `conflicts` is non-empty, show an amber callout
  naming the process + port, with two actions: "Use fallback ports"
  (one-click sets 8080/8443 and retries enable) and "Retry" (after the user
  quit the other app). No silent failures.
- SiteDetail: if the site's public URL is a domain URL and the router is in
  conflict, show a dismissible banner with the same callout (the toast alone
  is too easy to miss — the user is staring at a foreign 404 page).
- `lk doctor` reports port 80/443 ownership and the active router mode
  (default/fallback/disabled), so support questions have a copy-paste answer.

### Phase 4 — Docs

- README troubleshooting entry: "LocalWP / Local by Flywheel is installed" →
  expected behavior, fallback ports, why both apps can't share 80.
- AGENTS.md router convention block: note the port settings keys and that
  `site_url` is port-aware.

## Risks

- WP absolute URLs: in fallback mode `home`/`siteurl` contain `:8080`.
  `sync.rs` search-replace on pull uses the *current* `site_public_url`, which
  is port-aware after Phase 2, so local→remote and remote→local both
  round-trip; verify with a port-bearing URL in the m4 smoke.
- Another tool may bind 8080 too — the fallback enable path runs the same
  pre-flight probe and reports the new conflict by name instead of looping.
- Docker Desktop's own port forwarding on Windows can hold 80 briefly after a
  failed `compose up`; the probe runs before any compose mutation, so it can't
  race our own containers.

## Verification

- `cargo test --lib router`: new unit tests — `render_compose` with custom
  ports, `site_url` port formatting, probe returning empty when ports free.
- Manual matrix: (1) LocalWP running → enable domains → named conflict,
  one-click fallback → `http://test.test:8080` serves the LocalKit site;
  (2) quit LocalWP → "Retry" → clean `http://test.test`; (3) nothing
  conflicting → unchanged default behavior.
- `lk doctor` output in both modes.
