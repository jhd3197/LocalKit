# 28 — Control room: home dashboard, light theme, live rail, stamped tiles

Status: ⬜ planned

Frontend-only. No Rust, no migrations, no new dependencies. Builds directly
on plan 27's tiles and icon layer, and on the maintainer's direction:
LocalKit should be *the screen you walk to the PC to look at* — a control
room, not a settings page.

## Motivation

After plan 27 the pieces are good but the architecture still reads flat:

- The nav is two text rows (Sites, Terminal) — weak, and it wastes the one
  place a glanceable "what's running" board could live.
- There is no home. The sites list doubles as a landing page, so the first
  screen is management chrome, not status.
- Kind marks are tiny pills next to the name; the maintainer wants them
  *stamped on the tile* like Faro stamps a protocol on a server bubble.
- Labeled buttons ("Open", "Stop") spend space that icons + tooltips cover.
- One theme. The maintainer wants light — and the ROADMAP has always noted
  light needs a CSS-var token layer first. This plan builds that layer.

## Design

- **Token layer + light theme.** The Tailwind zinc/white ramps become CSS
  variables (`rgb(var(--c-zinc-900) / <alpha>)`) with dark defaults on
  `:root` and a light ramp on `[data-theme="light"]`. Violet is shared.
  A `theme` setting (default dark) with a Settings → General toggle;
  `main.tsx` stamps `data-theme` pre-render, App keeps it in sync.
  SiteTile's dimmed state reads lightness from vars so tiles work on both.
- **Home dashboard** (new default page): a status headline ("3 of 8 sites
  running"), a tile wall of every site (glow = running, click = detail),
  an attention list (degraded / error / half-created), and an environment
  card (Docker, local domains router, ServerKit connections, blueprints).
  All from existing stores — no new IPC.
- **Live rail.** The sidebar becomes a collapsible control rail: Home /
  Sites / Terminal, then every site as a tile row with a status dot —
  the active site expands sub-links (Overview, Tools, Snapshots, Logs).
  Collapse state persists in the settings KV (`railCollapsed`).
- **Stamped tiles.** SiteTile gains a corner chip with the real brand mark
  (WordPress / Docker / PHP via plan 27's offline set) — the Faro pattern.
  Kind pills leave the cards and list rows (the stamp carries it);
  KindBadge remains on the site detail header only.
- **Icon-only actions** on cards and list rows (tooltips + aria-labels
  carry the words). "Resume setup" keeps its label — recovery must read.
- **Blueprint grid background** on the content pane — ServerKit's square
  grid (1px lines each 40px, accent-tinted, ~5% alpha), theme-aware.
- **Detail tabs.** Overview | Tools | Snapshots | Logs — Snapshots and
  Logs move out of the overview scroll. The tab lives in the nav store
  (`page.tab`) so the rail can deep-link to it.

## Phases

1. Token layer + light theme + Settings toggle.
2. SiteTile stamps; dashboard cards/list go icon-only; pills out.
3. Rail v2 (collapse + live site list + sub-nav).
4. Home page + default-route change.
5. Detail tabs + blueprint grid background.
6. Capture script: light theme for all captures + a `home.png` shot +
   tab-aware snapshot capture; regenerate; verify; mark shipped.

## Verification

- `npm run build`, `npm run test` green.
- `npm run shots` (light theme) — visual review of every capture.
- Dark theme spot-checked by toggling the setting in the mock UI.
