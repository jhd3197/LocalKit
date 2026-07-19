# LocalKit Roadmap

LocalKit is a desktop app that manages local WordPress sites as per-site Docker
Compose projects. v1 = milestones M1–M3 (local sites + read-only ServerKit
connection). Later milestones push/pull sites to servers through a ServerKit API
extension (`serverkit-localkit`), then release polish.

The file numbers ARE the build order — each plan leans on the ones before it.

| # | Plan file | Status | Why here |
|---|-----------|--------|----------|
| 1 | `1_local-site-lifecycle` | ✅ shipped | Foundation: compose projects, ports, start/stop/delete. |
| 2 | `2_wordpress-install-and-detail` | ✅ shipped | Sites aren't useful until WP is installed and credentials are visible. |
| 3 | `3_serverkit-read-only-connection` | ✅ shipped | Validates the ServerKit API surface before we build sync on it. |
| 4 | `4_serverkit-push-pull` | ⬜ | The actual point of the product; needs the `serverkit-localkit` extension upstream. |
| 5 | `5_release-polish` | ⬜ | Installers, updates, keyring, tests — last because it assumes feature-freeze. |

Status glyphs: ✅ shipped · 🔄 partial · ⬜ not started · 🅿️ deferred

## Track A — Local sites (M1–M2)

- ✅ Compose project generation (`docker-compose.yml` + `.env` per site)
- ✅ Port allocation (site from 8081, DB = site + 10000)
- ✅ wp-cli install via profile-gated `wpcli` service
- ✅ Credentials, logs, container status in the UI
- ⬜ Site duplication / clone (nice-to-have, unplanned)

## Track B — ServerKit (M3–M4)

- ✅ Connection model + `X-API-Key` client (migration 2)
- ✅ Health check + key validation
- 🔄 Remote site listing — blocked upstream: `GET /api/v1/wordpress/sites`
  is JWT-only; API keys get 401/422 until the M4 extension lands
- ⬜ `serverkit-localkit` ServerKit extension (API-key-aware endpoints)
- ⬜ Push local site → server, pull server site → local

## Track C — Product (M5)

- ⬜ `npm run tauri build` installers per platform
- ⬜ Auto-update (Tauri updater)
- ⬜ OS keyring for ServerKit API keys (plaintext SQLite accepted for v1)
- ⬜ Real test suite (today: `cargo check` + the `smoke` example only)
