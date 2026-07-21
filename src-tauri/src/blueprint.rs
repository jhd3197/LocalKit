//! Reusable site blueprints (plan 20 phase 2).
//!
//! A blueprint is a *directory* on disk — no SQLite table, so no migration:
//!
//! ```text
//! <data dir>/blueprints/<slug>/
//!   blueprint.json     the Manifest below (the recipe + display metadata)
//!   db.sql.gz          `wp db export -`, gzipped
//!   wp-content.tar.gz  the site's wp-content dir
//! ```
//!
//! The two archives are the same format the snapshot engine writes, so a
//! blueprint is really "a snapshot you can stamp new sites out of". `save`
//! captures a site's current state (snapshotting it, then hardlinking the
//! snapshot's artifacts across so the bytes aren't duplicated) plus its plugin
//! and theme list as display-only metadata; `create_site` provisions a fresh
//! site and lays the recipe down, exactly like the clone flow.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::AppHandle;

use crate::{docker, router, site, snapshot, wordpress, AppState};

const MANIFEST_FILE: &str = "blueprint.json";
const DB_FILE: &str = "db.sql.gz";
const CODE_FILE: &str = "wp-content.tar.gz";

/// A plugin captured at save time — display metadata only. v1 does not
/// re-resolve or re-install these; they are shown so a blueprint's contents
/// are legible before you create a site from it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlueprintPlugin {
    pub name: String,
    pub status: String,
    pub version: String,
}

/// `blueprint.json` — the recipe. No id or byte sizes: the id is the directory
/// name and the sizes are read off the files, so neither is duplicated here.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub name: String,
    pub description: String,
    pub wp_version: String,
    pub php_version: String,
    pub plugins: Vec<BlueprintPlugin>,
    pub theme: String,
    pub created_at: String,
    pub source_site_name: String,
}

/// What the UI/CLI sees: the recipe plus the derived id and on-disk sizes.
#[derive(Debug, Clone, Serialize)]
pub struct Blueprint {
    /// Directory slug — the stable id used to create-from / delete / export.
    pub id: String,
    #[serde(flatten)]
    pub manifest: Manifest,
    pub db_bytes: u64,
    pub code_bytes: u64,
}

// ---------------------------------------------------------------------------
// Layout
// ---------------------------------------------------------------------------

pub fn blueprints_root(data_dir: &Path) -> PathBuf {
    data_dir.join("blueprints")
}

fn blueprint_dir(data_dir: &Path, id: &str) -> PathBuf {
    blueprints_root(data_dir).join(id)
}

/// First free `<base>`, `<base>-2`, ... for which `exists` is false. Pure so
/// the uniqueness rule is unit-testable without touching the filesystem.
fn pick_slug(base: &str, exists: impl Fn(&str) -> bool) -> String {
    if !exists(base) {
        return base.to_string();
    }
    for i in 2..1000 {
        let candidate = format!("{base}-{i}");
        if !exists(&candidate) {
            return candidate;
        }
    }
    format!("{base}-{}", 1000)
}

/// A blueprint slug unique among the blueprints already on disk.
fn unique_slug(data_dir: &Path, name: &str) -> String {
    let base = site::slugify(name);
    pick_slug(&base, |slug| blueprint_dir(data_dir, slug).is_dir())
}

// ---------------------------------------------------------------------------
// Hardlink-or-copy (pure enough to unit test)
// ---------------------------------------------------------------------------

/// Place `src` at `dst`, hardlinking when the filesystem allows (blueprints and
/// snapshots both live under the LocalKit data dir, so this is the norm) and
/// falling back to a byte copy otherwise. Hardlinking is what keeps a blueprint
/// from duplicating the snapshot's bytes — a wp-content archive can be hundreds
/// of megabytes.
pub fn hardlink_or_copy(src: &Path, dst: &Path) -> Result<(), String> {
    if dst.exists() {
        let _ = std::fs::remove_file(dst);
    }
    if std::fs::hard_link(src, dst).is_ok() {
        return Ok(());
    }
    copy_file(src, dst)
}

fn copy_file(src: &Path, dst: &Path) -> Result<(), String> {
    std::fs::copy(src, dst)
        .map(|_| ())
        .map_err(|e| format!("failed to copy blueprint artifact: {e}"))
}

// ---------------------------------------------------------------------------
// Read
// ---------------------------------------------------------------------------

fn file_len(path: &Path) -> u64 {
    std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

fn read_blueprint(data_dir: &Path, id: &str) -> Result<Blueprint, String> {
    let dir = blueprint_dir(data_dir, id);
    let text = std::fs::read_to_string(dir.join(MANIFEST_FILE))
        .map_err(|_| format!("blueprint `{id}` not found"))?;
    let manifest: Manifest = serde_json::from_str(&text)
        .map_err(|e| format!("blueprint `{id}` has an unreadable manifest: {e}"))?;
    Ok(Blueprint {
        id: id.to_string(),
        db_bytes: file_len(&dir.join(DB_FILE)),
        code_bytes: file_len(&dir.join(CODE_FILE)),
        manifest,
    })
}

/// All blueprints, newest first. A directory whose manifest is missing or
/// unreadable is skipped rather than failing the whole listing.
pub fn list(state: &AppState) -> Result<Vec<Blueprint>, String> {
    let root = blueprints_root(&state.data_dir);
    if !root.is_dir() {
        return Ok(vec![]);
    }
    let entries =
        std::fs::read_dir(&root).map_err(|e| format!("failed to read blueprints directory: {e}"))?;
    let mut out = Vec::new();
    for entry in entries.flatten() {
        if !entry.path().is_dir() {
            continue;
        }
        if let Some(name) = entry.file_name().to_str() {
            if let Ok(bp) = read_blueprint(&state.data_dir, name) {
                out.push(bp);
            }
        }
    }
    out.sort_by(|a, b| b.manifest.created_at.cmp(&a.manifest.created_at));
    Ok(out)
}

/// Resolve a blueprint by exact id (slug), then case-insensitive name — the
/// same shape as site resolution, for the CLI. Ambiguous names ask for the id.
pub fn find(state: &AppState, query: &str) -> Result<Blueprint, String> {
    let all = list(state)?;
    if let Some(bp) = all.iter().find(|b| b.id == query) {
        return Ok(bp.clone());
    }
    let q = query.to_lowercase();
    let hits: Vec<&Blueprint> = all.iter().filter(|b| b.manifest.name.to_lowercase() == q).collect();
    match hits.len() {
        1 => Ok(hits[0].clone()),
        0 => {
            let available = all.iter().map(|b| b.id.as_str()).collect::<Vec<_>>().join(", ");
            if available.is_empty() {
                Err(format!("no blueprint named `{query}` — there are none yet. save one with `lk blueprint save <site> <name>`."))
            } else {
                Err(format!("no blueprint named `{query}`. available: {available}"))
            }
        }
        _ => Err(format!("`{query}` matches more than one blueprint. pass the exact id.")),
    }
}

// ---------------------------------------------------------------------------
// Save
// ---------------------------------------------------------------------------

/// Save an existing site as a reusable blueprint.
///
/// Snapshots the site (transient `blueprint_source` kind), hardlinks the
/// snapshot's artifacts into the blueprint dir so the bytes are shared, records
/// the plugin/theme list as display metadata, then drops the snapshot. Emits
/// only `snapshot`-stage progress plus its own terminal stage.
pub async fn save(
    app: Option<&AppHandle>,
    state: &AppState,
    site_id: &str,
    name: String,
    description: Option<String>,
) -> Result<Blueprint, String> {
    let s = site::get(state, site_id)?;
    // Blueprints are WordPress recipes (per-kind blueprints arrive with plan
    // 26); saving a docker app through this WP-shaped flow would produce a
    // broken template.
    s.require(s.kind == site::KIND_WORDPRESS, "Saving a blueprint")?;
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("Blueprint name is required".into());
    }

    // Consistent point-in-time artifacts, via the retry-heavy snapshot engine.
    let snap = snapshot::create(
        app,
        state,
        site_id,
        snapshot::KIND_BLUEPRINT_SOURCE,
        Some(format!("blueprint \"{name}\"")),
    )
    .await
    .map_err(|e| format!("could not snapshot the site: {e}"))?;

    // Wrapped so a failure past this point still drops the transient snapshot.
    let result = finish_save(state, &s, &name, description, &snap.id).await;
    let _ = snapshot::delete(state, site_id, &snap.id);

    match result {
        Ok(bp) => {
            site::emit(
                app,
                site_id,
                "done",
                &format!("Saved \"{}\" as the blueprint \"{}\"", s.name, bp.manifest.name),
            );
            Ok(bp)
        }
        Err(e) => {
            site::emit(app, site_id, "error", &format!("Save as blueprint failed: {e}"));
            Err(e)
        }
    }
}

async fn finish_save(
    state: &AppState,
    s: &site::Site,
    name: &str,
    description: Option<String>,
    snapshot_id: &str,
) -> Result<Blueprint, String> {
    // The DB is up (the snapshot just exported it), so capture plugin/theme
    // metadata now — best effort, it is display-only.
    let plugins = capture_plugins(&s.dir()).await.unwrap_or_default();
    let theme = capture_theme(&s.dir()).await.unwrap_or_default();

    let id = unique_slug(&state.data_dir, name);
    let dir = blueprint_dir(&state.data_dir, &id);
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("failed to create blueprint directory: {e}"))?;

    let (snap_db, snap_code) = snapshot::artifact_paths(&state.data_dir, &s.id, snapshot_id);
    hardlink_or_copy(&snap_db, &dir.join(DB_FILE))?;
    hardlink_or_copy(&snap_code, &dir.join(CODE_FILE))?;

    let manifest = Manifest {
        name: name.to_string(),
        description: description.unwrap_or_default().trim().to_string(),
        wp_version: s.wp_version.clone(),
        php_version: s.php_version.clone(),
        plugins,
        theme,
        created_at: chrono::Utc::now().to_rfc3339(),
        source_site_name: s.name.clone(),
    };
    // Manifest last: a half-written blueprint has no manifest, so `list` skips
    // it instead of offering a broken create-from (same rule as snapshots).
    let json = serde_json::to_string_pretty(&manifest)
        .map_err(|e| format!("failed to serialize blueprint manifest: {e}"))?;
    std::fs::write(dir.join(MANIFEST_FILE), json)
        .map_err(|e| format!("failed to write blueprint manifest: {e}"))?;

    read_blueprint(&state.data_dir, &id)
}

/// Active theme name, or `None` when wp-cli can't answer (best effort).
async fn capture_theme(dir: &Path) -> Option<String> {
    let out = docker::compose_run(
        dir,
        "wpcli",
        &["wp", "theme", "list", "--status=active", "--field=name"],
    )
    .await
    .ok()?;
    out.lines().map(str::trim).find(|l| !l.is_empty()).map(str::to_string)
}

/// Plugin list (name/status/version) as display metadata (best effort).
async fn capture_plugins(dir: &Path) -> Result<Vec<BlueprintPlugin>, String> {
    let json = docker::compose_run(
        dir,
        "wpcli",
        &["wp", "plugin", "list", "--format=json", "--fields=name,status,version"],
    )
    .await?;
    serde_json::from_str(&json).map_err(|e| format!("failed to parse plugin list: {e}"))
}

// ---------------------------------------------------------------------------
// Delete
// ---------------------------------------------------------------------------

pub fn delete(state: &AppState, id: &str) -> Result<(), String> {
    let dir = blueprint_dir(&state.data_dir, id);
    if !dir.is_dir() {
        return Err(format!("blueprint `{id}` not found"));
    }
    std::fs::remove_dir_all(&dir).map_err(|e| format!("failed to delete blueprint: {e}"))
}

// ---------------------------------------------------------------------------
// Export / import — a single portable `.lkbp` file (plan 20)
// ---------------------------------------------------------------------------

/// The three files that make up a blueprint on disk; also the only entries an
/// imported archive may contain, so a shared `.lkbp` can't write anything else.
const ARTIFACTS: [&str; 3] = [MANIFEST_FILE, DB_FILE, CODE_FILE];

/// Bundle a blueprint into a single `.lkbp` file (a tar.gz of its three
/// artifacts at the archive root) so it can be shared without a registry.
pub fn export(state: &AppState, id: &str, dest: &Path) -> Result<(), String> {
    let dir = blueprint_dir(&state.data_dir, id);
    if !dir.is_dir() {
        return Err(format!("blueprint `{id}` not found"));
    }
    let file = std::fs::File::create(dest)
        .map_err(|e| format!("failed to create {}: {e}", dest.display()))?;
    let enc = flate2::write::GzEncoder::new(
        std::io::BufWriter::new(file),
        flate2::Compression::fast(),
    );
    let mut builder = tar::Builder::new(enc);
    for name in ARTIFACTS {
        let path = dir.join(name);
        if !path.exists() {
            return Err(format!("blueprint `{id}` is missing {name}; refusing to export a broken bundle"));
        }
        builder
            .append_path_with_name(&path, name)
            .map_err(|e| format!("failed to add {name} to the bundle: {e}"))?;
    }
    builder
        .into_inner()
        .map_err(|e| format!("failed to finalize the bundle: {e}"))?
        .finish()
        .map_err(|e| format!("failed to finalize the bundle: {e}"))?;
    Ok(())
}

/// Install a blueprint from a `.lkbp` file under a fresh unique slug.
///
/// The archive is treated as semi-trusted (a teammate may have made it): only
/// the three known filenames are accepted, each written through `io::copy` so a
/// crafted symlink or path entry can never place a file outside the staging
/// directory. Extraction lands in a temp dir first, so a bad bundle leaves no
/// half-installed blueprint behind.
pub fn import(state: &AppState, src: &Path) -> Result<Blueprint, String> {
    let file = std::fs::File::open(src)
        .map_err(|e| format!("failed to open {}: {e}", src.display()))?;
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(std::io::BufReader::new(file)));

    let root = blueprints_root(&state.data_dir);
    std::fs::create_dir_all(&root)
        .map_err(|e| format!("failed to create blueprints directory: {e}"))?;
    let tmp = root.join(format!(".import-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&tmp).map_err(|e| format!("failed to stage the import: {e}"))?;

    let staged = extract_artifacts(&mut archive, &tmp);
    let bp = staged.and_then(|_| install_staged(state, &tmp));
    if bp.is_err() {
        let _ = std::fs::remove_dir_all(&tmp);
    }
    bp
}

fn extract_artifacts<R: std::io::Read>(
    archive: &mut tar::Archive<R>,
    tmp: &Path,
) -> Result<(), String> {
    let entries = archive
        .entries()
        .map_err(|e| format!("the blueprint bundle is unreadable: {e}"))?;
    for entry in entries {
        let mut entry = entry.map_err(|e| format!("the blueprint bundle is unreadable: {e}"))?;
        let path = entry
            .path()
            .map_err(|e| format!("bundle entry has an unreadable path: {e}"))?
            .into_owned();
        let name = path.to_str().ok_or("bundle entry has a non-UTF-8 name")?;
        if !ARTIFACTS.contains(&name) {
            return Err(format!("blueprint bundle contains an unexpected entry: {name}"));
        }
        // io::copy reads the entry's data stream and writes a plain file — it
        // never follows a link header, so a symlink entry lands as a (harmless,
        // empty) regular file instead of escaping the staging dir.
        let mut out = std::fs::File::create(tmp.join(name))
            .map_err(|e| format!("failed to write {name}: {e}"))?;
        std::io::copy(&mut entry, &mut out).map_err(|e| format!("failed to write {name}: {e}"))?;
    }
    Ok(())
}

fn install_staged(state: &AppState, tmp: &Path) -> Result<Blueprint, String> {
    let text = std::fs::read_to_string(tmp.join(MANIFEST_FILE))
        .map_err(|_| "the bundle has no blueprint.json".to_string())?;
    let manifest: Manifest = serde_json::from_str(&text)
        .map_err(|e| format!("the bundle's blueprint.json is unreadable: {e}"))?;
    for name in [DB_FILE, CODE_FILE] {
        if !tmp.join(name).exists() {
            return Err(format!("the bundle is missing {name}"));
        }
    }
    let id = unique_slug(&state.data_dir, &manifest.name);
    let dest = blueprint_dir(&state.data_dir, &id);
    std::fs::rename(tmp, &dest)
        .map_err(|e| format!("failed to install the imported blueprint: {e}"))?;
    read_blueprint(&state.data_dir, &id)
}

// ---------------------------------------------------------------------------
// Create a site from a blueprint
// ---------------------------------------------------------------------------

/// Provision a brand-new site from a blueprint's recipe.
///
/// The create half of a clone, with the archives coming from the blueprint dir
/// instead of a live source: reserve a fresh site (versions matched to the
/// current allowlist, nearest when the recorded one has aged out), lay the
/// database + wp-content down, and rewrite the baked-in URL — read back out of
/// the imported database — to the new site's own. `wp core install` is never
/// run: the blueprint's database *is* the site.
pub async fn create_site(
    app: Option<&AppHandle>,
    state: &AppState,
    blueprint_id: &str,
    local_name: Option<String>,
) -> Result<site::Site, String> {
    let bp = read_blueprint(&state.data_dir, blueprint_id)?;
    let name = local_name
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| bp.manifest.name.clone());

    let (wp_version, _) = crate::sync::match_version(site::WP_VERSIONS, Some(&bp.manifest.wp_version));
    let (php_version, _) =
        crate::sync::match_version(site::PHP_VERSIONS, Some(&bp.manifest.php_version));

    // Blueprints are WordPress recipes today (per-kind blueprints arrive with
    // plan 26), so the target reserves the WordPress stack.
    let target = site::reserve(
        state,
        name,
        site::KIND_WORDPRESS.to_string(),
        wp_version,
        php_version,
        site::SiteConfig::default(),
        None,
    )
    .await?;

    // Own this site's status until it finishes provisioning (plan 23).
    let _guard = state.in_flight.guard(&target.id);
    match do_create(app, state, blueprint_id, &target).await {
        Ok(site) => {
            let url = router::site_public_url(state, &site);
            site::emit(
                app,
                &site.id,
                "done",
                &format!(
                    "{} created from blueprint \"{}\" — now running at {url}",
                    site.name, bp.manifest.name
                ),
            );
            site::get(state, &site.id)
        }
        Err(e) => {
            site::emit(app, &target.id, "error", &format!("Create from blueprint failed: {e}"));
            let _ = site::cleanup(state, &target).await;
            Err(e)
        }
    }
}

async fn do_create(
    app: Option<&AppHandle>,
    state: &AppState,
    blueprint_id: &str,
    target: &site::Site,
) -> Result<site::Site, String> {
    let dir = target.dir();
    let id = target.id.as_str();

    site::emit(app, id, "files", "Writing project files...");
    site::write_project_files(target)?;

    site::emit(app, id, "pulling", "Downloading WordPress images (first run can take a few minutes)...");
    docker::compose_pull(&dir, &["wordpress", "db", "wpcli"]).await?;

    site::emit(app, id, "containers", "Starting Docker containers...");
    docker::compose_up(&dir).await?;

    site::emit(app, id, "waiting", "Waiting for WordPress to come online...");
    site::wait_for_port(target.port, 180).await?;
    wordpress::wait_for_config(&dir, 24).await?;

    site::emit(app, id, "import", "Laying down the blueprint's content...");
    let bp_dir = blueprint_dir(&state.data_dir, blueprint_id);
    snapshot::restore_archives_into(&bp_dir.join(DB_FILE), &bp_dir.join(CODE_FILE), target).await?;
    // The archive brought its own mu-plugins over the one just written; keep
    // one-click login working.
    wordpress::ensure_login_plugin(&dir)?;

    // The blueprint's database has its source site's URL baked in; read it back
    // and rewrite it to this site's own public URL.
    let target_url = router::site_public_url(state, target);
    let source_url = docker::compose_run(&dir, "wpcli", &["wp", "option", "get", "siteurl"])
        .await
        .map(|u| u.trim().to_string())
        .unwrap_or_default();
    site::emit(app, id, "import", "Rewriting URLs to the new site...");
    wordpress::update_site_urls(&dir, &target_url).await?;
    if !source_url.is_empty() && source_url != target_url {
        wordpress::search_replace(&dir, &source_url, &target_url).await?;
    }
    let _ = docker::compose_run(&dir, "wpcli", &["wp", "rewrite", "flush"]).await;
    let _ = docker::compose_run(&dir, "wpcli", &["wp", "cache", "flush"]).await;

    // The admin login comes from the blueprint's database (its first
    // administrator); no password is stored, exactly like an import.
    let admin_user = first_admin(&dir)
        .await
        .unwrap_or_else(|| target.admin_user.clone());
    {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        db.set_status(id, "running")?;
        db.update_credentials(id, &admin_user, "")?;
    }
    router::refresh_routes(state).await;
    router::refresh_hosts(state).await;
    site::get(state, id)
}

/// First administrator in the freshly imported database, for `admin_user`.
async fn first_admin(dir: &Path) -> Option<String> {
    let out = docker::compose_run(
        dir,
        "wpcli",
        &["wp", "user", "list", "--role=administrator", "--field=user_login"],
    )
    .await
    .ok()?;
    out.lines().map(str::trim).find(|l| !l.is_empty()).map(str::to_string)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_round_trips() {
        let manifest = Manifest {
            name: "Starter Shop".into(),
            description: "WooCommerce + our base theme".into(),
            wp_version: "6.7".into(),
            php_version: "8.3".into(),
            plugins: vec![BlueprintPlugin {
                name: "woocommerce".into(),
                status: "active".into(),
                version: "9.6.0".into(),
            }],
            theme: "storefront".into(),
            created_at: "2026-07-20T10:00:00Z".into(),
            source_site_name: "Pixel Bakery".into(),
        };
        let text = serde_json::to_string_pretty(&manifest).unwrap();
        let back: Manifest = serde_json::from_str(&text).unwrap();
        assert_eq!(back.name, "Starter Shop");
        assert_eq!(back.theme, "storefront");
        assert_eq!(back.plugins.len(), 1);
        assert_eq!(back.plugins[0].name, "woocommerce");
        assert_eq!(back.source_site_name, "Pixel Bakery");
    }

    #[test]
    fn blueprint_flattens_manifest_into_a_flat_payload() {
        // The frontend expects a flat object (id + recipe + sizes), not a
        // nested `manifest`. Flatten is what delivers that.
        let bp = Blueprint {
            id: "starter-shop".into(),
            manifest: Manifest {
                name: "Starter Shop".into(),
                description: String::new(),
                wp_version: "6.7".into(),
                php_version: "8.3".into(),
                plugins: vec![],
                theme: "twentytwentyfive".into(),
                created_at: "2026-07-20T10:00:00Z".into(),
                source_site_name: "Src".into(),
            },
            db_bytes: 2048,
            code_bytes: 4096,
        };
        let v: serde_json::Value = serde_json::to_value(&bp).unwrap();
        assert_eq!(v["id"], "starter-shop");
        assert_eq!(v["name"], "Starter Shop"); // flattened, not v["manifest"]["name"]
        assert_eq!(v["db_bytes"], 2048);
        assert!(v.get("manifest").is_none());
    }

    #[test]
    fn slug_is_unique_against_existing_blueprints() {
        let taken = |s: &str| matches!(s, "shop" | "shop-2" | "shop-3");
        assert_eq!(pick_slug("shop", taken), "shop-4");
        // A free base is used verbatim.
        assert_eq!(pick_slug("blog", |_| false), "blog");
    }

    #[test]
    fn hardlink_or_copy_reproduces_the_bytes() {
        let root = std::env::temp_dir().join(format!("localkit-bp-hlc-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let src = root.join("src.bin");
        let dst = root.join("dst.bin");
        std::fs::write(&src, b"blueprint payload").unwrap();

        hardlink_or_copy(&src, &dst).unwrap();
        assert_eq!(std::fs::read(&dst).unwrap(), b"blueprint payload");

        // Idempotent: a second call over an existing dst still lands the bytes.
        hardlink_or_copy(&src, &dst).unwrap();
        assert_eq!(std::fs::read(&dst).unwrap(), b"blueprint payload");

        let _ = std::fs::remove_dir_all(&root);
    }

    fn make_tgz(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        {
            let mut builder = tar::Builder::new(&mut enc);
            for (name, data) in entries {
                let mut header = tar::Header::new_gnu();
                header.set_size(data.len() as u64);
                header.set_mode(0o644);
                builder.append_data(&mut header, name, *data).unwrap();
            }
            builder.finish().unwrap();
        }
        enc.finish().unwrap()
    }

    fn scratch(tag: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("localkit-bp-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn import_extracts_the_three_known_artifacts() {
        let tmp = scratch("extract-ok");
        let tgz = make_tgz(&[
            ("blueprint.json", b"{}"),
            ("db.sql.gz", b"db"),
            ("wp-content.tar.gz", b"code"),
        ]);
        let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(&tgz[..]));
        extract_artifacts(&mut archive, &tmp).unwrap();
        for f in ["blueprint.json", "db.sql.gz", "wp-content.tar.gz"] {
            assert!(tmp.join(f).exists(), "missing {f}");
        }
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn import_refuses_an_unexpected_entry() {
        // A `.lkbp` may be shared by a teammate: anything but the three known
        // filenames is refused rather than written.
        let tmp = scratch("extract-evil");
        let tgz = make_tgz(&[("blueprint.json", b"{}"), ("evil.txt", b"pwned")]);
        let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(&tgz[..]));
        let err = extract_artifacts(&mut archive, &tmp).unwrap_err();
        assert!(err.contains("unexpected entry"), "unexpected error: {err}");
        assert!(!tmp.join("evil.txt").exists(), "the rejected entry was written anyway");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn copy_fallback_reproduces_the_bytes() {
        // The branch hardlink_or_copy takes when the filesystem refuses a link.
        let root = std::env::temp_dir().join(format!("localkit-bp-copy-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let src = root.join("src.bin");
        let dst = root.join("dst.bin");
        std::fs::write(&src, b"copied bytes").unwrap();

        copy_file(&src, &dst).unwrap();
        assert_eq!(std::fs::read(&dst).unwrap(), b"copied bytes");

        let _ = std::fs::remove_dir_all(&root);
    }
}
