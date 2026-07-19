# Plan 6 — Local Domains & HTTPS (`mysite.test`) — ✅ shipped

## Context

Sites used to be reached as `http://localhost:<port>`, which leaks an
implementation detail (port allocation) into the UX. Tools like LocalWP offer
`mysite.local` with HTTPS. M6 adds the same: `http(s)://<slug>.test` via a
shared reverse proxy, managed hosts-file entries, and a local CA the user can
trust with one click.

## Shipped approach

### TLD — `.test` (RFC 2606)

Reserved for testing, never collides with LocalWP's `.local`, and resolves
consistently across browsers **and** OS resolvers. (`.localhost` was
considered — browsers auto-resolve it without hosts edits — but rejected:
non-browser tools don't, and explicit hosts entries make behavior identical
everywhere, including curl.)

### Router container — `src-tauri/src/router.rs`

One shared compose project at `<data dir>/router/` running **Caddy** on host
ports 80/443, with `extra_hosts: ["host.docker.internal:host-gateway"]` and a
named `caddy-data` volume. Routes proxy to `host.docker.internal:<site port>`,
so per-site compose projects are completely untouched (no shared Docker
network). The generated Caddyfile serves BOTH schemes per site, with no
http→https redirect (users who haven't trusted the CA get a clean http path):

```
http://<slug>.test  { reverse_proxy host.docker.internal:<port> }
https://<slug>.test { tls internal; reverse_proxy host.docker.internal:<port> }
```

The Caddyfile is regenerated + `caddy reload`ed (falling back to
`compose restart`) on site create/start/stop/delete and on enable.

### Hostnames — managed hosts-file block

`127.0.0.1  <slug>.test` entries inside `# BEGIN LOCALKIT` / `# END LOCALKIT`
markers in `C:\Windows\System32\drivers\etc\hosts` (Windows) or `/etc/hosts`
(macOS/Linux). The block is added/updated/removed on enable/disable and site
create/delete. The block-content logic is a pure function
(`update_hosts_content`, unit-tested for idempotency, CRLF preservation, and
byte-identical removal).

Writes require elevation, via a one-shot helper invoked per-OS:
Windows = PowerShell `Start-Process -Verb RunAs -Wait` (UAC prompt, temp .bat
does the copy), macOS = `osascript … with administrator privileges`, Linux =
`pkexec` (fallback: error message contains the exact `sudo cp` command). The
write is verified by re-reading the file afterwards. If elevation is declined,
`domains_enabled` stays off and the UI shows "administrator approval needed";
on disable the router is still stopped even if block removal fails (with a
warning — stale entries just loopback harmlessly).

### HTTPS trust

`trust_router_ca` extracts Caddy's local-CA root
(`/data/caddy/pki/authorities/local/root.crt`) via `docker compose cp` and
installs it per-OS: Windows `certutil -user -addstore Root` (current-user, no
admin), macOS `security add-trusted-cert` (login keychain), Linux best-effort
(`sudo cp` + `update-ca-certificates`). "Trusted" is recorded in the settings
table on command success — we don't probe OS trust stores.

### Settings flag — migration 4

`app_settings(key TEXT PRIMARY KEY, value TEXT)` with keys `domains_enabled`
(default off), `router_ca_trusted`, `router_last_error`.

### WordPress URLs

Enabling rewrites `home`/`siteurl` to `http(s)://<slug>.test` on every running
site (wp-cli `option update`); disabling reverts to `http://localhost:<port>`.
New installs use the domain URL when domains are enabled at install time. All
best-effort: failures are collected into the router status error, never fail
the enable/disable operation.

### Fallback

Port 80/443 conflicts (LocalWP's router, IIS, Skype, nginx, …) produce a
friendly "what's probably holding the port" error, the flag stays off, and
sites keep working on `localhost:<port>` — domains are additive, never
required.

## What shipped

1. `src-tauri/src/router.rs` — Caddy project, Caddyfile gen, hosts block
   (pure logic + elevated writer), CA trust, status; 7 unit tests
2. Migration 4 (`app_settings`) + 3 Tauri commands: `router_status`,
   `set_domains_enabled`, `trust_router_ca`
3. Site lifecycle hooks: Caddyfile/hosts refresh on create/delete, Caddyfile
   refresh on start/stop, domain install URL
4. UI: Settings → Local domains (toggle, router status, trust-CA button);
   Dashboard/SiteDetail show and open `*.test` URLs (`src/lib/domains.ts`,
   `src/stores/router.ts`); mocks + `settings-domains.png` screenshot

## Verification (done)

`cargo check` ✓ · `cargo test --lib router` (7 hosts-block tests) ✓ ·
`npm run build` ✓ · `npm run shots` with `.test` URLs in dashboard/detail and
the domains section ✓ · Caddy routing + proxy chain proven against a live
site (in-container through Caddy: WP responded via
`host.docker.internal:<port>`) ✓. The elevated hosts write needs an
interactive UAC prompt, so it was verified by code review + unit tests, not
E2E. Manual passes on macOS/Linux elevation paths are still pending.

## Risks (remaining)

- Port 80/443 conflicts → detected on enable with remediation text
- Elevation UX is platform-specific → single helper per OS; macOS/Linux paths
  untested on real machines yet
- Antivirus/SmartScreen flagging hosts edits → document; sign with plan 5
