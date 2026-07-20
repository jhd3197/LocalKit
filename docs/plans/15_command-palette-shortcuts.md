# 15 — Command palette + keyboard shortcuts (with remappable bindings)

Status: ✅ shipped

A single command registry feeding a fuzzy command palette, global keyboard
shortcuts, a cheat-sheet, and a **Settings → Keyboard** tab where every
binding is remappable. Ported from Faro (their plan 15), where this exact
stack already runs. Depends on plan 13 (settings store) for persisting
overrides; pairs naturally with plan 12 (command failures toast).

## Motivation

Faro's keyboard system is one of its biggest UX wins: one registry drives
the palette, the shortcuts, the menus and the cheat-sheet, and users can
rebind anything in Settings. LocalKit today has **zero** shortcuts — every
modal hand-rolls its own Escape handler, and navigation is mouse-only.
LocalKit's command surface is small and ideal for this: a handful of global
actions plus per-site commands generated from the sites list (exactly like
Faro's per-profile `Connect:` commands).

Key difference from Faro's plan: they already *had* the registry/dispatcher
and only added remapping. LocalKit starts from nothing, so this plan builds
the substrate first and lands remapping on top — same end state.

## Design

### Phase 1 — Command registry + dispatcher

**`src/lib/commands.tsx`** — single source of truth (Faro:
`src/lib/commands.tsx`). Every command: `{ id, title, group, combo?, context?, run() }`.

- Static commands: `Go to Sites`, `Go to Terminal`, `New site`,
  `Open settings`, `Refresh sites`, `Toggle grid/list view`.
- Dynamic per-site commands (recomputed when `stores/sites` changes):
  `Open <name>`, `Start/Stop <name>`, `WP Admin <name>` (plan 10),
  `Terminal <name>` — grouped under each site.
- Contexts stay simple: `global` (default) and `terminal` (for future
  terminal chords; the registry supports the field from day one so nothing
  refactors later).

**`src/lib/shortcuts.ts`** (port Faro's, ~37 lines): `keyCombo(e)`
canonicalizer, `mod` = Ctrl on Windows/Linux, Cmd on macOS, plus
`comboLabel()` for display (`⌘`/`Ctrl` per platform).

**`src/hooks/useShortcuts.ts`** (port Faro's, ~58 lines): one global
keydown listener matching `keyCombo(e)` against effective bindings.
Critical correctness piece: **editable-target guard** — never fire while
typing in an `input`/`textarea`/`select`/contenteditable/**xterm helper
textarea**, unless the combo has `mod` (mod-combos stay global).

### Phase 2 — Command palette

**`src/components/CommandPalette.tsx`** (port Faro's ~206-line palette +
their 44-line `lib/fuzzy.ts`):

- `mod+K` opens; grouped list, fuzzy filter, ↑/↓/Enter, Esc closes.
- Shows each command's effective binding on the right.
- Mock mode needs nothing (pure frontend over the sites store).

### Phase 3 — Remappable bindings + Settings → Keyboard

- Overrides live in the plan-13 settings store as `shortcut.<commandId>`
  keys (no new backend — `app_settings` KV + generic commands already
  planned there). Effective binding = override ?? default; one resolver
  used by dispatcher, palette, cheat-sheet and the settings UI (Faro's
  "no duplicated maps" rule).
- **Settings → Keyboard section**: grouped, searchable command list with a
  click-to-record capture field per row (click → "press keys…" →
  `keyCombo(next keydown)`; `Esc` cancels, `Backspace` clears to default).
- **Conflict detection**: captured combo already bound in the same context
  → show the collision, offer overwrite/cancel.
- Reset per row + "Reset all to defaults".
- Convert the cheat-sheet (`KeyboardShortcutsDialog`, port Faro's) to read
  effective combos; open it with `?` or from Settings.

### Phase 4 — Modal Escape cleanup

Replace each modal's hand-rolled Escape listener (Settings, NewSiteDialog,
etc.) with a tiny shared `useDialog` hook (Faro `src/hooks/useDialog.ts`:
Escape / outside-click / initial focus in one place).

## Explicitly out

- Multi-key sequences / leader keys (`Ctrl+K Ctrl+S` style) — single combos.
- Remappable terminal chords — plan 14's terminal has no chords yet; the
  context field exists so they slot in later.
- Non-modifier bare-key actions (`F2` rename etc.) — LocalKit has no
  file-browser-like surface that needs them; the dispatcher's guard
  supports them if one appears.

## Implementation notes

- Faro references: `src/lib/commands.tsx`, `src/lib/shortcuts.ts`,
  `src/hooks/useShortcuts.ts`, `src/components/CommandPalette.tsx`,
  `src/lib/fuzzy.ts`, `src/lib/keybindings.ts`,
  `src/components/KeyboardSettings.tsx`,
  `src/components/KeyboardShortcutsDialog.tsx`.
- Default bindings (suggested): `mod+K` palette, `mod+,` settings,
  `mod+N` new site, `mod+1` sites, `mod+2` terminal, `mod+R` refresh,
  `?` cheat-sheet. Keep the list short — everything else is reachable
  through the palette.
- Sequencing: needs plan 13 for Phase 3's persistence; Phases 1–2 work
  with hardcoded defaults if built before 13 lands.

## Definition of done

- `mod+K` palette fuzzy-finds every static + per-site command and runs it.
- Global shortcuts fire from any page, never while typing in inputs or the
  terminal.
- Rebinding a command in Settings → Keyboard persists across restarts,
  shows correctly in palette + cheat-sheet, and conflicts are caught.
- Modals share one Escape/focus implementation.
- `npm run build` clean; all of the above verifiable in mock mode.
