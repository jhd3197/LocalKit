# 23 — Status reconciliation & crash recovery

Status: ⬜ planned

Keep the app's view of the world honest: a reconciler that continuously
settles the DB's site statuses against Docker's ground truth, plus a
recovery path for sites left half-created by a crash or kill mid-install.

## Motivation

Site status today is write-path only: commands set `running`/`stopped` in
SQLite when they succeed. Reality disagrees often — Docker Desktop
restarts, the user kills the app mid-create, containers get `docker stop`ed
from outside, a push dies halfway. The UI then shows "running" sites that
are dead, "stopped" sites that are up, and (worst) a site whose directory
and containers exist but whose WP install never finished, with no offered
path except manual cleanup. The tray menu reads the same DB status, so the
lie propagates everywhere. Rule: **inspect ground truth, settle forward,
never guess.**

## Design

### Phase 1 — Reconciler (`src-tauri/src/reconcile.rs`)

- `reconcile_once(state) -> Vec<ReconcileEvent>`: for each site, compare
  the DB status with `docker::compose_ps` ground truth:
  - DB `running` but no containers → settle to `stopped` (external stop) —
    unless the site's own event stream marked it running in the last 60 s
    (grace window for slow starts).
  - DB `stopped` but wordpress container `Up` → settle to `running`
    (external start).
  - Containers up but wordpress service restarting/unhealthy → `degraded`
    (new status, amber badge — distinct from both running and stopped).
- **Forward-only semantics:** reconciliation may never downgrade a status
  that a *newer* explicit command/event set. Each status write carries a
  `status_updated_at` (new column, **migration 6**); the reconciler only
  wins when its observation is newer. A late success can never be clobbered
  by a stale inspect, and vice versa.
- Scheduling: once at app start (before the window's first data load, so
  the dashboard opens honest), then every 60 s while running, debounced
  against any in-flight lifecycle command per site (a site with an active
  command is skipped — its events own the truth right now).
- After any settle: `tray::refresh(&app)` + a lightweight
  `sites-changed` event so the frontend re-fetches; settles are logged
  (`reconciled: site X running→stopped (external stop)`) at info level.

### Phase 2 — Half-created site recovery

- Marker: `site.rs` writes `.localkit-install-complete` (empty file) in the
  site dir as the last create step. On reconcile, a site record whose dir
  lacks the marker is flagged `incomplete`.
- UI: incomplete sites render with an amber "Setup incomplete" badge and a
  choice dialog — **Resume setup** (re-run from the install stage:
  containers exist, images pulled, so it's the wait + `wp core install`
  tail of the create flow) or **Clean up** (delete path, which already
  tolerates partial state).
- Same guard covers killed installs behind the "waiting for database"
  stage: resume re-enters the existing wait loop rather than assuming
  health.

### Phase 3 — Docker daemon health

- `docker::check` result cached for 30 s and exposed via `app_info`; when
  the daemon drops, the sidebar shows a global "Docker unavailable" pill,
  lifecycle commands short-circuit with the existing `friendly_error`, and
  the reconciler suspends itself (no ground truth = no settles, definitely
  no mass "stopped" flapping when Docker Desktop restarts).

## Risks

- `compose_ps` per site every 60 s is N subprocesses; batch by running one
  `docker ps` filter pass and matching compose project names locally. Keep
  `no_window` discipline.
- Race with in-flight creates: the per-site in-flight set must be shared
  with *all* command paths (GUI commands, `lk`, tray spawns) — a
  `DashSet<String>` on `AppState` checked by the reconciler.
- `degraded` is a new status value — touchpoints: `StatusBadge`, dashboard
  filters, tray menu labels, `lk list` output, mock data.

## Verification

- `cargo test --lib reconcile`: decision-table unit tests over a stubbed
  compose-ps (every DB-status × container-state × recency combination,
  including the forward-only guard).
- Manual: `docker stop` a site's container externally → within 60 s the UI
  and tray show stopped; kill the app mid-create → relaunch → "Setup
  incomplete" → Resume finishes the install.
