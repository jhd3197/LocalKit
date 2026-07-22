# 27 — Visual identity pass: site tiles, action icons, micro-polish

Status: ✅ shipped

Frontend-only. No Rust, no migrations. Two icon dependencies, per the
maintainer's direction (the Faro pattern): `lucide-react` for UI chrome and
`@iconify/react/offline` + a committed, generated subset of
`@iconify-json/simple-icons` for real brand marks — zero icon network calls.

## Motivation

The UI is structurally complete but visually anonymous: every site is a block
of text, every action is a gray text pill, and the sidebar is two text links.
Competing tools (LocalWP, Herd, DevKinsta) feel "alive" mostly through one
device — each site is a recognizable *thing* with a face and a glanceable
running state. LocalKit has the information architecture; this pass gives it
the identity layer.

## Design

The existing token system (navy-tinted zinc scale, violet `#6C5CE7`, Inter +
JetBrains Mono, radius scale) is good and stays untouched. This is a pass
*within* the system, and the boldness is spent in exactly one place:

**Signature: deterministic monogram tiles.** `SiteTile` renders a rounded
square with the site's initials, tinted by a hue hashed from the site *slug*
(FNV-1a → 0–360). The color is therefore stable across grid, list, detail,
restarts and machines — a site's tile is its face. The tile doubles as a
status light:

- **up** (running/degraded): saturated gradient + soft same-hue glow
- **down** (stopped/error): same hue, desaturated and dim — identity
  persists while the site sleeps
- **creating**: dim + pulse

Everything else stays quiet and disciplined:

- **Icons on actions.** `components/icons.tsx` becomes thin lucide-react
  wrappers keeping the old names and `h-4 w-4` default (the hand-rolled set
  was feather-derived, so nothing shifts visually): play, stop,
  arrow-up-right (open), wrench (details), duplicate (clone), trash,
  bookmark (blueprint), camera (snapshots), database, key, file-text (logs),
  sync, search, bug, layers (sites nav). Buttons keep their labels — icons
  lead, text stays for discoverability.
- **Real brand marks.** `scripts/gen-brand-icons.mjs` (Faro plan-14 pattern)
  bakes a curated simple-icons subset (wordpress, docker, php, laravel,
  mariadb) into `src/lib/brandIconData.ts`; `lib/brandIcons.tsx` renders
  them offline. `KindBadge` shows the actual logo — and gains a distinct
  PHP variant (it previously mislabeled `php` sites as "WP").
- **Hero action.** "Open" on an up site becomes a violet-tinted button —
  the one thing you most likely came to do.
- **Section headers** on SiteDetail + the Tools/Snapshots/Sync panels get a
  small leading icon via a shared `SectionTitle`.
- **Sidebar**: layers icon on Sites plus a live running-count pill;
  Terminal keeps its icon.
- **Empty state**: three overlapping tiles (one violet, glowing) as a pure-CSS
  illustration + invitation copy.
- **Micro-motion**: card hover lift + border tint, transitions on buttons.
  All transform motion gated behind `motion-reduce`.

Out of scope, deliberately: Settings modal headers (secondary surface), light
theme, any palette/typography change, dialogs.

## Phases

1. `SiteTile` + `SectionTitle` + icon set additions.
2. Dashboard (grid cards, list rows, toolbar, empty state) + Sidebar.
3. SiteDetail header + section headers; Snapshots/Push/Tools panel headers.
4. Regenerate `docs/screenshots/` via `npm run shots`; verify `npm run build`
   + `npm run test`; mark shipped.

## Verification

- `npm run build`, `npm run test` green; `cargo check` untouched by design.
- `npm run shots` — visual review of dashboard, dashboard-list, site-detail,
  site-tools, snapshots captures; screenshots are the artifact of record.
- Screenshot script contract unchanged: it clicks by `textContent.trim()`
  equality — icons are `aria-hidden` SVGs and contribute no text, so
  `clickText`/`shotElement` selectors keep matching.
