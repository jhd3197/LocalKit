//! Generic "bring your own compose" Docker app kind (plan 22 phase 2).
//!
//! Unlike a WordPress site, LocalKit does not generate the compose project — it
//! **copies** an existing one the user points at into the managed site dir
//! (owned, not referenced: an external dir is a backup/locking nightmare). The
//! user picks which service is the app and its published port; LocalKit records
//! that in `SiteConfig` and everything the Phase-1 de-hardcoding unlocked —
//! lifecycle, logs, terminal, local domain, tray, `lk` — works for free.
//!
//! DB detection is captured (`config.db_engine`/`db_service`) but a docker app
//! stays code-only for now: engine-native dumps are a follow-up, so `db_sync`
//! stays off (see `site::Capabilities::for_kind`).

use serde::Serialize;
use std::path::{Path, PathBuf};
use tauri::AppHandle;

use crate::{
    docker, router,
    site::{self, SiteConfig},
    AppState,
};

/// Directory names excluded from the copy by default — heavy and regenerable.
/// The import can opt out (`include_all`) to copy them anyway.
pub const DEFAULT_EXCLUDES: &[&str] = &[".git", "node_modules", "vendor"];

/// Compose filenames LocalKit looks for, in Docker's own precedence order.
const COMPOSE_NAMES: &[&str] = &[
    "compose.yaml",
    "compose.yml",
    "docker-compose.yaml",
    "docker-compose.yml",
];

/// Map a recognized database image to its engine tag. Matched on the repository
/// component so `mariadb`, `bitnami/mariadb` and `docker.io/library/mysql:8`
/// all resolve. `None` = not a database we know how to dump.
fn detect_db_engine(image: &str) -> Option<&'static str> {
    let img = image.to_lowercase();
    // Drop any registry/namespace prefix and the tag/digest.
    let repo = img.rsplit('/').next().unwrap_or(&img);
    let repo = repo.split(['@', ':']).next().unwrap_or(repo);
    if repo == "mariadb" {
        Some("mariadb")
    } else if repo == "mysql" {
        Some("mysql")
    } else if repo == "postgres" || repo == "postgresql" {
        Some("postgres")
    } else {
        None
    }
}

/// A service found in the compose project (for the import dialog's picker).
#[derive(Debug, Clone, Serialize)]
pub struct DockerService {
    pub name: String,
    pub image: String,
    /// Host ports this service publishes; the first is the suggested app port.
    pub published_ports: Vec<u16>,
    /// The recognized DB engine tag when this service is a database.
    pub db_engine: Option<String>,
}

/// What the import dialog needs to know about a chosen folder before creating.
#[derive(Debug, Clone, Serialize)]
pub struct DockerProjectInspection {
    /// The compose file that was found (bare filename).
    pub compose_file: String,
    pub services: Vec<DockerService>,
    /// Suggested app service: the first non-DB service that publishes a port.
    pub suggested_service: Option<String>,
    pub suggested_port: Option<u16>,
    /// The recognized DB engine among the services (captured, not yet synced).
    pub db_engine: Option<String>,
    pub db_service: Option<String>,
    /// Bytes to copy after applying the default excludes — shown before the
    /// user confirms, so a huge project is not copied by surprise.
    pub copy_bytes: u64,
    /// The default excludes applied to that estimate.
    pub excluded: Vec<String>,
}

fn find_compose(dir: &Path) -> Option<String> {
    COMPOSE_NAMES
        .iter()
        .find(|n| dir.join(n).is_file())
        .map(|s| s.to_string())
}

/// Pull `published` out of a `docker compose config` port entry, which may be a
/// string (`"8080"`) or a bare number depending on the source compose.
fn parse_published(value: &serde_json::Value) -> Option<u16> {
    match value {
        serde_json::Value::String(s) => s.split('/').next()?.parse().ok(),
        serde_json::Value::Number(n) => u16::try_from(n.as_u64()?).ok(),
        _ => None,
    }
}

/// Parse the normalized `docker compose config --format json` into services.
fn parse_services(config: &serde_json::Value) -> Vec<DockerService> {
    let Some(services) = config.get("services").and_then(|s| s.as_object()) else {
        return Vec::new();
    };
    let mut out: Vec<DockerService> = services
        .iter()
        .map(|(name, svc)| {
            let image = svc
                .get("image")
                .and_then(|i| i.as_str())
                .unwrap_or_default()
                .to_string();
            let published_ports = svc
                .get("ports")
                .and_then(|p| p.as_array())
                .map(|ports| {
                    ports
                        .iter()
                        .filter_map(|p| p.get("published").and_then(parse_published))
                        .collect()
                })
                .unwrap_or_default();
            DockerService {
                name: name.clone(),
                image: image.clone(),
                published_ports,
                db_engine: detect_db_engine(&image).map(str::to_string),
            }
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// Recursively sum file sizes under `dir`, skipping any directory or file whose
/// name is in `excludes` (checked at every level, so a nested `node_modules`
/// is skipped too). Best-effort — an unreadable entry counts as zero.
fn dir_size(dir: &Path, excludes: &[&str]) -> u64 {
    let mut total = 0u64;
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        if excludes.iter().any(|e| *e == name.to_string_lossy()) {
            continue;
        }
        match entry.file_type() {
            Ok(ft) if ft.is_dir() => total += dir_size(&entry.path(), excludes),
            Ok(ft) if ft.is_file() => {
                if let Ok(meta) = entry.metadata() {
                    total += meta.len();
                }
            }
            _ => {}
        }
    }
    total
}

/// Inspect a candidate Docker project folder for the import dialog.
pub async fn inspect(source_dir: &Path) -> Result<DockerProjectInspection, String> {
    if !source_dir.is_dir() {
        return Err(format!("{} is not a folder", source_dir.display()));
    }
    let compose_file = find_compose(source_dir).ok_or_else(|| {
        "no compose file found — the folder needs a docker-compose.yml or compose.yml".to_string()
    })?;

    let json = docker::compose_config(source_dir).await.map_err(|e| {
        format!("the compose project could not be read (is it valid?): {e}")
    })?;
    let value: serde_json::Value =
        serde_json::from_str(&json).map_err(|e| format!("could not parse the compose project: {e}"))?;
    let services = parse_services(&value);
    if services.is_empty() {
        return Err("the compose project defines no services".into());
    }

    let db = services.iter().find(|s| s.db_engine.is_some());
    let app = services
        .iter()
        .find(|s| s.db_engine.is_none() && !s.published_ports.is_empty());

    Ok(DockerProjectInspection {
        compose_file,
        db_engine: db.and_then(|s| s.db_engine.clone()),
        db_service: db.map(|s| s.name.clone()),
        suggested_service: app.map(|s| s.name.clone()),
        suggested_port: app.and_then(|s| s.published_ports.first().copied()),
        copy_bytes: dir_size(source_dir, DEFAULT_EXCLUDES),
        excluded: DEFAULT_EXCLUDES.iter().map(|s| s.to_string()).collect(),
        services,
    })
}

/// Recursively copy `src` into `dst`, skipping names in `excludes` and symlinks
/// (a symlink could point outside the tree — refuse it rather than follow it).
/// Shared with the plan-26 php import ("bring your own code into a generated
/// stack" is the same copy problem as importing a whole compose project).
pub(crate) fn copy_tree(src: &Path, dst: &Path, excludes: &[&str]) -> Result<(), String> {
    std::fs::create_dir_all(dst)
        .map_err(|e| format!("failed to create {}: {e}", dst.display()))?;
    let entries =
        std::fs::read_dir(src).map_err(|e| format!("failed to read {}: {e}", src.display()))?;
    for entry in entries.flatten() {
        let name = entry.file_name();
        if excludes.iter().any(|e| *e == name.to_string_lossy()) {
            continue;
        }
        let from = entry.path();
        let to = dst.join(&name);
        match entry.file_type() {
            Ok(ft) if ft.is_dir() => copy_tree(&from, &to, excludes)?,
            Ok(ft) if ft.is_file() => {
                std::fs::copy(&from, &to)
                    .map_err(|e| format!("failed to copy {}: {e}", from.display()))?;
            }
            // Skip symlinks, sockets, fifos — a compose project is plain files.
            _ => {}
        }
    }
    Ok(())
}

/// Give the copied project a deterministic compose project name via `.env`
/// (`COMPOSE_PROJECT_NAME=localkit-<slug>`). Appends to an existing `.env`
/// rather than clobbering it, and only if the key is not already set.
fn ensure_project_name_env(dir: &Path, slug: &str) -> Result<(), String> {
    let env_path = dir.join(".env");
    let existing = std::fs::read_to_string(&env_path).unwrap_or_default();
    if existing
        .lines()
        .any(|l| l.trim_start().starts_with("COMPOSE_PROJECT_NAME"))
    {
        return Ok(());
    }
    let mut content = existing;
    if !content.is_empty() && !content.ends_with('\n') {
        content.push('\n');
    }
    content.push_str(&format!("COMPOSE_PROJECT_NAME=localkit-{slug}\n"));
    std::fs::write(&env_path, content).map_err(|e| format!("failed to write .env: {e}"))
}

/// Import a Docker project as a new local site.
///
/// Copies the folder into a managed site dir, records the chosen app service +
/// port (and any detected DB engine) in `SiteConfig`, then brings it up. On any
/// failure after the site is reserved, the half-built site is cleaned up
/// wholesale, exactly like the WordPress create/import flows.
pub async fn import_project(
    app: Option<&AppHandle>,
    state: &AppState,
    name: String,
    source_dir: PathBuf,
    service: String,
    app_port: u16,
    include_all: bool,
) -> Result<site::Site, String> {
    // Re-inspect at import time: it validates the folder and recaptures the DB
    // engine, so a stale dialog cannot smuggle in a bad service/port.
    let inspection = inspect(&source_dir).await?;
    if !inspection.services.iter().any(|s| s.name == service) {
        return Err(format!(
            "no service named `{service}` in the compose project"
        ));
    }
    if app_port == 0 {
        return Err("the app port must be between 1 and 65535".into());
    }

    let config = SiteConfig {
        service,
        // A docker app's "code" is the whole copied project.
        sync_path: ".".to_string(),
        app_port: Some(app_port),
        db_engine: inspection.db_engine.clone(),
        db_service: inspection.db_service.clone(),
    };

    let site = site::reserve(
        state,
        name,
        site::KIND_DOCKER.to_string(),
        String::new(),
        String::new(),
        config,
        None,
    )
    .await?;

    // Own this site's status until the import finishes (plan 23).
    let _guard = state.in_flight.guard(&site.id);
    let excludes: &[&str] = if include_all { &[] } else { DEFAULT_EXCLUDES };
    match do_import(app, state, &site, &source_dir, excludes, app_port).await {
        Ok(site) => {
            let url = router::site_public_url(state, &site);
            site::emit(
                app,
                &site.id,
                "done",
                &format!("{} imported — now running at {url}", site.name),
            );
            Ok(site)
        }
        Err(e) => {
            site::emit(app, &site.id, "error", &format!("Import failed: {e}"));
            let _ = site::cleanup(state, &site).await;
            Err(e)
        }
    }
}

async fn do_import(
    app: Option<&AppHandle>,
    state: &AppState,
    site: &site::Site,
    source_dir: &Path,
    excludes: &[&str],
    app_port: u16,
) -> Result<site::Site, String> {
    let dir = site.dir();
    let id = site.id.as_str();

    site::emit(app, id, "files", "Copying the Docker project...");
    copy_tree(source_dir, &dir, excludes)?;
    ensure_project_name_env(&dir, &site.slug)?;

    site::emit(
        app,
        id,
        "pulling",
        "Pulling images (first run can take a few minutes)...",
    );
    // Best-effort: `up` pulls anything still missing, so a registry hiccup here
    // must not fail the import outright.
    let _ = docker::compose_pull_all(&dir).await;

    site::emit(app, id, "containers", "Starting containers...");
    docker::compose_up(&dir).await?;

    // Wait for the app's published port, but don't fail the import if it never
    // answers: a generic app may be a worker, a slow starter, or non-HTTP.
    site::emit(app, id, "waiting", "Waiting for the app to come online...");
    let _ = site::wait_for_port(app_port, 120).await;

    let mut running = site.clone();
    running.status = "running".into();
    {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        db.set_status(id, "running")?;
    }
    // Last step: the completion marker (plan 23) — its absence flags a killed
    // import.
    site::mark_complete(&dir);
    // A docker app is an ordinary site to the router/tray — it gets a domain.
    router::refresh_routes(state).await;
    router::refresh_hosts(state).await;
    Ok(running)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_database_images_by_repository() {
        assert_eq!(detect_db_engine("mariadb:11"), Some("mariadb"));
        assert_eq!(detect_db_engine("mysql:8.0"), Some("mysql"));
        assert_eq!(detect_db_engine("postgres:16-alpine"), Some("postgres"));
        assert_eq!(detect_db_engine("bitnami/postgresql:16"), Some("postgres"));
        assert_eq!(detect_db_engine("docker.io/library/mariadb:latest"), Some("mariadb"));
        // Not a database, and not a false positive on a lookalike name.
        assert_eq!(detect_db_engine("nginx:latest"), None);
        assert_eq!(detect_db_engine("my-mysql-admin:1"), None);
        assert_eq!(detect_db_engine("postgrest/postgrest"), None);
    }

    #[test]
    fn parses_services_ports_and_db_engine_from_compose_config() {
        let json = serde_json::json!({
            "services": {
                "web": { "image": "nginx:latest", "ports": [
                    { "target": 80, "published": "8091", "protocol": "tcp" }
                ]},
                "db": { "image": "postgres:16-alpine" },
                "worker": { "image": "python:3.12" }
            }
        });
        let services = parse_services(&json);
        // Sorted by name.
        assert_eq!(services.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["db", "web", "worker"]);
        let web = services.iter().find(|s| s.name == "web").unwrap();
        assert_eq!(web.published_ports, vec![8091]);
        assert!(web.db_engine.is_none());
        let db = services.iter().find(|s| s.name == "db").unwrap();
        assert_eq!(db.db_engine.as_deref(), Some("postgres"));
        assert!(db.published_ports.is_empty());
    }

    #[test]
    fn parses_a_numeric_published_port() {
        let json = serde_json::json!({
            "services": { "app": { "image": "caddy", "ports": [ { "published": 3000, "target": 3000 } ] } }
        });
        let services = parse_services(&json);
        assert_eq!(services[0].published_ports, vec![3000]);
    }

    #[test]
    fn dir_size_applies_the_ignore_list_at_every_level() {
        let root = std::env::temp_dir().join(format!("lk-dockerapp-size-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::create_dir_all(root.join("node_modules/pkg")).unwrap();
        std::fs::create_dir_all(root.join("src/node_modules")).unwrap();
        std::fs::write(root.join("compose.yml"), vec![0u8; 100]).unwrap();
        std::fs::write(root.join("src/app.js"), vec![0u8; 50]).unwrap();
        std::fs::write(root.join("node_modules/pkg/index.js"), vec![0u8; 9999]).unwrap();
        std::fs::write(root.join("src/node_modules/dep.js"), vec![0u8; 8888]).unwrap();

        // Only compose.yml (100) + src/app.js (50) count; both node_modules skip.
        assert_eq!(dir_size(&root, DEFAULT_EXCLUDES), 150);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn copy_tree_skips_excluded_dirs_and_reproduces_the_rest() {
        let base = std::env::temp_dir().join(format!("lk-dockerapp-copy-{}", std::process::id()));
        let src = base.join("src");
        let dst = base.join("dst");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(src.join("app")).unwrap();
        std::fs::create_dir_all(src.join(".git")).unwrap();
        std::fs::create_dir_all(src.join("node_modules")).unwrap();
        std::fs::write(src.join("docker-compose.yml"), b"services: {}").unwrap();
        std::fs::write(src.join("app/main.py"), b"print(1)").unwrap();
        std::fs::write(src.join(".git/HEAD"), b"ref").unwrap();
        std::fs::write(src.join("node_modules/x.js"), b"x").unwrap();

        copy_tree(&src, &dst, DEFAULT_EXCLUDES).unwrap();
        assert!(dst.join("docker-compose.yml").is_file());
        assert_eq!(std::fs::read(dst.join("app/main.py")).unwrap(), b"print(1)");
        assert!(!dst.join(".git").exists(), ".git must be excluded");
        assert!(!dst.join("node_modules").exists(), "node_modules must be excluded");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn ensure_project_name_env_appends_without_clobbering() {
        let dir = std::env::temp_dir().join(format!("lk-dockerapp-env-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(".env"), "EXISTING=1").unwrap();

        ensure_project_name_env(&dir, "my-api").unwrap();
        let content = std::fs::read_to_string(dir.join(".env")).unwrap();
        assert!(content.contains("EXISTING=1"), "existing keys preserved");
        assert!(content.contains("COMPOSE_PROJECT_NAME=localkit-my-api"));

        // Idempotent: a second call does not duplicate the key.
        ensure_project_name_env(&dir, "my-api").unwrap();
        let content = std::fs::read_to_string(dir.join(".env")).unwrap();
        assert_eq!(content.matches("COMPOSE_PROJECT_NAME").count(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
