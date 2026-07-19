# 7 — CLI companion (`lk`)

Status: ✅ shipped (v1 core)

A headless command-line companion for LocalKit that can create and manage
local WordPress sites without opening the GUI. It shares the same SQLite DB
and data dir as the desktop app, so both always see the same sites.

## Motivation

Everything the app does already runs headless — the `smoke` / `m4_smoke`
examples drive the full lifecycle with no Tauri runtime. The CLI productizes
that: same library, same data dir, scriptable output, CI-friendly.

## Design

- **Separate workspace crate.** `lk` lives at `src-tauri/lk/` (workspace
  member of the `src-tauri` root package), not as a `[[bin]]` in the GUI
  package — a second `[[bin]]` there breaks the macOS universal bundler.
  It depends on `localkit_lib` by path, exactly like the GUI binary does.
- **Thin binary.** All logic stays in the library; the CLI only parses args,
  resolves sites, and formats output. Pure helpers (site matching, export
  rendering) are unit-tested in the same file.
- **Shared state.** Default data dir = the GUI's (`dirs::data_dir()/LocalKit`).
  Override with `--data-dir <path>` or the `LOCALKIT_DATA_DIR` env var.
- **stdout/stderr discipline.** stdout carries data only (tables, JSON, URLs,
  exports, wp output); chrome — ✓ success lines, → hints, ! warnings,
  `[stage]` progress — goes to stderr. `lk list --json | jq` is always clean.
- **Conventions.** `--json` is per-command, always pretty-printed, raw
  payload (no envelope). Errors print `error: <msg>` in red on stderr,
  exit code 1. Colors are hand-rolled ANSI, disabled by `--no-color`,
  `NO_COLOR`, or a non-TTY stdout.
- **Site addressing.** Exact id wins, then case-insensitive slug, then
  case-insensitive name. Ambiguity → error listing matches; no match →
  error listing available slugs.
- **Destructive ops.** `lk delete` prompts (default No); `--yes` skips the
  prompt and is *required* when stdout is not a TTY (no silent deletes in
  scripts).
- **Progress without a GUI.** Long operations emit `site-event` stages; when
  there is no Tauri app handle, `site::emit` prints `[stage] message` to
  stderr instead of dropping the event. Zero signature changes; the GUI and
  the smoke examples are unaffected.

## Command surface (v1 core)

| Command | What it does |
|---|---|
| `lk list [--json]` | Table of sites: slug, live status, URL, WP/PHP versions |
| `lk create <name> [--wp-version V] [--php-version V] [--json]` | Full create flow; URL on stdout, admin credentials on stderr |
| `lk start <site>` / `lk stop <site>` / `lk restart <site>` | Lifecycle (restart = stop + start so DB status stays correct); start/restart print the URL |
| `lk delete <site> [--yes]` | Confirms interactively; `--yes` required non-interactively |
| `lk info <site> [--json]` | Full detail incl. DB credentials |
| `lk logs <site> [--tail N]` | Container logs (default tail 100) |
| `lk wp <site> <args...>` | wp-cli passthrough via the `wpcli` compose service (always prepends `wp`) |
| `lk env <site> [--shell bash\|powershell] [--json]` | Eval-able exports: `LOCALKIT_SITE_URL`, `DB_HOST/PORT/NAME/USER/PASSWORD` (hint goes to stderr, exports to stdout) |
| `lk doctor` | Checks: Docker daemon reachable, compose plugin present, data dir writable. Exit 1 on failure |

## Known trade-offs (accepted for v1)

- The `lk` crate compiles the `tauri` crate (it links `localkit_lib`), so
  builds are heavier than a pure CLI. A future refactor could feature-gate
  tauri or split a core crate — not worth it yet.
- Progress output is the simple stderr `emit` fallback, not a spinner/progress
  bar. A proper event-sink trait (GUI emitter vs. CLI renderer) is a future
  refinement.

## Future work (not planned)

- ServerKit from the CLI: `lk connection add/list`, `lk push code|db`,
  `lk pull db`, `lk sync history` (the library calls already exist in
  `sync.rs` / `serverkit.rs`).
- Shell completions (`clap_complete`), self-update.

## Verification

- `cargo check --all-targets` clean; `cargo test -p lk` (site resolution +
  export rendering unit tests).
- E2E against real Docker: `lk doctor` → `lk create clitest` → `lk list` →
  `lk info clitest` → `lk wp clitest option get siteurl` → `lk env clitest` →
  `lk stop clitest` → `lk restart clitest` → `lk delete clitest --yes`.
- Non-TTY `lk delete` without `--yes` refuses (safety).
- `cargo run --example smoke -- cleanup` still passes (emit refactor).
