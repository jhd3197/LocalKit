# Screenshots — how they're made

The PNGs in this folder are **auto-generated** from a mock-data build, so every
site name, credential, hostname, and path is fictional. They render in the
README's screenshots section (between the `<!-- LOCALKIT:SHOTS:START -->` and
`<!-- LOCALKIT:SHOTS:END -->` markers).

## Regenerate them

```bash
npm run shots
```

That script (`scripts/capture-screenshots.mjs`):

1. starts the **mock Vite build** (`npm run dev:mock`, i.e.
   `vite --mode mock --port 1426`), which aliases `@tauri-apps/api/core`,
   `@tauri-apps/api/event` and `@tauri-apps/plugin-opener` to the in-browser
   mocks in `src/mock/` (see `vite.config.ts`) — the whole UI renders with fake
   data and **no Tauri runtime, no Docker, no real sites**;
2. drives the UI with headless Chrome/Edge (via `puppeteer-core`, using an
   already-installed browser — no download) by clicking real buttons, and
   writes the PNGs here at 1440×900 @2x (2880×1800).

## What gets captured

| File | Screen |
|---|---|
| `dashboard.png` | Site list, grid view — kind badges (WP / PHP / Docker), running / degraded / stopped / creating / setup-incomplete states, imported-from link, `.test` domain URLs |
| `dashboard-list.png` | Site list, dense list view (toolbar toggle) |
| `site-detail.png` | Detail of a running WordPress site, Overview tab (credentials, DB, wp-cli info, snapshots, sync history, logs) |
| `snapshots.png` | The Snapshots panel on its own (element crop) — manual + before-push/pull history with one-click Restore |
| `site-tools.png` | Site detail → Tools tab (Adminer DB browser, search-replace, WP\_DEBUG + log, wp-config/.env editor) |
| `new-site.png` | NewSiteDialog over the dashboard — WordPress / PHP-Laravel / Docker tabs and the blueprint picker |
| `import-site.png` | Import-remote-site dialog (from Settings → ServerKit), with the version-fallback warning |
| `settings.png` | Settings modal — Docker status, updates, data paths, version allowlists |
| `settings-domains.png` | Settings modal — Local domains section (router toggle, HTTPS trust) |
| `settings-serverkit.png` | Settings modal — ServerKit section (saved connection, expanded remote-sites table with Import buttons) |

## Tweaking the shots

- **Change the mock data** (sites, wp-cli plugins, logs, ServerKit connections,
  remote sites, sync history): edit `src/mock/data.ts`. The command payloads in
  `src/mock/core.ts` mirror the real Tauri commands in `src/lib/ipc.ts` — keep
  the two in sync when commands change. WP/PHP version allowlists mirror
  `WP_VERSIONS` / `PHP_VERSIONS` in `src-tauri/src/site.rs`.
- **Add / reorder shots or change what each captures**: edit
  `scripts/capture-screenshots.mjs`. Navigation is done by clicking real UI
  buttons via puppeteer.
- **Preview the mock build by hand**: `npm run dev:mock`, then open
  http://localhost:1426. Start/stop/delete/create work against the in-memory
  mock state, and `create_site` emits fake `site-event` progress.

If you add a shot, also add its `docs/screenshots/<name>.png` reference inside
the `LOCALKIT:SHOTS` block in `README.md`.

Note: the mock server uses port **1426** (1420 is the Tauri dev port; 1425 is
used by Faro's mock server on this machine).
