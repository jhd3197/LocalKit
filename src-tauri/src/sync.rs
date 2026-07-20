//! Push/pull orchestration between a local site and a ServerKit server (M4).

use serde::{Deserialize, Serialize};
use std::io::Read;
use tauri::{AppHandle, Emitter};
use uuid::Uuid;

use crate::{router, serverkit, site, snapshot, wordpress, AppState};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncRecord {
    pub id: String,
    pub site_id: String,
    pub connection_id: String,
    pub direction: String, // "push" | "pull"
    pub kind: String,      // "code" | "db"
    pub status: String,    // "success" | "error"
    pub message: String,
    pub created_at: String,
}

fn emit(app: Option<&AppHandle>, id: &str, stage: &str, message: &str) {
    if let Some(app) = app {
        let _ = app.emit(
            "site-event",
            site::SiteEvent {
                id: id.to_string(),
                stage: stage.to_string(),
                message: message.to_string(),
            },
        );
    }
}

fn record(state: &AppState, rec: &SyncRecord) {
    if let Ok(db) = state.db.lock() {
        let _ = db.insert_sync(rec);
    }
}

fn load(state: &AppState, connection_id: &str, site_id: &str) -> Result<(serverkit::ServerKitConnection, site::Site), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let conn = db.get_connection(connection_id)?;
    let site = db.get_site(site_id)?;
    Ok((conn, site))
}

async fn run(
    app: Option<&AppHandle>,
    state: &AppState,
    connection_id: &str,
    site_id: &str,
    direction: &str,
    kind: &str,
    op: impl std::future::Future<Output = Result<String, String>>,
) -> Result<(), String> {
    match op.await {
        Ok(message) => {
            emit(app, site_id, "done", &message);
            record(
                state,
                &SyncRecord {
                    id: Uuid::new_v4().to_string(),
                    site_id: site_id.to_string(),
                    connection_id: connection_id.to_string(),
                    direction: direction.to_string(),
                    kind: kind.to_string(),
                    status: "success".to_string(),
                    message,
                    created_at: chrono::Utc::now().to_rfc3339(),
                },
            );
            Ok(())
        }
        Err(e) => {
            emit(app, site_id, "error", &format!("{direction} {kind} failed: {e}"));
            record(
                state,
                &SyncRecord {
                    id: Uuid::new_v4().to_string(),
                    site_id: site_id.to_string(),
                    connection_id: connection_id.to_string(),
                    direction: direction.to_string(),
                    kind: kind.to_string(),
                    status: "error".to_string(),
                    message: e.clone(),
                    created_at: chrono::Utc::now().to_rfc3339(),
                },
            );
            Err(e)
        }
    }
}

pub async fn push_code(
    app: Option<&AppHandle>,
    state: &AppState,
    connection_id: &str,
    site_id: &str,
    remote_site_id: i64,
) -> Result<(), String> {
    let (conn, site) = load(state, connection_id, site_id)?;
    emit(app, site_id, "push", "Bundling wp-content...");
    let tgz = snapshot::build_wp_content_tgz(&site.dir())?;
    let size_mb = tgz.len() as f64 / 1_048_576.0;
    emit(app, site_id, "push", &format!("Uploading wp-content ({size_mb:.1} MB)..."));
    run(app, state, connection_id, site_id, "push", "code", async move {
        serverkit::push_code(&conn.url, &conn.api_key, remote_site_id, tgz).await?;
        Ok(format!("{} code pushed to remote site #{remote_site_id}", site.name))
    })
    .await
}

/// Local safety net before a sync mutates something (plan 17).
///
/// A failure here aborts the sync: never mutate without a net. The note
/// records which connection/remote the operation was aimed at, so the
/// snapshot list reads as a history of "what did I sync, and against what".
async fn pre_sync_snapshot(
    app: Option<&AppHandle>,
    state: &AppState,
    site_id: &str,
    kind: &str,
    conn: &serverkit::ServerKitConnection,
    remote_site_id: i64,
) -> Result<(), String> {
    let note = format!("{} (#{remote_site_id} on {})", conn.label, conn.url);
    snapshot::create(app, state, site_id, kind, Some(note))
        .await
        .map(|_| ())
        .map_err(|e| format!("pre-sync snapshot failed, nothing was synced: {e}"))
}

pub async fn push_db(
    app: Option<&AppHandle>,
    state: &AppState,
    connection_id: &str,
    site_id: &str,
    remote_site_id: i64,
) -> Result<(), String> {
    let (conn, site) = load(state, connection_id, site_id)?;
    pre_sync_snapshot(app, state, site_id, snapshot::KIND_PRE_PUSH, &conn, remote_site_id).await?;
    emit(app, site_id, "push", "Exporting local database...");
    let dump_path = std::env::temp_dir().join(format!("localkit-dump-{}.sql", site.slug));
    wordpress::export_db(&site.dir(), &dump_path).await?;
    let sql = std::fs::read(&dump_path).map_err(|e| format!("failed to read dump: {e}"));
    let _ = std::fs::remove_file(&dump_path);
    let sql = sql?;

    // Whatever the site is actually served at — its `.test` domain (with the
    // port in fallback mode) when local domains are on. The server rewrites
    // local -> remote with this, so a hardcoded localhost:<port> would leave
    // `<slug>.test` URLs baked into the remote database.
    let local_url = router::site_public_url(state, &site);
    emit(app, site_id, "push", "Uploading database dump...");
    run(app, state, connection_id, site_id, "push", "db", async move {
        serverkit::push_db(&conn.url, &conn.api_key, remote_site_id, &local_url, sql).await?;
        Ok(format!(
            "{} database pushed to remote site #{remote_site_id} (URLs rewritten to remote)",
            site.name
        ))
    })
    .await
}

pub async fn pull_db(
    app: Option<&AppHandle>,
    state: &AppState,
    connection_id: &str,
    site_id: &str,
    remote_site_id: i64,
    remote_url: Option<String>,
) -> Result<(), String> {
    let (conn, site) = load(state, connection_id, site_id)?;
    pre_sync_snapshot(app, state, site_id, snapshot::KIND_PRE_PULL, &conn, remote_site_id).await?;
    emit(app, site_id, "pull", "Downloading remote database dump...");
    let gz = serverkit::pull_db(&conn.url, &conn.api_key, remote_site_id).await?;

    // Decompress the .sql.gz dump.
    let mut sql = Vec::new();
    flate2::read::GzDecoder::new(&gz[..])
        .read_to_end(&mut sql)
        .map_err(|e| format!("failed to decompress remote dump: {e}"))?;

    // Same rule on the way back in: pulling must land the site on its current
    // public URL, not silently knock it off its domain onto localhost.
    let local_url = router::site_public_url(state, &site);
    emit(app, site_id, "pull", "Importing database into local site...");
    run(app, state, connection_id, site_id, "pull", "db", async move {
        wordpress::import_db(&site.dir(), &sql).await?;
        wordpress::update_site_urls(&site.dir(), &local_url).await?;
        let mut msg = format!("Remote database imported into {}", site.name);
        if let Some(remote) = remote_url.filter(|u| !u.is_empty() && *u != local_url) {
            emit(app, site_id, "pull", "Rewriting URLs remote -> local...");
            wordpress::search_replace(&site.dir(), &remote, &local_url).await?;
            msg = format!("{msg} (URLs rewritten to local)");
        }
        Ok(msg)
    })
    .await
}

pub fn history(state: &AppState, site_id: &str) -> Result<Vec<SyncRecord>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.list_sync(site_id, 20)
}
