# Plan 5 — Release Polish (M5)

## Context

v1 features are in, but the app is dev-mode only: no installers, no updates,
API keys in plaintext SQLite, and no automated tests beyond the smoke example.
M5 makes LocalKit distributable and maintainable.

## Approach

### Installers

`npm run tauri build` per platform (Windows NSIS/MSI first; macOS .dmg and
Linux .AppImage/.deb when CI runners exist). Icons already live in
`src-tauri/icons/`.

### Auto-update

Tauri updater plugin with signed releases published from GitHub Releases.

### Keyring

Replace plaintext ServerKit API keys with the OS keyring (keyring crate);
SQLite keeps a key reference. Migration: on first run after upgrade, move
existing keys and null the column.

### Tests

- Rust: unit tests for port allocation, slug generation, compose template
  rendering, migration forward-onlyness
- Frontend: `npm run build` type-check stays the gate; store-level tests only
  where cheap
- E2E: keep the `smoke` example as the real-Docker gate; run in CI with a
  Docker service

## Phases

1. CI workflow (cargo check + tsc/vite build)
2. Release workflow (build + sign + publish installers)
3. Tauri updater wiring
4. Keyring migration (migration 3)
5. Test suite + coverage of the risky bits (ports, templates, migrations)

## Integration points

`.github/workflows/{ci,release}.yml` (new), `src-tauri/src/db.rs`
(migration 3), `src-tauri/src/serverkit.rs` (keyring), `tauri.conf.json`

## Risks

- Code signing cost/availability on Windows → start unsigned with a SmartScreen
  note, sign later
- Updater regressions → staged rollout via GitHub Releases prerelease channel

## Verification

Install from a produced installer on a clean machine, create + delete a site,
update from version N to N+1, confirm keyring migration preserves connections.
