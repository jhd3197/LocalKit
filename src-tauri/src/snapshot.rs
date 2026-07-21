//! Point-in-time site snapshots + one-click restore (plan 17).
//!
//! A snapshot is a directory on disk — no SQLite table, so no migration:
//!
//! ```text
//! <data dir>/snapshots/<site_id>/<id>/
//!   manifest.json      the Snapshot struct below
//!   db.sql.gz          `wp db export -`, gzipped
//!   wp-content.tar.gz  the bind-mounted wp-content dir
//! ```
//!
//! The archive format is deliberately the same one `sync::push_code` uploads
//! (`build_wp_content_tgz` lives here and is shared), so a snapshot is
//! restorable by hand with `tar -xzf` if LocalKit is not around.
//!
//! Snapshots are taken automatically before every destructive operation
//! (push, pull, delete, and restore itself) — see `kind` below.

use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::{Path, PathBuf};
use tauri::AppHandle;

use crate::{docker, site, wordpress, AppState};

/// Manual snapshots are never auto-pruned; every other kind is capped at
/// `RETENTION` per site per kind after each create.
pub const KIND_MANUAL: &str = "manual";
pub const KIND_PRE_PUSH: &str = "pre_push";
pub const KIND_PRE_PULL: &str = "pre_pull";
pub const KIND_PRE_DELETE: &str = "pre_delete";
pub const KIND_PRE_RESTORE: &str = "pre_restore";
/// Transient snapshot a clone takes of its source (plan 20). It exists only to
/// seed the new site and is deleted the moment the clone finishes — an
/// implementation detail, not a snapshot the user asked for.
pub const KIND_CLONE_SOURCE: &str = "clone_source";
/// Transient snapshot `save_blueprint` takes to capture consistent artifacts
/// (plan 20). Like `clone_source`, its bytes are hardlinked into the blueprint
/// and the snapshot itself is deleted — not a user snapshot.
pub const KIND_BLUEPRINT_SOURCE: &str = "blueprint_source";

pub const KINDS: &[&str] = &[
    KIND_MANUAL,
    KIND_PRE_PUSH,
    KIND_PRE_PULL,
    KIND_PRE_DELETE,
    KIND_PRE_RESTORE,
    KIND_CLONE_SOURCE,
    KIND_BLUEPRINT_SOURCE,
];

/// Transient kinds hidden from the user-facing listing: they back the clone
/// and blueprint flows and are deleted the instant they have served their
/// purpose, so surfacing them would only confuse.
fn is_transient(kind: &str) -> bool {
    kind == KIND_CLONE_SOURCE || kind == KIND_BLUEPRINT_SOURCE
}

/// How many auto snapshots to keep per site per kind.
const RETENTION: usize = 5;

const DB_FILE: &str = "db.sql.gz";
const CODE_FILE: &str = "wp-content.tar.gz";
const MANIFEST_FILE: &str = "manifest.json";

/// `manifest.json` — everything the UI/CLI needs without touching the archives.
/// Kept richer than strictly necessary (name/slug/wp_version) so a snapshot
/// stays meaningful after its site row is gone (deleted site).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    /// Sortable timestamp id; also the directory name.
    pub id: String,
    pub site_id: String,
    pub site_name: String,
    pub site_slug: String,
    pub created_at: String,
    /// manual | pre_push | pre_pull | pre_delete | pre_restore
    pub kind: String,
    pub note: String,
    pub db_bytes: u64,
    pub code_bytes: u64,
    pub wp_version: String,
}

// ---------------------------------------------------------------------------
// Layout
// ---------------------------------------------------------------------------

pub fn snapshots_root(data_dir: &Path) -> PathBuf {
    data_dir.join("snapshots")
}

pub fn site_snapshots_dir(data_dir: &Path, site_id: &str) -> PathBuf {
    snapshots_root(data_dir).join(site_id)
}

fn snapshot_dir(data_dir: &Path, site_id: &str, id: &str) -> PathBuf {
    site_snapshots_dir(data_dir, site_id).join(id)
}

/// Timestamp id, filesystem-safe on Windows (no colons).
fn new_id() -> String {
    chrono::Utc::now().format("%Y%m%d-%H%M%S-%3f").to_string()
}

// ---------------------------------------------------------------------------
// Archive helpers (shared with sync::push_code)
// ---------------------------------------------------------------------------

/// Bundle the site's code directory (`sync_path`) as a tar.gz in memory.
/// `sync_path` is the site's `config.sync_path` — `wp-content` for a WP site
/// (plan 22).
pub(crate) fn build_wp_content_tgz(site_dir: &Path, sync_path: &str) -> Result<Vec<u8>, String> {
    let mut buf = Vec::new();
    write_wp_content_tgz(site_dir, sync_path, &mut buf)?;
    Ok(buf)
}

/// Stream the same archive into an arbitrary writer.
///
/// This is the form sync v2 uses (plan 19): the tar/gzip pipeline runs
/// straight into a staging file, so a site with a real `uploads/` directory
/// never has to exist as a `Vec<u8>` first. `build_wp_content_tgz` is now just
/// this with a `Vec` on the end.
///
/// Entries are prefixed with `sync_path`, so an archive is self-describing and
/// `restore_wp_content` unpacks it back into the same relative location.
pub(crate) fn write_wp_content_tgz(
    site_dir: &Path,
    sync_path: &str,
    out: &mut dyn std::io::Write,
) -> Result<(), String> {
    let content = site_dir.join(sync_path);
    if !content.is_dir() {
        return Err(format!("{sync_path} directory not found in the local site"));
    }
    let enc = flate2::write::GzEncoder::new(out, flate2::Compression::fast());
    let mut builder = tar::Builder::new(enc);
    builder
        .append_dir_all(sync_path, &content)
        .map_err(|e| format!("failed to bundle {sync_path}: {e}"))?;
    // Finish both layers explicitly: letting the encoder write its trailer on
    // drop would discard the error, and a truncated gzip only shows up much
    // later as an unreadable archive.
    builder
        .into_inner()
        .map_err(|e| format!("failed to finalize archive: {e}"))?
        .finish()
        .map_err(|e| format!("failed to finalize archive: {e}"))?;
    Ok(())
}

fn gzip(bytes: &[u8]) -> Result<Vec<u8>, String> {
    use std::io::Write;
    let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    enc.write_all(bytes)
        .map_err(|e| format!("failed to compress database dump: {e}"))?;
    enc.finish()
        .map_err(|e| format!("failed to compress database dump: {e}"))
}

fn gunzip(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    flate2::read::GzDecoder::new(bytes)
        .read_to_end(&mut out)
        .map_err(|e| format!("failed to decompress snapshot dump: {e}"))?;
    Ok(out)
}

// ---------------------------------------------------------------------------
// Retention (pure — unit tested)
// ---------------------------------------------------------------------------

/// Ids to prune after a create: for every *auto* kind keep the newest
/// `RETENTION`, drop the rest. `manual` snapshots are never auto-pruned —
/// they are the ones the user deliberately took.
pub fn prunable(snapshots: &[Snapshot]) -> Vec<String> {
    let mut sorted: Vec<&Snapshot> = snapshots.iter().collect();
    // Newest first. `id` is a fixed-width timestamp, so lexical == chronological.
    sorted.sort_by(|a, b| b.id.cmp(&a.id));

    let mut seen: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    let mut out = Vec::new();
    for s in sorted {
        if s.kind == KIND_MANUAL {
            continue;
        }
        let n = seen.entry(s.kind.as_str()).or_insert(0);
        *n += 1;
        if *n > RETENTION {
            out.push(s.id.clone());
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Read
// ---------------------------------------------------------------------------

/// Every snapshot on disk for a site, newest first — including the transient
/// `clone_source` ones. Internal: retention (`prune`) needs to see them to cap
/// any orphaned by a hard crash mid-clone; the user-facing `list` hides them.
/// A directory whose manifest is missing or unreadable is skipped rather than
/// failing the whole listing.
fn list_all(state: &AppState, site_id: &str) -> Result<Vec<Snapshot>, String> {
    let dir = site_snapshots_dir(&state.data_dir, site_id);
    if !dir.is_dir() {
        return Ok(vec![]);
    }
    let entries =
        std::fs::read_dir(&dir).map_err(|e| format!("failed to read snapshots directory: {e}"))?;
    let mut out = Vec::new();
    for entry in entries.flatten() {
        if !entry.path().is_dir() {
            continue;
        }
        if let Ok(text) = std::fs::read_to_string(entry.path().join(MANIFEST_FILE)) {
            if let Ok(snap) = serde_json::from_str::<Snapshot>(&text) {
                out.push(snap);
            }
        }
    }
    out.sort_by(|a, b| b.id.cmp(&a.id));
    Ok(out)
}

/// User-facing snapshot listing, newest first. Hides the transient `*_source`
/// snapshots the clone/blueprint flows use — internal details, not user
/// snapshots.
pub fn list(state: &AppState, site_id: &str) -> Result<Vec<Snapshot>, String> {
    Ok(list_all(state, site_id)?
        .into_iter()
        .filter(|s| !is_transient(&s.kind))
        .collect())
}

fn read_manifest(data_dir: &Path, site_id: &str, id: &str) -> Result<Snapshot, String> {
    let path = snapshot_dir(data_dir, site_id, id).join(MANIFEST_FILE);
    let text = std::fs::read_to_string(&path).map_err(|_| format!("snapshot `{id}` not found"))?;
    serde_json::from_str(&text).map_err(|e| format!("snapshot `{id}` has an unreadable manifest: {e}"))
}

// ---------------------------------------------------------------------------
// Create
// ---------------------------------------------------------------------------

/// Take a snapshot of a site: DB dump + wp-content archive + manifest.
///
/// Emits `snapshot`-stage progress only — never `done`/`error`, because this
/// also runs *inside* longer operations (push/pull/delete) whose own progress
/// toast must not be resolved early. Standalone callers emit the terminal
/// stage themselves.
///
/// Works on a stopped site: `docker compose run wpcli` brings the `db` service
/// up (and waits for its healthcheck) through the compose `depends_on`.
pub async fn create(
    app: Option<&AppHandle>,
    state: &AppState,
    site_id: &str,
    kind: &str,
    note: Option<String>,
) -> Result<Snapshot, String> {
    if !KINDS.contains(&kind) {
        return Err(format!("unknown snapshot kind: {kind}"));
    }
    let s = site::get(state, site_id)?;
    let dir = s.dir();
    if !dir.exists() {
        return Err(format!("site directory not found: {}", dir.display()));
    }

    site::emit(app, site_id, "snapshot", "Exporting database...");
    let sql = export_db(&dir).await?;
    let db_gz = gzip(sql.as_bytes())?;

    site::emit(app, site_id, "snapshot", "Archiving wp-content...");
    let code_tgz = build_wp_content_tgz(&dir, &s.config.sync_path)?;

    let snap = Snapshot {
        id: new_id(),
        site_id: s.id.clone(),
        site_name: s.name.clone(),
        site_slug: s.slug.clone(),
        created_at: chrono::Utc::now().to_rfc3339(),
        kind: kind.to_string(),
        note: note.unwrap_or_default(),
        db_bytes: db_gz.len() as u64,
        code_bytes: code_tgz.len() as u64,
        wp_version: s.wp_version.clone(),
    };

    let out = snapshot_dir(&state.data_dir, site_id, &snap.id);
    std::fs::create_dir_all(&out)
        .map_err(|e| format!("failed to create snapshot directory: {e}"))?;
    // Write the payloads before the manifest: a half-written snapshot has no
    // manifest, so `list` skips it instead of offering a broken restore.
    std::fs::write(out.join(DB_FILE), &db_gz)
        .map_err(|e| format!("failed to write database dump: {e}"))?;
    std::fs::write(out.join(CODE_FILE), &code_tgz)
        .map_err(|e| format!("failed to write wp-content archive: {e}"))?;
    let manifest =
        serde_json::to_string_pretty(&snap).map_err(|e| format!("failed to write manifest: {e}"))?;
    std::fs::write(out.join(MANIFEST_FILE), manifest)
        .map_err(|e| format!("failed to write manifest: {e}"))?;

    prune(state, site_id);
    Ok(snap)
}

/// `wp db export -` with a short retry loop: on a stopped site the first call
/// races the database container's first boot (same reason `wordpress::install`
/// retries).
async fn export_db(dir: &Path) -> Result<String, String> {
    const ATTEMPTS: u32 = 5;
    let mut last = String::new();
    for attempt in 1..=ATTEMPTS {
        match docker::compose_run(dir, "wpcli", &["wp", "db", "export", "-"]).await {
            Ok(sql) if !sql.trim().is_empty() => return Ok(sql),
            Ok(_) => last = "the database export came back empty".into(),
            Err(e) => last = e,
        }
        if attempt < ATTEMPTS {
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        }
    }
    Err(format!("failed to export the database: {last}"))
}

/// Apply the retention policy. Best effort — a failed prune must never fail
/// the snapshot that triggered it.
fn prune(state: &AppState, site_id: &str) {
    let Ok(all) = list_all(state, site_id) else { return };
    for id in prunable(&all) {
        let _ = std::fs::remove_dir_all(snapshot_dir(&state.data_dir, site_id, &id));
    }
}

// ---------------------------------------------------------------------------
// Restore
// ---------------------------------------------------------------------------

/// Roll a site back to a snapshot: DB import + wp-content replace + cache
/// flush. Takes a `pre_restore` snapshot first — restoring is destructive too.
///
/// Returns a user-facing summary (mentions the auto-start when it happened).
pub async fn restore(
    app: Option<&AppHandle>,
    state: &AppState,
    site_id: &str,
    snapshot_id: &str,
) -> Result<String, String> {
    let snap = read_manifest(&state.data_dir, site_id, snapshot_id)?;
    let s = site::get(state, site_id)?;
    let dir = s.dir();
    let src = snapshot_dir(&state.data_dir, site_id, snapshot_id);

    // Read the archives before mutating anything: a corrupt snapshot should
    // fail while the site is still intact.
    let db_gz = std::fs::read(src.join(DB_FILE))
        .map_err(|e| format!("failed to read the snapshot's database dump: {e}"))?;
    let sql = gunzip(&db_gz)?;
    let code_tgz = std::fs::read(src.join(CODE_FILE))
        .map_err(|e| format!("failed to read the snapshot's wp-content archive: {e}"))?;

    site::emit(app, site_id, "restore", "Taking a pre-restore snapshot...");
    create(
        app,
        state,
        site_id,
        KIND_PRE_RESTORE,
        Some(format!("before restoring {}", snap.created_at)),
    )
    .await
    .map_err(|e| format!("pre-restore snapshot failed, nothing was changed: {e}"))?;

    // The DB import needs the stack up; the user asked to go back to this
    // snapshot, so start the site rather than refusing.
    let mut started = false;
    if !is_running(&dir, s.app_service()).await {
        site::emit(app, site_id, "restore", "Starting the site...");
        site::start(state, site_id).await?;
        started = true;
    }

    site::emit(app, site_id, "restore", "Importing database...");
    wordpress::import_db(&dir, &sql).await?;

    site::emit(app, site_id, "restore", "Restoring wp-content...");
    restore_wp_content(&dir, &s.config.sync_path, &code_tgz)?;

    // Object/transient caches can outlive the import; best effort.
    let _ = docker::compose_run(&dir, "wpcli", &["wp", "cache", "flush"]).await;

    let when = &snap.created_at;
    Ok(if started {
        format!("{} restored to the snapshot from {when} (the site was stopped, so it was started)", s.name)
    } else {
        format!("{} restored to the snapshot from {when}", s.name)
    })
}

async fn is_running(dir: &Path, service: &str) -> bool {
    docker::compose_ps(dir)
        .await
        .map(|cs| {
            cs.iter()
                .any(|c| c.service == service && c.state == "running")
        })
        .unwrap_or(false)
}

/// Replace `sync_path` (`wp-content` for a WP site) with the archived copy. The
/// directory itself is kept (it is bind-mounted into the running containers —
/// removing it would break the mount); only its contents are swapped.
fn restore_wp_content(site_dir: &Path, sync_path: &str, tgz: &[u8]) -> Result<(), String> {
    let content = site_dir.join(sync_path);
    if content.is_dir() {
        let entries = std::fs::read_dir(&content)
            .map_err(|e| format!("failed to read {sync_path}: {e}"))?;
        for entry in entries.flatten() {
            let path = entry.path();
            let removed = if path.is_dir() {
                std::fs::remove_dir_all(&path)
            } else {
                std::fs::remove_file(&path)
            };
            removed.map_err(|e| {
                format!("failed to clear {sync_path} ({}): {e}", path.display())
            })?;
        }
    } else {
        std::fs::create_dir_all(&content)
            .map_err(|e| format!("failed to create {sync_path}: {e}"))?;
    }
    // Entries are prefixed with `sync_path`, so unpack at the site root.
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(tgz));
    archive
        .unpack(site_dir)
        .map_err(|e| format!("failed to restore {sync_path}: {e}"))
}

/// Seed a *target* site from a snapshot taken of a *different* site (plan 20).
///
/// The clone flow snapshots a source site, provisions a fresh target, and lays
/// the source's data down onto it here. Distinct from `restore` on three
/// counts: the snapshot lives under another site's id (`source_id`), no
/// `pre_restore` snapshot is taken (the target is brand-new — there is nothing
/// worth preserving), and the source site is never touched. The target must
/// already be running: importing its database needs the stack up.
///
/// The archives are the same format as `restore` reads, so this reuses
/// `restore_wp_content` — the wp-content bytes are ours, not hostile remote
/// input, so the plain contents-swap is the right tool (no safe-extract dance).
pub async fn restore_into(
    state: &AppState,
    source_id: &str,
    snapshot_id: &str,
    target: &site::Site,
) -> Result<(), String> {
    let (db_gz, code_tgz) = artifact_paths(&state.data_dir, source_id, snapshot_id);
    restore_archives_into(&db_gz, &code_tgz, target).await
}

/// Lay a `(db.sql.gz, wp-content.tar.gz)` pair down onto a target site, by
/// path. The shared core of `restore_into` (snapshot dir) and the blueprint
/// create flow (blueprint dir) — same archive format, one implementation. The
/// target must already be running (the DB import needs the stack up).
pub async fn restore_archives_into(
    db_gz: &Path,
    code_tgz: &Path,
    target: &site::Site,
) -> Result<(), String> {
    let db_bytes = std::fs::read(db_gz)
        .map_err(|e| format!("failed to read the database dump: {e}"))?;
    let sql = gunzip(&db_bytes)?;
    let code = std::fs::read(code_tgz)
        .map_err(|e| format!("failed to read the wp-content archive: {e}"))?;

    let dir = target.dir();
    wordpress::import_db(&dir, &sql).await?;
    restore_wp_content(&dir, &target.config.sync_path, &code)?;
    // Object/transient caches from the fresh install can outlive the import.
    let _ = docker::compose_run(&dir, "wpcli", &["wp", "cache", "flush"]).await;
    Ok(())
}

/// Absolute paths of a snapshot's two archive files (DB dump, wp-content).
/// The blueprint flow hardlinks these out of the snapshot dir (plan 20).
pub fn artifact_paths(data_dir: &Path, site_id: &str, snapshot_id: &str) -> (PathBuf, PathBuf) {
    let dir = snapshot_dir(data_dir, site_id, snapshot_id);
    (dir.join(DB_FILE), dir.join(CODE_FILE))
}

// ---------------------------------------------------------------------------
// Delete
// ---------------------------------------------------------------------------

pub fn delete(state: &AppState, site_id: &str, snapshot_id: &str) -> Result<(), String> {
    let dir = snapshot_dir(&state.data_dir, site_id, snapshot_id);
    if !dir.is_dir() {
        return Err(format!("snapshot `{snapshot_id}` not found"));
    }
    std::fs::remove_dir_all(&dir).map_err(|e| format!("failed to delete snapshot: {e}"))
}

/// Drop every snapshot for a site — only ever called when the user explicitly
/// ticks "also delete snapshots" while deleting the site.
pub fn delete_all(data_dir: &Path, site_id: &str) -> Result<(), String> {
    let dir = site_snapshots_dir(data_dir, site_id);
    if !dir.is_dir() {
        return Ok(());
    }
    std::fs::remove_dir_all(&dir).map_err(|e| format!("failed to delete snapshots: {e}"))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(id: &str, kind: &str) -> Snapshot {
        Snapshot {
            id: id.into(),
            site_id: "site-1".into(),
            site_name: "Site One".into(),
            site_slug: "site-one".into(),
            created_at: "2026-07-20T10:00:00Z".into(),
            kind: kind.into(),
            note: String::new(),
            db_bytes: 1024,
            code_bytes: 2048,
            wp_version: "6.7".into(),
        }
    }

    /// Ids sort chronologically, so "newest RETENTION" is a lexical top-N.
    fn auto_run(kind: &str, n: usize) -> Vec<Snapshot> {
        (1..=n)
            .map(|i| snap(&format!("20260720-{i:06}-000"), kind))
            .collect()
    }

    #[test]
    fn keeps_everything_under_the_cap() {
        let all = auto_run(KIND_PRE_PULL, RETENTION);
        assert!(prunable(&all).is_empty());
    }

    #[test]
    fn prunes_oldest_auto_snapshots_beyond_the_cap() {
        let all = auto_run(KIND_PRE_PULL, RETENTION + 3);
        let pruned = prunable(&all);
        assert_eq!(pruned.len(), 3);
        // The three oldest go, the newest RETENTION stay.
        assert!(pruned.contains(&"20260720-000001-000".to_string()));
        assert!(pruned.contains(&"20260720-000003-000".to_string()));
        assert!(!pruned.contains(&"20260720-000004-000".to_string()));
    }

    #[test]
    fn retention_is_per_kind() {
        let mut all = auto_run(KIND_PRE_PULL, RETENTION);
        all.extend(auto_run(KIND_PRE_PUSH, RETENTION));
        // Both kinds are at the cap on their own; neither is over it together.
        assert!(prunable(&all).is_empty());
    }

    #[test]
    fn manual_snapshots_are_never_pruned() {
        let all = auto_run(KIND_MANUAL, RETENTION * 3);
        assert!(prunable(&all).is_empty());
    }

    #[test]
    fn manual_snapshots_do_not_shield_auto_ones() {
        let mut all = auto_run(KIND_MANUAL, 20);
        all.extend(auto_run(KIND_PRE_DELETE, RETENTION + 1));
        assert_eq!(prunable(&all).len(), 1);
    }

    #[test]
    fn manifest_round_trips() {
        let original = snap("20260720-120000-000", KIND_PRE_PUSH);
        let text = serde_json::to_string_pretty(&original).unwrap();
        let back: Snapshot = serde_json::from_str(&text).unwrap();
        assert_eq!(back.id, original.id);
        assert_eq!(back.kind, KIND_PRE_PUSH);
        assert_eq!(back.site_slug, "site-one");
        assert_eq!(back.db_bytes, 1024);
        assert_eq!(back.wp_version, "6.7");
    }

    #[test]
    fn gzip_round_trips() {
        let payload = b"-- MySQL dump\nCREATE TABLE wp_posts;\n";
        let back = gunzip(&gzip(payload).unwrap()).unwrap();
        assert_eq!(back, payload);
    }
}
