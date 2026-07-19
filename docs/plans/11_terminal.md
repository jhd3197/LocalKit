# 11 — Per-site embedded terminals

Status: ✅ shipped

A **Terminal** page with one tab per site: a real interactive shell inside
the site's WordPress container, embedded in the app via xterm.js. Site detail
gets a **Terminal** button that jumps straight to that site's tab.

## Motivation

The most common escape hatch in a local-WP tool is "give me a shell":
`wp plugin list`, editing a `wp-config.php` constant, inspecting the
database, tailing logs with grep. Today that means leaving the app and
typing `docker compose exec …` by hand. Faro already proved the pattern
(xterm.js frontend + a PTY manager in Rust streaming over Tauri events), so
this ports that architecture to local Docker containers.

## Design

**Backend — `src-tauri/src/terminal.rs` (`PtyManager` on `AppState.terminals`).**
Modeled on Faro's SSH `PtyManager`, but each session is a local PTY from
`portable-pty` (ConPTY on Windows, openpty elsewhere) running:

```
docker compose exec wordpress bash      # current_dir = site dir
```

- `terminal_open(site_id, cols, rows)` → terminal id. First checks
  `docker::compose_ps` that the `wordpress` container is running, so a
  stopped site gets a clean "start the site first" error instead of a dead
  shell.
- `terminal_write` / `terminal_resize` / `terminal_close` keyed by the
  terminal id.
- Output streams on `terminal://data`, exit on `terminal://exit` — same
  event names and payload shapes as Faro (`{terminalId, data}` /
  `{terminalId, code}`).
- Reader/wait live on plain std threads; the slave side is dropped right
  after spawn so the reader sees EOF when the shell exits.

**Frontend — xterm instances live outside React** (`src/lib/terminalRegistry.ts`,
simplified from Faro's: no splits/popouts/suggestions). Keyed by site id;
pages only `attach`/`detach` the cached DOM node, so scrollback and the live
PTY survive tab switches, page navigation, and HMR. Disposal is explicit
(`restartTerminal` after a session ends), never a React-unmount side effect.

**`src/pages/Terminal.tsx`.** One tab per site with a live status dot
(emerald = running). Stopped sites show a "Start site" prompt instead of an
error; dead sessions get a "Reconnect" bar. Opened terminals stay mounted —
only the active one is visible.

**Entry points.** Sidebar **Terminal** nav item (page
`{ name: "terminal", siteId? }` in the nav store) and a **Terminal** button
in the SiteDetail header actions.

**Mock mode.** `mock/core.ts` answers the four commands with a fake
line-echo shell (`mockShells` map) so `npm run dev:mock` shows a working
terminal. Gotcha: `terminal_resize` must be a no-op there — xterm's
FitAddon fires a resize immediately after open.

## Conventions

- Any `AppState` constructor (GUI `run()`, `lk` CLI, the smoke examples)
  must pass `terminal::PtyManager::new()`.
- The terminal always shells into the `wordpress` service with bash; the
  stock `wordpress` image ships bash, so no fallback is needed.
- Theme: `#08090E` background, JetBrains Mono, brand-violet cursor — kept in
  the registry, not in Settings (no terminal settings exist yet).

## Verification

- `cargo check --workspace --all-targets` clean.
- `npm run build` (tsc + vite) clean.
- Headless Chrome against `vite --mode mock`: tabs render for every site,
  typing echoes, Enter gets a mock response, tab switching preserves state.
