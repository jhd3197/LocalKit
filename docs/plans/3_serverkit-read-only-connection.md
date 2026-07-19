# Plan 3 — ServerKit Read-Only Connection (M3)

## Context

LocalKit's endgame is push/pull to servers managed by ServerKit. Before
building sync, M3 validates the ServerKit API surface with a read-only
connection: save a server URL + API key, test it, and (where the API allows)
browse remote WordPress sites.

## Approach

### API client — `src-tauri/src/serverkit.rs`

reqwest (rustls) client sending an `X-API-Key` header. `test_connection` =
public `GET /api/v1/system/health` (no key sent — ServerKit 401s *any*
request carrying an invalid key) + key validation via
`GET /api/v1/setup-health/account` (`@auth_required`).

### Persistence

`serverkit_connections` table (migration 2). **API keys are stored in
plaintext SQLite** — accepted for v1, keyring is planned in M5.

### Known upstream limitation

`GET /api/v1/wordpress/sites` is bare `@jwt_required()` upstream today, so
API keys get 401/422 `{"msg": ...}` — mapped to a clear "needs M4 extension"
error in the UI instead of a generic failure.

### UI

Settings page section: add/test/delete connections, browse remote sites when
the API permits.

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
bad key → clear 401 message, good key → account info shown.
