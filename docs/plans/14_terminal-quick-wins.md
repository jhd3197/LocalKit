# 14 — Terminal quick wins

Status: ✅ shipped

Four small, independent upgrades to the plan-11 terminal, all ported from
Faro where they've been running in production. Depends on plan 13 only for
the settings-backed pieces (3 and 4 can land first if wanted).

Shipped notes:
- xterm 6 needed two deviations from the Faro originals: `allowProposedApi:
  true` (decorations are still proposed API), and the suggestion echo-check
  pins its marker when the input line STARTS, not at Enter — v6 delivers
  input asynchronously, so at Enter time the shell's echo has often already
  moved the cursor off the command row.
- Accept key is → or End; history lives at `localkit.termHistory.<siteId>`.
- `terminalFontSize` live-applies via a `useSettings.subscribe` in the
  registry; `terminalScrollback` is read at terminal creation only.

## Motivation

Plan 11 shipped the terminal as a deliberately simplified port of Faro's
registry ("no splits/popouts/suggestions"). The following were left on the
table and are each cheap: clickable URLs, copy-on-select, ghost-text
command history, and user-facing terminal settings. wp-cli usage is
repetitive (`wp plugin list`, `wp db export`, `wp search-replace …`), so
history suggestions punch above their weight here.

## 1. WebLinksAddon (tiny)

- Add `@xterm/addon-web-links` and load it in `lib/terminalRegistry.ts`
  (Faro: `terminalRegistry.ts:14,108`). URLs in wp-cli output, logs pasted
  into the shell, etc. become ctrl-clickable and open via the opener
  plugin's default handler.

## 2. Copy-on-select (tiny)

- `term.onSelectionChange` → if the selection is non-empty,
  `navigator.clipboard.writeText(...)` (Faro: `terminalRegistry.ts:195-203`).
- Hardcode ON for v1 (PuTTY-style; it's what devs expect in a local tool).
  If complaints appear, it becomes a settings key via plan 13 — don't build
  the setting preemptively.

## 3. Ghost-text history suggestions (small-medium)

- Port `Faro/src/lib/termSuggest.ts` (258 lines, self-contained, no
  backend): as you type, the most-recent matching command appears as dim
  ghost text; →/End accepts. History is an MRU list in localStorage.
- Key the history per **site** (`localkit.termHistory.<siteId>`) — Faro
  keys per server profile; same idea. Record a command into history when
  Enter is pressed with non-empty input.
- Attach in the registry next to the existing `term.onData` wiring:
  `attachSuggestions(term, { historyKey, send })` shape. Keep it
  frontend-only — the mock build needs nothing.
- Swallow keys only when a suggestion is visible (Faro uses
  `attachCustomKeyEventHandler`; keep that, drop their app-level chord
  integration — LocalKit has no chords yet).

## 4. Terminal settings (small; needs plan 13)

- New Settings → Terminal section with: **font size** (11–16, default 13)
  and **scrollback lines** (1k/5k/10k, default 5k).
- Keys `terminalFontSize` / `terminalScrollback` in the plan-13 settings
  store; the registry reads them at terminal creation and live-applies font
  size via `term.options.fontSize` on change (Faro does exactly this in
  `Terminal.tsx:521-528`). Scrollback applies to newly opened terminals
  only — fine, note it in the UI.
- No terminal theme picker yet — that's tied to a real theme system (Faro
  `TERMINAL_THEMES` can come with it). Document the single hardcoded theme
  as-is.

## Explicitly NOT in this plan

Split panes, popout windows, snippets, remappable chords — each is a real
feature with its own plan when demand shows up (see the Faro port survey;
snippets is the most likely next one).

## Definition of done

- `wp plugin list` output URLs ctrl-click open in the browser.
- Selecting text in the terminal copies it (empty selection doesn't clobber
  the clipboard).
- Typing `wp pl` ghosts a previous `wp plugin list` per site; → accepts;
  history survives reload.
- Settings → Terminal changes font size live in an open terminal; new
  terminals honor scrollback.
- `npm run build` clean; mock mode exercises all four.
