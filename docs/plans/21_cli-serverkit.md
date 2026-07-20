# 21 — `lk` CLI: ServerKit connections, push/pull, shell completions

Status: ⬜ planned

Close out Track D: give the `lk` CLI full access to the ServerKit side of
the app — manage connections, list remote sites, push, pull — plus shell
completions. Everything is a thin wrapper over `localkit_lib` calls that
already exist; this plan is mostly CLI ergonomics and conventions.

## Motivation

The GUI can do everything ServerKit-related; the CLI can do none of it.
That blocks scripting the exact workflows the CLI exists for ("nightly
`lk pull db` before I start work", CI-flavored local refreshes) and leaves
Track D's checkboxes open. Because `sync.rs` and `serverkit.rs` already do
the heavy lifting and emit progress to stderr when there's no Tauri handle,
this is a high-value, low-risk surface expansion.

## Design

### Phase 1 — Connections (`src-tauri/lk/src/main.rs`)

- `lk connection add <name> <url>` — prompts for the API key with a hidden
  TTY prompt (`rpassword`; `--key` flag / `LOCALKIT_API_KEY` env for
  non-TTY), then immediately runs the same `test_connection` validation as
  the GUI (health → key → extension probe) and refuses to store a key that
  doesn't validate.
- `lk connection list` — table or `--json`: name, url, extension version /
  features, last used. `lk connection test <name>` re-runs validation.
  `lk connection remove <name>` — prompts (default No), `--yes` on non-TTY.
- Connections resolve by exact id or case-insensitive name, same rule as
  sites; ambiguity is an error listing the matches.

### Phase 2 — Sync commands

- `lk sites --remote <connection>` — remote site listing via the extension
  (new read-only wrapper over `serverkit.rs`).
- `lk push <site> --code|--db [--connection <name>]` and `lk pull <site>
  --db [--connection <name>]`. `--connection` is required only when the
  site has no linked remote (plan 18's migration-5 columns) and more than
  one connection exists.
- Progress: the library's `site::emit` already prints `[stage] message` to
  stderr with no app handle; v2 byte progress (plan 19) prints a
  `\r`-redrawn single-line percentage on TTY, plain lines when piped.
- Exit codes: 0 success, 1 error, 2 = remote rejected the operation
  (distinguishable in scripts). Errors keep the `error: <msg>` stderr
  convention.
- `--json` on push/pull prints the resulting `SyncRecord`.

### Phase 3 — Completions + doctor

- `lk completions <bash|zsh|fish|powershell>` via `clap_complete` — static
  subcommand/flag completion (dynamic site-name completion is a later
  stretch; the generator hooks make it cheap to add).
- `lk doctor` gains connection checks: for each stored connection, DNS +
  TLS + `/pair` reachability, printed as pass/fail lines — one command to
  answer "is it me or the server".

## Conventions (binding, per AGENTS.md)

- stdout carries data only; all chrome/progress/✓ to stderr.
- `--json` per command, always pretty.
- No logic in the CLI crate — anything reusable goes into `localkit_lib`
  (e.g. the "resolve connection by name" helper lives next to the site
  resolver).

## Verification

- Against `examples/mock_localkit_ext.cjs` (extended in plans 18/19):
  scripted run — `connection add` (env key) → `sites --remote` → `push
  --code` → `pull --db` → assert exit codes and `--json` shapes.
- `clap_complete` output smoke: generate all four shells, assert non-empty
  and stable (snapshot test).
- Manual: `lk doctor` with the mock server up/down.
