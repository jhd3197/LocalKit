# 9 — Windows: hide console windows & fix install "hang"

Status: ✅ shipped

Two related Windows papercuts observed while creating a site from the
installed MSI (screenshot 2026-07-19): clicking **Create** pops a black,
empty console window (`C:\Program Files\Docker\...` in the title bar) that
sits on top of the app, and the UI then sits on **"Installing WordPress..."**
for a very long time with no visible progress and no error.

## Motivation

- A desktop app must never flash console windows — it looks broken and
  steals focus. Every other dev tool (LocalWP, Docker Desktop) hides this.
- "Installing WordPress..." with no feedback is indistinguishable from a
  hang. First-time creation on a machine legitimately takes minutes (image
  pulls), but the user can't tell *that* from a stuck loop.

## Part 1 — Suppress console windows on Windows

**Root cause.** The app is a GUI-subsystem binary, but every
`tokio::process::Command` spawn of a console program (`docker.exe`,
`powershell.exe`, `certutil.exe`) allocates a visible console window on
Windows. Fix is the `CREATE_NO_WINDOW` creation flag.

**Spawn sites to cover** (grep `Command::new` under `src-tauri/src/`):

- `docker.rs` — `check()` (line ~26), `compose_output()` (~77),
  `compose_run_stdin()` (~130). These are the hot path (every compose call).
- `router.rs` — `powershell` elevated hosts writer (~234) and the generic
  runner at ~551 (`certutil` CA trust, router commands). The Unix arms
  (`osascript`, `pkexec`) are untouched.
- Any future spawn must go through the helper — note it in AGENTS.md.

**Implementation.**

```rust
/// Hide the console window Windows would otherwise allocate for a
/// console-subsystem child of our GUI process. No-op on other OSes.
fn no_window(cmd: &mut tokio::process::Command) -> &mut tokio::process::Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}
```

Put it in `docker.rs` (or a tiny `proc.rs` if router reuse gets awkward) and
apply it at every spawn site above. `tokio::process::Command` exposes
`creation_flags` on Windows, so no extra crate. Do **not** use
`CREATE_NEW_CONSOLE`/`DETACHED` — output must stay piped.

**Verify.** Build the MSI (or run a release binary, not `tauri dev` — dev
already inherits the terminal), create a site, trust the CA, toggle local
domains: zero console windows at any point.

## Part 2 — The "Installing WordPress..." hang

Not confirmed a bug yet — first reproduce and instrument, then fix what the
evidence shows. Most likely candidates, in order:

1. **First-run image pull is invisible.** The `wpcli` service
   (`wordpress:cli-php<X>`) is profile-gated, so `compose up -d` does **not**
   pull it. The first `docker compose run --rm wpcli ...` pulls the image
   with stdout/stderr piped — on a fresh machine that's a multi-minute
   download that looks exactly like a hang. (The empty black console in the
   screenshot was this pull running blind.)
2. **DB first boot.** MariaDB/MySQL init on first `up` can take tens of
   seconds; the install retry loop (`wordpress.rs`: 12 attempts × 5 s) hides
   which attempt it's on and what the last error was.
3. **Windows bind-mount slowness.** `./wp-content` on a Windows host mount
   makes WP core extraction noticeably slower than native Linux.

**Fixes (do all three — they're cheap and independent):**

- **Pre-pull with progress.** In `site.rs` `create`, between the `files` and
  `containers` stages (or as part of `containers`), run `docker compose pull`
  **including the wpcli service** (`docker compose pull wpcli` works for
  profile-gated services when named explicitly) and emit a `pulling` stage
  event ("Downloading WordPress images (first run can take a few
  minutes)..."). This turns the blind multi-minute stall into a labeled,
  expected stage.
- **Emit per-attempt progress in the install loop.** `wordpress::install`
  gains an `app: Option<&AppHandle>` (or a closure) and re-emits the
  `install` stage with attempt info ("Installing WordPress... (attempt 3,
  waiting for database)") so the toast visibly changes. On final failure,
  include the last wp-cli stderr in the error event (today only `last_err`
  bubbles up at the end — fine, but make sure the UI actually shows it).
- **Sanity timeout + repro harness.** Time each stage of
  `cargo run --example smoke -- create` on Windows with cold Docker cache
  (`docker rmi` the site images first) to get real numbers; if any stage
  exceeds ~10 min on a warm cache, that's the real bug — chase it with the
  now-visible per-attempt logs. `site::emit` already prints
  `[stage] message` to stderr when there's no app handle, so the smoke
  example is the perfect repro.

## Out of scope

- Streaming live pull percentage into the UI (compose's `--progress json`
  parsing) — the stage label change is enough for v1.
- Retrying/resuming a failed creation (user deletes and recreates — already
  works).

## Definition of done

- No console window ever appears from any LocalKit operation on Windows
  (create, start, stop, CA trust, hosts write, tray actions).
- Site creation on a cold Docker cache shows: files → pulling → containers →
  waiting → install (with attempt counter) → done, and a failure shows the
  actual wp-cli error, never a silent spinner.
- `cargo check` clean; smoke example create/delete cycle passes on Windows.
