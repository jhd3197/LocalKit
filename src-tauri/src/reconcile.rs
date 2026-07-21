//! Status reconciliation + crash recovery (plan 23).
//!
//! Site status is otherwise write-path only — commands set `running`/`stopped`
//! on success — so reality drifts: Docker Desktop restarts, a container is
//! `docker stop`ed from outside, the app is killed mid-create. This module
//! settles the DB's stored status against Docker's ground truth. The rule is
//! **inspect ground truth, settle forward, never guess**: it never downgrades a
//! status a newer command/event set (the `settle_status` compare-and-swap in
//! `db.rs`), and with no ground truth (Docker down) it suspends rather than
//! flap every site to stopped.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tauri::{Emitter, Manager};

use crate::{db::Db, docker, site, AppState};

/// How recently a `running` write must have happened for the reconciler to
/// leave an empty ground truth alone (grace window for slow container starts).
const GRACE_SECS: i64 = 60;

/// Interval between reconcile passes while the app runs.
const TICK_SECS: u64 = 60;

/// What Docker's ground truth says about a site's app service right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Observed {
    /// The app service container is up (and not failing a health check).
    Running,
    /// The app service container is up but restarting or unhealthy.
    Degraded,
    /// No running app service container — stopped, exited, or absent.
    Down,
}

/// Classify a compose project's containers for a given app service. An app
/// service missing from the pass (project absent, or container removed) reads
/// as `Down`.
pub fn classify(containers: &[docker::ContainerInfo], app_service: &str) -> Observed {
    let Some(c) = containers.iter().find(|c| c.service == app_service) else {
        return Observed::Down;
    };
    let state = c.state.to_lowercase();
    let status = c.status.to_lowercase();
    if state == "restarting" || status.contains("unhealthy") {
        Observed::Degraded
    } else if state == "running" {
        // "Up (health: starting)" is still coming up, not yet degraded.
        Observed::Running
    } else {
        // created / exited / paused / dead → not serving.
        Observed::Down
    }
}

/// Decide the status a site should settle to, or `None` to leave it untouched.
///
/// Pure and exhaustively unit-tested — the whole decision table lives here.
/// `recently_started` is true when the site's status was written within the
/// grace window; it suppresses the `running`→`stopped` downgrade so a slow
/// start is not flapped to stopped before its containers finish coming up.
pub fn decide(db_status: &str, observed: Observed, recently_started: bool) -> Option<&'static str> {
    match (db_status, observed) {
        // A create in flight or half-finished — the reconciler never touches
        // `creating`; recovering it is Phase 2's job (incomplete detection).
        ("creating", _) => None,

        // Already agrees with the ground truth: nothing to do.
        ("running", Observed::Running) => None,
        ("stopped", Observed::Down) => None,
        ("degraded", Observed::Degraded) => None,

        // Ground truth is unhealthy → surface `degraded` from any other state.
        (_, Observed::Degraded) => Some("degraded"),

        // External start: the DB says down, a container is up.
        ("stopped", Observed::Running) | ("degraded", Observed::Running) => Some("running"),

        // External stop: the DB says up, no container — but respect the grace
        // window for a running site whose containers are still starting.
        ("running", Observed::Down) if recently_started => None,
        ("running", Observed::Down) | ("degraded", Observed::Down) => Some("stopped"),

        // Any other (unknown/legacy) stored status: settle toward the truth.
        (_, Observed::Running) => Some("running"),
        (_, Observed::Down) => Some("stopped"),
    }
}

/// A status settle that was applied this pass — for logging + the caller's
/// tray refresh / `sites-changed` emit.
#[derive(Debug, Clone)]
pub struct ReconcileEvent {
    pub site_id: String,
    pub slug: String,
    pub from: String,
    pub to: String,
    pub reason: &'static str,
}

fn reason_for(from: &str, to: &str) -> &'static str {
    match (from, to) {
        (_, "degraded") => "unhealthy",
        (_, "running") => "external start",
        ("running", "stopped") | ("degraded", "stopped") => "external stop",
        _ => "settled",
    }
}

/// True when `status_updated_at` (RFC3339) is within the grace window of `now`.
/// An empty/unparseable timestamp is "long ago" → false.
fn recently_started(status_updated_at: &str, now: chrono::DateTime<chrono::Utc>) -> bool {
    match chrono::DateTime::parse_from_rfc3339(status_updated_at) {
        Ok(ts) => (now - ts.with_timezone(&chrono::Utc)).num_seconds() < GRACE_SECS,
        Err(_) => false,
    }
}

/// The compose project name for a site — `localkit-<slug>` for every kind.
fn project_name(slug: &str) -> String {
    format!("localkit-{slug}")
}

/// One reconcile pass: settle every site's stored status against Docker's
/// ground truth, forward-only. Returns the settles that landed. No ground truth
/// (Docker down) → an empty result and zero writes: the reconciler suspends
/// rather than flap every site to stopped when Docker Desktop restarts.
pub async fn reconcile_once(state: &AppState) -> Vec<ReconcileEvent> {
    // 1. Snapshot the sites (short lock, no await held).
    let sites = {
        let Ok(db) = state.db.lock() else {
            return Vec::new();
        };
        db.list_sites().unwrap_or_default()
    };
    if sites.is_empty() {
        return Vec::new();
    }

    // 2. Ground truth in one pass. On error (daemon down) → suspend.
    let truth = match docker::project_container_states().await {
        Ok(t) => t,
        Err(_) => return Vec::new(),
    };

    let now = chrono::Utc::now();
    let empty: Vec<docker::ContainerInfo> = Vec::new();
    let mut events = Vec::new();
    for s in sites {
        // A site with an in-flight lifecycle command owns its own truth right
        // now — its events are authoritative, so an inspect must not race it.
        if state.in_flight.contains(&s.id) {
            continue;
        }
        let containers = truth.get(&project_name(&s.slug)).unwrap_or(&empty);
        let observed = classify(containers, s.app_service());
        let recently = recently_started(&s.status_updated_at, now);
        let Some(target) = decide(&s.status, observed, recently) else {
            continue;
        };
        // Forward-only compare-and-swap: only lands if no command/event wrote a
        // newer status since we read the row.
        let applied = {
            let Ok(db) = state.db.lock() else {
                continue;
            };
            db.settle_status(&s.id, target, &s.status_updated_at)
                .unwrap_or(false)
        };
        if applied {
            let reason = reason_for(&s.status, target);
            eprintln!("reconciled: site {} {}→{} ({reason})", s.slug, s.status, target);
            events.push(ReconcileEvent {
                site_id: s.id,
                slug: s.slug,
                from: s.status,
                to: target.to_string(),
                reason,
            });
        }
    }
    events
}

/// One-time startup backfill (plan 23): mark every already-complete site
/// (running/stopped/degraded, directory present) with the completion marker.
/// This is what keeps a pre-plan-23 site — or a create that crashed after
/// `set_status` but before the marker write — from being mistaken for a
/// half-created one. Run synchronously before the window loads so the first
/// `list_sites` is already honest.
pub fn backfill_markers(db: &Db) {
    for s in db.list_sites().unwrap_or_default() {
        let done = matches!(s.status.as_str(), "running" | "stopped" | "degraded");
        let dir = s.dir();
        if done && dir.exists() && !site::is_complete(&dir) {
            site::mark_complete(&dir);
        }
    }
}

/// Start the background reconcile loop: one pass immediately (so the dashboard
/// opens honest on cold start), then every 60 s. After any pass that settled
/// something, the tray is rebuilt and a `sites-changed` event tells the
/// frontend to re-fetch. Runs for the life of the app.
pub fn spawn_loop(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            let events = {
                let state = app.state::<AppState>();
                reconcile_once(&state).await
            };
            if !events.is_empty() {
                // A settled status must reach the tray menu/tooltip and the
                // dashboard, same as any lifecycle change.
                crate::tray::refresh(&app);
                let _ = app.emit("sites-changed", ());
            }
            tokio::time::sleep(std::time::Duration::from_secs(TICK_SECS)).await;
        }
    });
}

/// Per-site set of in-flight lifecycle commands, shared across every command
/// path (GUI commands, `lk`, tray spawns). The reconciler skips any site in
/// this set. RAII + refcounted: `guard()` returns a handle that removes the id
/// on drop, and nested guards for the same site are safe.
#[derive(Clone, Default)]
pub struct InFlight {
    inner: Arc<Mutex<HashMap<String, u32>>>,
}

impl InFlight {
    /// Mark a site as having an in-flight command for the guard's lifetime.
    pub fn guard(&self, site_id: &str) -> InFlightGuard {
        if let Ok(mut map) = self.inner.lock() {
            *map.entry(site_id.to_string()).or_insert(0) += 1;
        }
        InFlightGuard {
            site_id: site_id.to_string(),
            registry: self.clone(),
        }
    }

    pub fn contains(&self, site_id: &str) -> bool {
        self.inner
            .lock()
            .map(|m| m.contains_key(site_id))
            .unwrap_or(false)
    }
}

/// Handle held by a running command; drops the site from the in-flight set when
/// the last guard for it goes away.
pub struct InFlightGuard {
    site_id: String,
    registry: InFlight,
}

impl Drop for InFlightGuard {
    fn drop(&mut self) {
        if let Ok(mut map) = self.registry.inner.lock() {
            if let Some(n) = map.get_mut(&self.site_id) {
                *n -= 1;
                if *n == 0 {
                    map.remove(&self.site_id);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests — the decision table (every db-status × observation × recency) and the
// container classifier. The forward-only compare-and-swap itself is tested in
// db.rs (`settle_status_is_a_compare_and_swap_on_the_timestamp`).
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn ci(service: &str, state: &str, status: &str) -> docker::ContainerInfo {
        docker::ContainerInfo {
            service: service.into(),
            state: state.into(),
            status: status.into(),
        }
    }

    #[test]
    fn classify_running_healthy_is_running() {
        let cs = [ci("wordpress", "running", "Up 3 minutes (healthy)")];
        assert_eq!(classify(&cs, "wordpress"), Observed::Running);
    }

    #[test]
    fn classify_starting_health_is_still_running_not_degraded() {
        let cs = [ci("wordpress", "running", "Up 2 seconds (health: starting)")];
        assert_eq!(classify(&cs, "wordpress"), Observed::Running);
    }

    #[test]
    fn classify_unhealthy_or_restarting_is_degraded() {
        let unhealthy = [ci("wordpress", "running", "Up 5 minutes (unhealthy)")];
        assert_eq!(classify(&unhealthy, "wordpress"), Observed::Degraded);
        let restarting = [ci("wordpress", "restarting", "Restarting (1) 1 second ago")];
        assert_eq!(classify(&restarting, "wordpress"), Observed::Degraded);
    }

    #[test]
    fn classify_exited_or_absent_is_down() {
        let exited = [ci("wordpress", "exited", "Exited (0) 1 minute ago")];
        assert_eq!(classify(&exited, "wordpress"), Observed::Down);
        // The app service isn't in the pass at all (only its db is up).
        let other = [ci("db", "running", "Up 1 minute")];
        assert_eq!(classify(&other, "wordpress"), Observed::Down);
        assert_eq!(classify(&[], "wordpress"), Observed::Down);
    }

    #[test]
    fn decide_leaves_agreeing_states_alone() {
        assert_eq!(decide("running", Observed::Running, false), None);
        assert_eq!(decide("stopped", Observed::Down, false), None);
        assert_eq!(decide("degraded", Observed::Degraded, false), None);
    }

    #[test]
    fn decide_settles_external_stop_unless_within_grace() {
        // Container vanished and the running write is old → settle to stopped.
        assert_eq!(decide("running", Observed::Down, false), Some("stopped"));
        // …but a just-started site whose containers are still coming up is left
        // alone (the grace window).
        assert_eq!(decide("running", Observed::Down, true), None);
    }

    #[test]
    fn decide_settles_external_start() {
        assert_eq!(decide("stopped", Observed::Running, false), Some("running"));
        // Recency never blocks an upgrade — grace only guards the downgrade.
        assert_eq!(decide("stopped", Observed::Running, true), Some("running"));
    }

    #[test]
    fn decide_surfaces_degraded_from_any_state() {
        assert_eq!(decide("running", Observed::Degraded, false), Some("degraded"));
        assert_eq!(decide("stopped", Observed::Degraded, false), Some("degraded"));
        assert_eq!(decide("degraded", Observed::Degraded, false), None);
    }

    #[test]
    fn decide_recovers_from_degraded() {
        assert_eq!(decide("degraded", Observed::Running, false), Some("running"));
        assert_eq!(decide("degraded", Observed::Down, false), Some("stopped"));
    }

    #[test]
    fn decide_never_touches_a_creating_site() {
        for obs in [Observed::Running, Observed::Degraded, Observed::Down] {
            assert_eq!(decide("creating", obs, false), None);
            assert_eq!(decide("creating", obs, true), None);
        }
    }

    #[test]
    fn decide_settles_an_unknown_status_toward_the_truth() {
        assert_eq!(decide("weird", Observed::Running, false), Some("running"));
        assert_eq!(decide("weird", Observed::Down, false), Some("stopped"));
        assert_eq!(decide("weird", Observed::Degraded, false), Some("degraded"));
    }

    #[test]
    fn recently_started_reads_the_grace_window() {
        let now = chrono::Utc::now();
        let just = (now - chrono::Duration::seconds(5)).to_rfc3339();
        let old = (now - chrono::Duration::seconds(120)).to_rfc3339();
        assert!(recently_started(&just, now));
        assert!(!recently_started(&old, now));
        assert!(!recently_started("", now), "empty timestamp is long ago");
    }

    #[test]
    fn in_flight_guard_refcounts_and_clears_on_drop() {
        let reg = InFlight::default();
        assert!(!reg.contains("s1"));
        let g1 = reg.guard("s1");
        let g2 = reg.guard("s1");
        assert!(reg.contains("s1"));
        drop(g1);
        assert!(reg.contains("s1"), "still held by the second guard");
        drop(g2);
        assert!(!reg.contains("s1"), "cleared when the last guard drops");
    }
}
