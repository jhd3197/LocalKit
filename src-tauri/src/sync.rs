//! Push/pull orchestration between a local site and a ServerKit server (M4),
//! plus importing a remote site as a brand-new local one (plan 18).

use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use tauri::AppHandle;
use uuid::Uuid;

use crate::{docker, router, serverkit, site, snapshot, transfer, wordpress, AppState};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncRecord {
    pub id: String,
    pub site_id: String,
    pub connection_id: String,
    pub direction: String, // "push" | "pull"
    pub kind: String,      // "code" | "db" | "import"
    pub status: String,    // "success" | "error"
    pub message: String,
    pub created_at: String,
}

/// Sync progress goes through the same emitter as the site lifecycle, so that
/// with no Tauri app handle (the `lk` CLI, examples) stages print to stderr
/// instead of vanishing — a multi-minute import must not look like a hang.
fn emit(app: Option<&AppHandle>, id: &str, stage: &str, message: &str) {
    site::emit(app, id, stage, message);
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
        // A user pressing Cancel travels the error path but is not a failure:
        // it gets its own terminal stage and history status so the UI can say
        // "cancelled" in neutral colours instead of flashing a red error.
        Err(e) if transfer::is_cancel(&e) => {
            let message = format!("{direction} {kind} cancelled");
            emit(app, site_id, "cancelled", &message);
            record(
                state,
                &SyncRecord {
                    id: Uuid::new_v4().to_string(),
                    site_id: site_id.to_string(),
                    connection_id: connection_id.to_string(),
                    direction: direction.to_string(),
                    kind: kind.to_string(),
                    status: "cancelled".to_string(),
                    message: message.clone(),
                    created_at: chrono::Utc::now().to_rfc3339(),
                },
            );
            Err(message)
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

// ---------------------------------------------------------------------------
// Protocol selection (plan 19)
// ---------------------------------------------------------------------------

/// Does this server speak the chunked protocol?
///
/// One client, both servers: a server without `sync-v2` gets the v1 monolithic
/// path untouched. A failed probe answers "no" on purpose — falling back to v1
/// always works, so a blip on `/pair` must not fail the push outright.
async fn supports_v2(conn: &serverkit::ServerKitConnection) -> bool {
    serverkit::has_feature(&conn.url, &conn.api_key, serverkit::FEATURE_SYNC_V2)
        .await
        .unwrap_or(false)
}

/// Byte-progress reporter for a transfer stage.
///
/// The event carries raw counters and a bare label; formatting the
/// "148 MB / 312 MB" readout is the frontend's job (and `site::emit_bytes`
/// does it for stderr when there is no frontend).
fn reporter<'a>(
    app: Option<&'a AppHandle>,
    site_id: &'a str,
    stage: &'a str,
    label: &'a str,
) -> impl Fn(u64, u64) + Send + Sync + 'a {
    move |done, total| site::emit_bytes(app, site_id, stage, label, done, total)
}

// ---------------------------------------------------------------------------
// Push code
// ---------------------------------------------------------------------------

pub async fn push_code(
    app: Option<&AppHandle>,
    state: &AppState,
    connection_id: &str,
    site_id: &str,
    remote_site_id: i64,
) -> Result<(), String> {
    let (conn, site) = load(state, connection_id, site_id)?;
    site.require(site.kind == site::KIND_WORDPRESS, "ServerKit push")?;
    let v2 = supports_v2(&conn).await;
    let cancel = state.transfers.begin(site_id);
    run(app, state, connection_id, site_id, "push", "code", async {
        if v2 {
            push_code_v2(app, &conn, &site, site_id, remote_site_id, &cancel).await
        } else {
            push_code_v1(app, &conn, &site, site_id, remote_site_id).await
        }
    })
    .await
}

/// Sync v1: build the whole archive in memory, POST it in one multipart
/// request. Deliberately left as one isolated function rather than a set of
/// `if v2` branches sprinkled through the v2 flow — the two protocols share
/// nothing but their inputs and their success message.
async fn push_code_v1(
    app: Option<&AppHandle>,
    conn: &serverkit::ServerKitConnection,
    site: &site::Site,
    site_id: &str,
    remote_site_id: i64,
) -> Result<String, String> {
    emit(app, site_id, "push", "Bundling wp-content...");
    let tgz = snapshot::build_wp_content_tgz(&site.dir(), &site.config.sync_path)?;
    let size = transfer::human_bytes(tgz.len() as u64);
    emit(app, site_id, "push", &format!("Uploading wp-content ({size})..."));
    serverkit::push_code(&conn.url, &conn.api_key, remote_site_id, tgz).await?;
    Ok(format!("{} code pushed to remote site #{remote_site_id}", site.name))
}

/// Sync v2: tar straight into a staging file, then upload it in chunks.
///
/// The archive never exists as a `Vec<u8>`, which is what makes a site with a
/// real `uploads/` directory pushable at all — and the chunking is what gets
/// it past the server's 100 MB request limit.
async fn push_code_v2(
    app: Option<&AppHandle>,
    conn: &serverkit::ServerKitConnection,
    site: &site::Site,
    site_id: &str,
    remote_site_id: i64,
    cancel: &transfer::CancelToken,
) -> Result<String, String> {
    emit(app, site_id, "push", "Bundling wp-content...");
    let dir = site.dir();
    let staged =
        transfer::stage("wp-content", |w| snapshot::write_wp_content_tgz(&dir, &site.config.sync_path, w))?;
    cancel.check()?;

    let size = transfer::human_bytes(staged.total());
    let progress = reporter(app, site_id, "push", "Pushing wp-content");
    serverkit::push_chunked(
        &conn.url,
        &conn.api_key,
        "code",
        remote_site_id,
        None,
        &staged,
        cancel,
        &progress,
    )
    .await?;
    Ok(format!(
        "{} code pushed to remote site #{remote_site_id} ({size})",
        site.name
    ))
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

// ---------------------------------------------------------------------------
// Push database
// ---------------------------------------------------------------------------

pub async fn push_db(
    app: Option<&AppHandle>,
    state: &AppState,
    connection_id: &str,
    site_id: &str,
    remote_site_id: i64,
) -> Result<(), String> {
    let (conn, site) = load(state, connection_id, site_id)?;
    site.require(site.kind == site::KIND_WORDPRESS, "ServerKit push")?;
    pre_sync_snapshot(app, state, site_id, snapshot::KIND_PRE_PUSH, &conn, remote_site_id).await?;

    // Whatever the site is actually served at — its `.test` domain (with the
    // port in fallback mode) when local domains are on. The server rewrites
    // local -> remote with this, so a hardcoded localhost:<port> would leave
    // `<slug>.test` URLs baked into the remote database.
    let local_url = router::site_public_url(state, &site);
    let v2 = supports_v2(&conn).await;
    let cancel = state.transfers.begin(site_id);
    run(app, state, connection_id, site_id, "push", "db", async {
        let dump = export_dump(app, &site, site_id).await?;
        if v2 {
            let progress = reporter(app, site_id, "push", "Pushing database");
            serverkit::push_chunked(
                &conn.url,
                &conn.api_key,
                "db",
                remote_site_id,
                Some(&local_url),
                &dump,
                &cancel,
                &progress,
            )
            .await?;
        } else {
            emit(app, site_id, "push", "Uploading database dump...");
            let sql = std::fs::read(dump.path()).map_err(|e| format!("failed to read dump: {e}"))?;
            serverkit::push_db(&conn.url, &conn.api_key, remote_site_id, &local_url, sql).await?;
        }
        Ok(format!(
            "{} database pushed to remote site #{remote_site_id} (URLs rewritten to remote)",
            site.name
        ))
    })
    .await
}

/// Export the local database to a self-deleting staged file.
///
/// Staged rather than read into a `Vec` because v2 uploads it chunk by chunk
/// straight off disk; the v1 path reads it back, which is what it did before.
async fn export_dump(
    app: Option<&AppHandle>,
    site: &site::Site,
    site_id: &str,
) -> Result<transfer::Staged, String> {
    emit(app, site_id, "push", "Exporting local database...");
    // A TempFile from the start, so a failed export cleans up after itself
    // instead of leaving a partial dump behind.
    let dump = transfer::TempFile::new(&format!("dump-{}", site.slug))?;
    wordpress::export_db(&site.dir(), dump.path()).await?;
    transfer::Staged::adopt_temp(dump)
}

// ---------------------------------------------------------------------------
// Pull database
// ---------------------------------------------------------------------------

pub async fn pull_db(
    app: Option<&AppHandle>,
    state: &AppState,
    connection_id: &str,
    site_id: &str,
    remote_site_id: i64,
    remote_url: Option<String>,
) -> Result<(), String> {
    let (conn, site) = load(state, connection_id, site_id)?;
    site.require(site.kind == site::KIND_WORDPRESS, "ServerKit pull")?;
    let v2 = supports_v2(&conn).await;

    // v1 snapshots before the download because it has no way to stop one
    // half-way. v2 does: the download is cancellable, so the snapshot is taken
    // after it, once something is actually about to be overwritten — a
    // cancelled pull then leaves no pointless `pre_pull` snapshot behind.
    if !v2 {
        pre_sync_snapshot(app, state, site_id, snapshot::KIND_PRE_PULL, &conn, remote_site_id).await?;
    }

    // Same rule on the way back in: pulling must land the site on its current
    // public URL, not silently knock it off its domain onto localhost.
    let local_url = router::site_public_url(state, &site);
    let cancel = state.transfers.begin(site_id);
    let dir = site.dir();

    run(app, state, connection_id, site_id, "pull", "db", async {
        if v2 {
            emit(app, site_id, "pull", "Downloading remote database dump...");
            let progress = reporter(app, site_id, "pull", "Pulling database");
            let gz = serverkit::download_resumable(
                &conn.url,
                &conn.api_key,
                "/api/v1/localkit/pull/db",
                remote_site_id,
                "database dump",
                &cancel,
                &progress,
            )
            .await?;
            cancel.check()?;
            pre_sync_snapshot(app, state, site_id, snapshot::KIND_PRE_PULL, &conn, remote_site_id)
                .await?;
            emit(app, site_id, "pull", "Importing database into local site...");
            // Streams decompress -> pipe -> `wp db import`; the dump never
            // exists decompressed in memory.
            wordpress::import_db_from_gz(&dir, gz.path()).await?;
        } else {
            emit(app, site_id, "pull", "Downloading remote database dump...");
            let gz = serverkit::pull_db(&conn.url, &conn.api_key, remote_site_id).await?;
            let mut sql = Vec::new();
            flate2::read::GzDecoder::new(&gz[..])
                .read_to_end(&mut sql)
                .map_err(|e| format!("failed to decompress remote dump: {e}"))?;
            emit(app, site_id, "pull", "Importing database into local site...");
            wordpress::import_db(&dir, &sql).await?;
        }

        wordpress::update_site_urls(&dir, &local_url).await?;
        let mut msg = format!("Remote database imported into {}", site.name);
        if let Some(remote) = remote_url.as_deref().filter(|u| !u.is_empty() && *u != local_url) {
            emit(app, site_id, "pull", "Rewriting URLs remote -> local...");
            wordpress::search_replace(&dir, remote, &local_url).await?;
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

// ---------------------------------------------------------------------------
// Import a remote site as a new local site (plan 18)
// ---------------------------------------------------------------------------

/// Safe-extract policy for a downloaded `wp-content` archive — the client-side
/// mirror of the server's `_safe_extract_tar_gz`.
///
/// The archive comes off a remote server we do not fully control, so it is
/// treated as hostile input: an entry may only be a plain file or directory
/// under `wp-content/`. Everything else is refused rather than sanitized,
/// because every "clean it up and carry on" branch is a place a crafted
/// archive could write outside the site directory.
fn safe_entry_path(name: &Path) -> Result<PathBuf, String> {
    let mut out = PathBuf::new();
    for component in name.components() {
        match component {
            // `./foo` — GNU tar emits these; harmless, just drop them.
            Component::CurDir => continue,
            Component::Normal(part) => out.push(part),
            // Absolute paths, `..`, and Windows drive/UNC prefixes all escape
            // the destination directory.
            Component::ParentDir => {
                return Err(format!("archive entry escapes the site directory: {}", name.display()))
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(format!("archive entry has an absolute path: {}", name.display()))
            }
        }
    }
    if out.as_os_str().is_empty() {
        return Err("archive contains an entry with an empty path".into());
    }
    if out.components().next() != Some(Component::Normal("wp-content".as_ref())) {
        return Err(format!(
            "archive entry is outside wp-content: {}",
            name.display()
        ));
    }
    Ok(out)
}

/// Unpack a `wp-content` tar.gz into `site_dir`, applying `safe_entry_path`.
/// Returns the number of files written.
///
/// Entries are prefixed `wp-content/`, matching what `push_code` uploads and
/// what a snapshot archives — one archive format in both directions.
///
/// Takes a reader rather than a byte slice so the import can untar straight
/// off the downloaded file (plan 19): a 4 GB remote `wp-content` should never
/// need 4 GB of RAM to land.
fn extract_wp_content<R: Read>(tgz: R, site_dir: &Path) -> Result<usize, String> {
    use tar::EntryType;

    let dest = site_dir
        .canonicalize()
        .map_err(|e| format!("site directory is unusable: {e}"))?;
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(tgz));
    let entries = archive
        .entries()
        .map_err(|e| format!("the downloaded wp-content archive is unreadable: {e}"))?;

    let mut files = 0usize;
    for entry in entries {
        let mut entry = entry.map_err(|e| format!("the downloaded wp-content archive is unreadable: {e}"))?;
        let path = entry
            .path()
            .map_err(|e| format!("archive entry has an unreadable path: {e}"))?
            .into_owned();
        let rel = safe_entry_path(&path)?;
        let target = dest.join(&rel);

        match entry.header().entry_type() {
            EntryType::Directory => {
                std::fs::create_dir_all(&target)
                    .map_err(|e| format!("failed to create {}: {e}", rel.display()))?;
            }
            EntryType::Regular | EntryType::Continuous => {
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent)
                        .map_err(|e| format!("failed to create {}: {e}", rel.display()))?;
                }
                let mut out = std::fs::File::create(&target)
                    .map_err(|e| format!("failed to write {}: {e}", rel.display()))?;
                std::io::copy(&mut entry, &mut out)
                    .map_err(|e| format!("failed to write {}: {e}", rel.display()))?;
                files += 1;
            }
            // Symlinks and hardlinks are the classic escape hatch: the path
            // check above passes, then the *link target* points anywhere.
            EntryType::Symlink | EntryType::Link => {
                return Err(format!(
                    "archive contains a link, which is not allowed: {}",
                    rel.display()
                ))
            }
            // GNU long-name/PAX metadata entries carry no payload of their own
            // (the tar crate has already applied them to the real entry).
            EntryType::GNULongName | EntryType::GNULongLink | EntryType::XHeader | EntryType::XGlobalHeader => {}
            other => {
                return Err(format!(
                    "archive contains an unsupported entry type ({other:?}): {}",
                    rel.display()
                ))
            }
        }
    }
    if files == 0 {
        return Err("the remote wp-content archive contained no files".into());
    }
    Ok(files)
}

/// Pick the local image version closest to what the remote reports.
///
/// Remote versions carry a patch level (`6.7.2`) that our image allowlist does
/// not, so the match is on `major.minor`. Returns the chosen version and
/// whether it was an exact match — an inexact one is surfaced as a warning
/// rather than an error, because a small version gap almost always still runs.
pub(crate) fn match_version(available: &[&str], remote: Option<&str>) -> (String, bool) {
    let newest = available[0].to_string();
    let Some(remote) = remote.map(str::trim).filter(|v| !v.is_empty()) else {
        return (newest, false);
    };
    let major_minor: String = {
        let mut parts = remote.split('.');
        match (parts.next(), parts.next()) {
            (Some(a), Some(b)) => format!("{a}.{b}"),
            _ => remote.to_string(),
        }
    };
    match available.iter().find(|v| **v == major_minor) {
        Some(v) => (v.to_string(), true),
        None => (newest, false),
    }
}

/// Clone a remote ServerKit site into a brand-new local site.
///
/// Unlike `pull_db`, which overwrites an existing local site, this provisions
/// one: fresh slug, ports and compose project, then the remote `wp-content`
/// and database on top. `wp core install` is deliberately never run — the
/// imported database *is* the site, and installing over it would replace the
/// content the user came for.
pub async fn import_site(
    app: Option<&AppHandle>,
    state: &AppState,
    connection_id: &str,
    remote_site_id: i64,
    local_name: Option<String>,
) -> Result<site::Site, String> {
    let conn = {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        db.get_connection(connection_id)?
    };

    // Everything that can be known before provisioning is checked before
    // provisioning: a failure here must leave no half-built site behind.
    let remote = pre_import(state, &conn, remote_site_id).await?;
    let name = local_name
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| remote.name.clone());

    let (wp_version, wp_exact) = match_version(site::WP_VERSIONS, remote.wp_version.as_deref());
    let (php_version, php_exact) = match_version(site::PHP_VERSIONS, remote.php_version.as_deref());

    let v2 = supports_v2(&conn).await;

    let site = site::reserve(
        state,
        name,
        site::KIND_WORDPRESS.to_string(),
        wp_version.clone(),
        php_version.clone(),
        site::SiteConfig::default(),
        Some((conn.id.clone(), remote_site_id)),
    )
    .await?;

    // The cancel token is keyed on the *new* site's id — that is what the UI
    // shows progress against, so it is what a Cancel button can address.
    let cancel = state.transfers.begin(&site.id);
    // Own this site's status until the import finishes (plan 23).
    let _guard = state.in_flight.guard(&site.id);

    // From here on a failure owns cleanup — the site row and directory exist.
    match do_import(app, state, &conn, &site, &remote, (wp_exact, php_exact), v2, &cancel).await {
        Ok(message) => {
            emit(app, &site.id, "done", &message);
            record(
                state,
                &SyncRecord {
                    id: Uuid::new_v4().to_string(),
                    site_id: site.id.clone(),
                    connection_id: conn.id.clone(),
                    direction: "pull".into(),
                    kind: "import".into(),
                    status: "success".into(),
                    message,
                    created_at: chrono::Utc::now().to_rfc3339(),
                },
            );
            site::get(state, &site.id)
        }
        Err(e) => {
            let cancelled = transfer::is_cancel(&e);
            let message = if cancelled {
                "Import cancelled".to_string()
            } else {
                format!("Import failed: {e}")
            };
            emit(app, &site.id, if cancelled { "cancelled" } else { "error" }, &message);
            // The sync record is keyed to a site that is about to disappear,
            // so the failure is reported through the event stream only.
            let _ = site::cleanup(state, &site).await;
            Err(message)
        }
    }
}

/// Checks that must pass *before* a local site is provisioned: the extension
/// can serve code, the remote site exists and is importable, and we are not
/// about to make a second copy of something already imported.
async fn pre_import(
    state: &AppState,
    conn: &serverkit::ServerKitConnection,
    remote_site_id: i64,
) -> Result<serverkit::RemoteWpSite, String> {
    if !serverkit::has_feature(&conn.url, &conn.api_key, serverkit::FEATURE_PULL_CODE).await? {
        return Err(format!(
            "The serverkit-localkit extension on {} is too old to import sites \
             (no pull/code endpoint). Update the extension on the server.",
            conn.label
        ));
    }

    let sites = serverkit::list_wp_sites(&conn.url, &conn.api_key).await?;
    let remote = sites
        .into_iter()
        .find(|s| s.id == remote_site_id)
        .ok_or_else(|| format!("Remote site #{remote_site_id} was not found on {}.", conn.label))?;

    // A network of sites cannot be represented by one local compose project;
    // refuse rather than produce a half-broken copy.
    if remote.multisite {
        return Err(format!(
            "\"{}\" is a WordPress multisite install, which LocalKit cannot import.",
            remote.name
        ));
    }

    let existing = {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        db.sites_from_remote(&conn.id, remote_site_id)?
    };
    if let Some(s) = existing.first() {
        return Err(format!(
            "\"{}\" was already imported from {} as the local site \"{}\". \
             Pull its database into that site instead of importing a second copy.",
            remote.name, conn.label, s.name
        ));
    }
    Ok(remote)
}

/// The provisioning half of an import. Returns the success message.
#[allow(clippy::too_many_arguments)]
async fn do_import(
    app: Option<&AppHandle>,
    state: &AppState,
    conn: &serverkit::ServerKitConnection,
    site: &site::Site,
    remote: &serverkit::RemoteWpSite,
    exact: (bool, bool),
    v2: bool,
    cancel: &transfer::CancelToken,
) -> Result<String, String> {
    let dir = site.dir();
    let id = site.id.as_str();
    let (wp_exact, php_exact) = exact;

    emit(app, id, "files", "Writing project files...");
    site::write_project_files(site)?;
    if !wp_exact || !php_exact {
        emit(
            app,
            id,
            "files",
            &format!(
                "Remote runs WordPress {} / PHP {}; importing onto WordPress {} / PHP {}.",
                remote.wp_version.as_deref().unwrap_or("unknown"),
                remote.php_version.as_deref().unwrap_or("unknown"),
                site.wp_version,
                site.php_version,
            ),
        );
    }

    emit(
        app,
        id,
        "pulling",
        "Downloading WordPress images (first run can take a few minutes)...",
    );
    docker::compose_pull(&dir, &["wordpress", "db", "wpcli"]).await?;

    emit(app, id, "code", "Downloading remote wp-content...");
    let files = if v2 {
        let progress = reporter(app, id, "code", "Downloading wp-content");
        let tgz = serverkit::download_resumable(
            &conn.url,
            &conn.api_key,
            "/api/v1/localkit/pull/code",
            remote.id,
            "wp-content archive",
            cancel,
            &progress,
        )
        .await?;
        cancel.check()?;
        let size = transfer::human_bytes(tgz.len());
        emit(app, id, "code", &format!("Extracting wp-content ({size})..."));
        let file = std::fs::File::open(tgz.path())
            .map_err(|e| format!("failed to reopen the downloaded archive: {e}"))?;
        extract_wp_content(std::io::BufReader::new(file), &dir)?
    } else {
        let tgz = serverkit::pull_code(&conn.url, &conn.api_key, remote.id).await?;
        let size = transfer::human_bytes(tgz.len() as u64);
        emit(app, id, "code", &format!("Extracting wp-content ({size})..."));
        extract_wp_content(&tgz[..], &dir)?
    };
    // The archive may have brought its own mu-plugins directory over the one
    // written a moment ago; one-click login must survive the import.
    wordpress::ensure_login_plugin(&dir)?;

    emit(app, id, "containers", "Starting Docker containers...");
    docker::compose_up(&dir).await?;

    emit(app, id, "waiting", "Waiting for WordPress to come online...");
    site::wait_for_port(site.port, 180).await?;
    // The port answering is not the same as WordPress being ready — see
    // `wait_for_config`. Without this the first wp-cli call below dies with
    // "'wp-config.php' not found".
    wordpress::wait_for_config(&dir, 24).await?;

    emit(app, id, "install", "Downloading remote database...");
    // No `wp core install` anywhere in here: the imported database IS the
    // site. Installing would overwrite the content this whole flow exists to
    // bring down.
    if v2 {
        let progress = reporter(app, id, "install", "Downloading database");
        let gz = serverkit::download_resumable(
            &conn.url,
            &conn.api_key,
            "/api/v1/localkit/pull/db",
            remote.id,
            "database dump",
            cancel,
            &progress,
        )
        .await?;
        cancel.check()?;
        emit(app, id, "install", "Importing remote database...");
        wordpress::import_db_from_gz(&dir, gz.path()).await?;
    } else {
        let gz = serverkit::pull_db(&conn.url, &conn.api_key, remote.id).await?;
        let mut sql = Vec::new();
        flate2::read::GzDecoder::new(&gz[..])
            .read_to_end(&mut sql)
            .map_err(|e| format!("failed to decompress remote dump: {e}"))?;
        drop(gz);
        emit(app, id, "install", "Importing remote database...");
        wordpress::import_db(&dir, &sql).await?;
    }

    let local_url = router::site_public_url(state, site);
    emit(app, id, "install", "Rewriting URLs remote -> local...");
    wordpress::update_site_urls(&dir, &local_url).await?;
    if let Some(remote_url) = remote.url.as_deref().filter(|u| !u.is_empty() && *u != local_url) {
        wordpress::search_replace(&dir, remote_url, &local_url).await?;
    }
    // Permalinks are stored as rules tied to the old host; regenerate them or
    // every imported page 404s. Best effort — a pretty-permalink failure must
    // not throw away a successful import.
    optional(docker::compose_run(&dir, "wpcli", &["wp", "rewrite", "flush"])).await;
    optional(docker::compose_run(&dir, "wpcli", &["wp", "cache", "flush"])).await;

    // The local admin_user comes from the imported users table — the stock
    // `admin` this site was reserved with does not exist in the remote data.
    let admin_user = imported_admin(&dir).await.unwrap_or_else(|| site.admin_user.clone());
    {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        db.set_status(id, "running")?;
        // No password: the remote's hash is unknown to us, and one-click login
        // does not need one. Storing a fake would be worse than storing none.
        db.update_credentials(id, &admin_user, "")?;
    }
    router::refresh_routes(state).await;
    router::refresh_hosts(state).await;

    Ok(format!(
        "{} imported from {} ({files} files) — now running at {local_url}",
        site.name, conn.label
    ))
}

/// How long an optional post-import wp-cli call may take before it is given up
/// on. These run *after* the site's data is already in place, so hanging on one
/// would throw away a completed import — and `docker compose run` can hang
/// indefinitely if the daemon leaves a container in a bad state (observed with
/// a container Docker reported as "Up" that had no processes left inside).
const OPTIONAL_STEP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

/// Run a best-effort step, discarding both failures and hangs.
async fn optional<T>(op: impl std::future::Future<Output = Result<T, String>>) -> Option<T> {
    tokio::time::timeout(OPTIONAL_STEP_TIMEOUT, op).await.ok()?.ok()
}

/// First administrator in the freshly imported database, for `admin_user`.
/// Optional: falling back to the reserved `admin` is better than failing an
/// import whose data already landed.
async fn imported_admin(dir: &Path) -> Option<String> {
    let out = optional(docker::compose_run(
        dir,
        "wpcli",
        &["wp", "user", "list", "--role=administrator", "--field=user_login"],
    ))
    .await?;
    out.lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(str::to_string)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    // -- version matching ---------------------------------------------------

    #[test]
    fn version_match_ignores_the_remote_patch_level() {
        let (v, exact) = match_version(site::WP_VERSIONS, Some("6.6.4"));
        assert_eq!(v, "6.6");
        assert!(exact);
    }

    #[test]
    fn version_match_falls_back_to_the_newest_and_flags_it() {
        // 6.2 predates the allowlist entirely.
        let (v, exact) = match_version(site::WP_VERSIONS, Some("6.2.1"));
        assert_eq!(v, site::WP_VERSIONS[0]);
        assert!(!exact, "an unavailable remote version must not report as exact");
    }

    #[test]
    fn version_match_handles_a_remote_that_reports_nothing() {
        for missing in [None, Some(""), Some("  ")] {
            let (v, exact) = match_version(site::PHP_VERSIONS, missing);
            assert_eq!(v, site::PHP_VERSIONS[0]);
            assert!(!exact);
        }
    }

    #[test]
    fn version_match_accepts_a_bare_major_minor() {
        let (v, exact) = match_version(site::PHP_VERSIONS, Some("8.1"));
        assert_eq!(v, "8.1");
        assert!(exact);
    }

    // -- path policy --------------------------------------------------------

    #[test]
    fn entry_paths_under_wp_content_are_accepted() {
        let ok = safe_entry_path(Path::new("wp-content/themes/twenty/style.css")).unwrap();
        assert_eq!(ok, PathBuf::from("wp-content/themes/twenty/style.css"));
    }

    #[test]
    fn leading_current_dir_is_stripped() {
        let ok = safe_entry_path(Path::new("./wp-content/plugins/x.php")).unwrap();
        assert_eq!(ok, PathBuf::from("wp-content/plugins/x.php"));
    }

    #[test]
    fn traversal_is_rejected() {
        for evil in [
            "wp-content/../../etc/passwd",
            "wp-content/themes/../../../x",
            "../wp-content/x",
        ] {
            assert!(
                safe_entry_path(Path::new(evil)).is_err(),
                "traversal slipped through: {evil}"
            );
        }
    }

    #[test]
    fn absolute_paths_are_rejected() {
        assert!(safe_entry_path(Path::new("/etc/passwd")).is_err());
        #[cfg(windows)]
        assert!(safe_entry_path(Path::new(r"C:\Windows\system32\evil.dll")).is_err());
    }

    #[test]
    fn entries_outside_wp_content_are_rejected() {
        for evil in ["wp-config.php", "html/wp-content/x", "wp-contents/x"] {
            assert!(
                safe_entry_path(Path::new(evil)).is_err(),
                "entry outside wp-content slipped through: {evil}"
            );
        }
    }

    // -- extraction against real archives -----------------------------------

    /// Raw ustar entry writer.
    ///
    /// `tar::Builder` deliberately refuses to emit `..` paths — which is
    /// precisely the archive this extractor exists to survive. So the hostile
    /// fixtures are assembled header-byte by header-byte instead of through
    /// the safe API, which is the only way to prove the check does anything.
    fn raw_entry(out: &mut Vec<u8>, name: &str, kind: tar::EntryType, link: &str, body: &[u8]) {
        let mut header = tar::Header::new_ustar();
        header.set_size(body.len() as u64);
        header.set_mode(0o644);
        header.set_mtime(0);
        header.set_entry_type(kind);
        {
            // ustar layout: name at 0..100, linkname at 157..257.
            let bytes = header.as_mut_bytes();
            bytes[..name.len()].copy_from_slice(name.as_bytes());
            bytes[157..157 + link.len()].copy_from_slice(link.as_bytes());
        }
        header.set_cksum();
        out.extend_from_slice(header.as_bytes());
        out.extend_from_slice(body);
        out.extend(std::iter::repeat(0u8).take((512 - body.len() % 512) % 512));
    }

    fn gz(build: impl FnOnce(&mut Vec<u8>)) -> Vec<u8> {
        let mut tar_bytes = Vec::new();
        build(&mut tar_bytes);
        // Two zero blocks = end of archive.
        tar_bytes.extend(std::iter::repeat(0u8).take(1024));
        let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        enc.write_all(&tar_bytes).unwrap();
        enc.finish().unwrap()
    }

    fn file_entry(out: &mut Vec<u8>, name: &str, body: &[u8]) {
        raw_entry(out, name, tar::EntryType::Regular, "", body);
    }

    fn link_entry(out: &mut Vec<u8>, name: &str, target: &str) {
        raw_entry(out, name, tar::EntryType::Symlink, target, &[]);
    }

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "localkit-extract-{}-{tag}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn extracts_a_normal_archive() {
        let dir = scratch("ok");
        let tgz = gz(|b| {
            file_entry(b, "wp-content/themes/mytheme/style.css", b"body{}");
            file_entry(b, "wp-content/plugins/hello.php", b"<?php");
        });

        let files = extract_wp_content(&tgz[..], &dir).unwrap();
        assert_eq!(files, 2);
        assert_eq!(
            std::fs::read_to_string(dir.join("wp-content/themes/mytheme/style.css")).unwrap(),
            "body{}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn refuses_an_archive_that_traverses_out() {
        let dir = scratch("traversal");
        let tgz = gz(|b| {
            file_entry(b, "wp-content/ok.txt", b"fine");
            file_entry(b, "wp-content/../../pwned.txt", b"pwned");
        });

        let err = extract_wp_content(&tgz[..], &dir).unwrap_err();
        assert!(err.contains("escapes the site directory"), "unexpected: {err}");
        // And nothing landed outside the destination.
        assert!(!dir.parent().unwrap().join("pwned.txt").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn refuses_an_archive_containing_links() {
        let dir = scratch("symlink");
        let tgz = gz(|b| {
            file_entry(b, "wp-content/ok.txt", b"fine");
            link_entry(b, "wp-content/escape", "/etc/passwd");
        });

        let err = extract_wp_content(&tgz[..], &dir).unwrap_err();
        assert!(err.contains("link"), "unexpected: {err}");
        assert!(!dir.join("wp-content/escape").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn refuses_an_archive_with_nothing_under_wp_content() {
        let dir = scratch("outside");
        let tgz = gz(|b| file_entry(b, "wp-config.php", b"<?php"));

        let err = extract_wp_content(&tgz[..], &dir).unwrap_err();
        assert!(err.contains("outside wp-content"), "unexpected: {err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn refuses_an_empty_archive() {
        let dir = scratch("empty");
        let tgz = gz(|_| {});
        assert!(extract_wp_content(&tgz[..], &dir).unwrap_err().contains("no files"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
