# 25 — Release polish completion: updater, keyring, notifications, test suite

Status: ✅ implemented (on `dev`) — all four phases: update checker,
OS keyring for API keys, OS notifications, and the automated test suite
(Rust `cargo test --workspace` + frontend `vitest`, both wired into CI).

Finish the genuinely remaining M5 work (plan 5 predates the CI/release
workflows, which shipped separately): in-app update awareness, OS-keyring
storage for ServerKit API keys, OS desktop notifications for long
operations, and a real automated test suite.

## Motivation

Releases already build and publish for all platforms via
`.github/workflows/release.yml`, but the *installed* app has no idea newer
versions exist — users only update by re-downloading manually. ServerKit
API keys sit in plaintext SQLite (accepted for v1, now the largest security
debt in the app). Long operations (create, push, pull) complete silently
when the window is unfocused or closed-to-tray. And the test surface is
still `cargo check` + a handful of unit tests + manual smoke examples,
which every plan above (16–24) will strain. These four items are the
difference between "works on my machine" and a distributable product.

## Design

### Phase 1 — Update awareness

- `tauri-plugin-updater` requires signed releases; our releases are
  unsigned. So: a lightweight checker instead — on launch (and daily),
  GET the latest GitHub release tag via the API; if newer than
  `env!("CARGO_PKG_VERSION")`, show a dismissible toast + a Settings →
  General "Update available" row linking to the release page
  (opener plugin). Snooze state + last-checked in `app_settings` (KV).
- Same check in `lk` (`lk doctor` prints "update available: vX.Y.Z"; never
  auto-downloads).
- If releases become signed later, swapping the checker for the real
  updater is a drop-in replacement behind the same Settings row.

### Phase 2 — OS keyring for ServerKit API keys

- `keyring` crate (Windows Credential Manager / macOS Keychain / Secret
  Service) keyed `localkit/connection/<id>`.
- `serverkit.rs` gains a `KeyStore` abstraction with two backends; read
  path = keyring → SQLite fallback (legacy) → migrate-on-read (write to
  keyring, null the column). New/changed keys only ever touch the keyring.
- Graceful degradation: keyring unavailable (headless Linux, locked
  keychain) → fall back to SQLite with a one-time warning logged, never a
  hard failure. `lk` on servers keeps working.
- `serverkit_connections.api_key` column stays (nullable) for downgrade
  compat — no migration needed, just stop writing it.

### Phase 3 — OS desktop notifications

- `tauri-plugin-notification`: fire on completion of long operations
  (site created, push/pull done or failed, restore done) **only when the
  window is unfocused or closed-to-tray** — the toast system already owns
  in-focus feedback, and double-notifying is worse than either alone.
- Settings → General toggle `osNotifications` (default on), per the
  settings-store conventions. Clicking a notification focuses the window
  (single-instance plugin already handles focus).

### Phase 4 — Test suite

- Rust (`cargo test --workspace`, already wired in CI): unit tests per
  pure module — `site::slugify`/`unique_slug`/port allocation, `db`
  migration forward-only invariants (apply 1→N twice, assert
  `user_version`), `sync` archive builders, plus whatever plans 16–24 add
  (probe parsing, chunker, reconcile decision table, retention pruning).
- Frontend (`vitest`, new dev-dep, added to the CI build job):
  `lib/shortcuts.ts` canonicalizer, `lib/fuzzy.ts`, `lib/keybindings.ts`
  resolver, settings store parsing (`"true"`→bool, migrations), toast
  dedupe logic in `lib/errors.ts`.
- Keep the smoke examples as the E2E layer; the unit suites exist so most
  regressions are caught without Docker.

## Risks

- Keyring prompts: macOS may show a keychain permission dialog on first
  access — acceptable one-time cost; documented in Settings copy.
- Notification permission on macOS must be requested at runtime; treat
  denial as "toggle off", don't nag.
- Vitest + jsdom for store tests: keep them DOM-free where possible (pure
  logic), mock `window.__LOCALKIT_SETTINGS__` explicitly.

## Verification

- `cargo test --workspace` + `npm run test` green in CI (new step).
- Manual: install previous release → launch → update toast appears → link
  opens release page. Add a connection → key visible in Windows Credential
  Manager, absent from SQLite. Close to tray → run a push from `lk` →
  completion notification appears.
