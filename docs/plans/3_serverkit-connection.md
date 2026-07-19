# Plan 3 — ServerKit Connection (M3) ✅

## Context

LocalKit's endgame is push/pull to servers managed by ServerKit. Before
building sync, M3 validates the ServerKit API surface: save a server URL +
API key, test it, detect whether the `serverkit-localkit` extension is
installed, and browse remote WordPress sites.

## Approach

### API client — `src-tauri/src/serverkit.rs`

reqwest (rustls) client sending an `X-API-Key` header. `test_connection` =
public `GET /api/v1/system/health` (no key sent — ServerKit 401s *any*
request carrying an invalid key) + key validation via
`GET /api/v1/setup-health/account` (`@auth_required`) + a
`/api/v1/localkit/pair` probe that reports whether the extension is present.

### Persistence

`serverkit_connections` table (migration 2). **API keys are stored in
plaintext SQLite** — accepted for v1, keyring is planned in M5.

### Why the extension

Core `GET /api/v1/wordpress/sites` is bare `@jwt_required()` upstream, so API
keys get 401/422 there. All WordPress operations therefore go through the
`serverkit-localkit` extension (`/api/v1/localkit/...`), which is
API-key-aware; a missing extension maps to a clear "install the extension"
error.

### UI

Settings page section: add/test/delete connections (test result shows health,
key validity, and extension presence), browse remote sites, provision new
ones.

## Phases

1. `serverkit.rs` — client + connection model + friendly error mapping
2. Migration 2 (`serverkit_connections`)
3. Tauri commands — save/test/delete/list connections, list remote sites
4. Settings UI

## Integration points

`src-tauri/src/serverkit.rs`, `src-tauri/src/db.rs` (migration 2),
`src/pages/Settings.tsx`, `src/lib/ipc.ts`, `src/lib/types.ts`

## Risks

- ServerKit API changes upstream → client is small and centralized in one file
- Plaintext keys → documented in README; keyring tracked in plan 5

## Verification

Manual against a real ServerKit instance: health check passes without a key,
bad key → clear 401 message, good key → account info + extension flag shown.
