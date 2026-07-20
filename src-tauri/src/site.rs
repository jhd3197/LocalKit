//! Site model + lifecycle (create / start / stop / delete / list / logs).

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter};
use uuid::Uuid;

use crate::{docker, router, wordpress, AppState};

/// Allowlisted versions (kept small on purpose; extend as needed).
pub const WP_VERSIONS: &[&str] = &["6.7", "6.6", "6.5"];
pub const PHP_VERSIONS: &[&str] = &["8.3", "8.2", "8.1"];
pub const DEFAULT_ADMIN_USER: &str = "admin";
/// First host port we try for sites.
pub const BASE_PORT: u16 = 8081;
/// Host DB port = site port + this offset (8081 -> 18081).
pub const DB_PORT_OFFSET: u16 = 10000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Site {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub path: String,
    pub port: u16,
    pub wp_version: String,
    pub php_version: String,
    pub status: String,
    pub admin_user: String,
    pub admin_pass: String,
    pub created_at: String,
}

impl Site {
    pub fn db_port(&self) -> u16 {
        self.port + DB_PORT_OFFSET
    }

    pub fn dir(&self) -> PathBuf {
        PathBuf::from(&self.path)
    }
}

/// A site row plus its live container status.
#[derive(Debug, Clone, Serialize)]
pub struct SiteWithStatus {
    #[serde(flatten)]
    pub site: Site,
    pub live_status: String,
}

/// Detail payload for the site page (includes DB credentials from .env).
#[derive(Debug, Clone, Serialize)]
pub struct SiteDetail {
    #[serde(flatten)]
    pub site: Site,
    pub live_status: String,
    pub db_host: String,
    pub db_port: u16,
    pub db_name: String,
    pub db_user: String,
    pub db_password: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SiteEvent {
    pub id: String,
    pub stage: String,
    pub message: String,
}

/// Emit a progress event to the frontend. `app` is optional so the lifecycle
/// can also be driven from tests / example binaries / the `lk` CLI without a
/// Tauri runtime; in that case progress is printed to stderr instead.
pub(crate) fn emit(app: Option<&AppHandle>, id: &str, stage: &str, message: &str) {
    match app {
        Some(app) => {
            let _ = app.emit(
                "site-event",
                SiteEvent {
                    id: id.to_string(),
                    stage: stage.to_string(),
                    message: message.to_string(),
                },
            );
        }
        None => eprintln!("[{stage}] {message}"),
    }
}

pub fn slugify(name: &str) -> String {
    let mut s = String::new();
    for c in name.trim().chars() {
        if c.is_ascii_alphanumeric() {
            s.push(c.to_ascii_lowercase());
        } else if !s.is_empty() && !s.ends_with('-') {
            s.push('-');
        }
    }
    let s = s.trim_matches('-').to_string();
    if s.is_empty() {
        "site".into()
    } else {
        s
    }
}

fn unique_slug(state: &AppState, base: &str) -> Result<String, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    if !db.slug_exists(base)? {
        return Ok(base.to_string());
    }
    for i in 2..100 {
        let candidate = format!("{base}-{i}");
        if !db.slug_exists(&candidate)? {
            return Ok(candidate);
        }
    }
    Err("could not generate a unique slug".into())
}

/// Pick a free host port starting at BASE_PORT: not used by another site, and
/// with neither it nor its DB port already held on the host.
///
/// The host check consults the OS listener table, not just a trial bind. A
/// bind-only test is the plan-16 SO_REUSEADDR trap all over again: Docker's
/// port publisher binds the wildcard address with SO_REUSEADDR, so binding
/// 127.0.0.1:8081 still succeeds while a container is published on 8081 — we
/// would hand out that port and creation would die at `compose up`, after the
/// image pull, with a raw Docker error. Both ports matter: only the site port
/// was ever checked, so a free site port with a taken DB port failed the same
/// way.
async fn free_port(state: &AppState) -> Result<u16, String> {
    let used = {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        db.used_ports()?
    };
    let listening = router::listening_ports().await;
    let free = |port: u16| -> bool {
        !listening.contains(&port) && std::net::TcpListener::bind(("127.0.0.1", port)).is_ok()
    };
    let mut port = BASE_PORT;
    loop {
        if !used.contains(&port) && free(port) && free(port + DB_PORT_OFFSET) {
            return Ok(port);
        }
        port += 1;
        if port > 55000 {
            return Err("no free port available".into());
        }
    }
}

pub fn random_password(len: usize) -> String {
    use rand::Rng;
    // No ambiguous characters (0/O, 1/l/I).
    const CHARSET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghjkmnpqrstuvwxyz23456789";
    let mut rng = rand::thread_rng();
    (0..len)
        .map(|_| CHARSET[rng.gen_range(0..CHARSET.len())] as char)
        .collect()
}

pub fn site_dir(data_dir: &Path, slug: &str) -> PathBuf {
    data_dir.join("sites").join(slug)
}

// ---------------------------------------------------------------------------
// Compose / env templates
// ---------------------------------------------------------------------------

pub fn render_compose(site: &Site) -> String {
    format!(
        r#"name: localkit-{slug}

services:
  wordpress:
    image: wordpress:{wp}-php{php}-apache
    restart: unless-stopped
    ports:
      - "${{WP_PORT}}:80"
    environment:
      WORDPRESS_DB_HOST: db:3306
      WORDPRESS_DB_NAME: ${{DB_NAME}}
      WORDPRESS_DB_USER: ${{DB_USER}}
      WORDPRESS_DB_PASSWORD: ${{DB_PASSWORD}}
      WORDPRESS_CONFIG_EXTRA: |
        define('WP_ENVIRONMENT_TYPE', 'local');
    volumes:
      - wp-data:/var/www/html
      - ./wp-content:/var/www/html/wp-content
    depends_on:
      db:
        condition: service_healthy

  db:
    image: mariadb:11
    restart: unless-stopped
    ports:
      - "${{DB_PORT}}:3306"
    environment:
      MYSQL_DATABASE: ${{DB_NAME}}
      MYSQL_USER: ${{DB_USER}}
      MYSQL_PASSWORD: ${{DB_PASSWORD}}
      MYSQL_RANDOM_ROOT_PASSWORD: "1"
    volumes:
      - db-data:/var/lib/mysql
    healthcheck:
      test: ["CMD", "healthcheck.sh", "--connect", "--innodb_initialized"]
      interval: 5s
      timeout: 3s
      retries: 24

  # wp-cli helper (official wordpress:cli image). Started on demand via
  # `docker compose run --rm wpcli ...`; the profile keeps it out of `up`.
  wpcli:
    image: wordpress:cli-php{php}
    profiles: ["tools"]
    depends_on:
      db:
        condition: service_healthy
    environment:
      WORDPRESS_DB_HOST: db:3306
      WORDPRESS_DB_NAME: ${{DB_NAME}}
      WORDPRESS_DB_USER: ${{DB_USER}}
      WORDPRESS_DB_PASSWORD: ${{DB_PASSWORD}}
    volumes:
      - wp-data:/var/www/html
      - ./wp-content:/var/www/html/wp-content

volumes:
  wp-data:
  db-data:
"#,
        slug = site.slug,
        wp = site.wp_version,
        php = site.php_version,
    )
}

pub fn render_env(site: &Site, db_password: &str) -> String {
    format!(
        "WP_PORT={}\nDB_PORT={}\nDB_NAME=wordpress\nDB_USER=wordpress\nDB_PASSWORD={}\n",
        site.port,
        site.db_port(),
        db_password
    )
}

fn read_env_value(dir: &Path, key: &str) -> Option<String> {
    let content = std::fs::read_to_string(dir.join(".env")).ok()?;
    for line in content.lines() {
        if let Some((k, v)) = line.split_once('=') {
            if k.trim() == key {
                return Some(v.trim().to_string());
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Lifecycle
// ---------------------------------------------------------------------------

pub async fn create(
    app: Option<&AppHandle>,
    state: &AppState,
    name: String,
    wp_version: String,
    php_version: String,
) -> Result<Site, String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("Site name is required".into());
    }
    if !WP_VERSIONS.contains(&wp_version.as_str()) {
        return Err(format!("unsupported WordPress version: {wp_version}"));
    }
    if !PHP_VERSIONS.contains(&php_version.as_str()) {
        return Err(format!("unsupported PHP version: {php_version}"));
    }

    let slug = unique_slug(state, &slugify(&name))?;
    let port = free_port(state).await?;
    let dir = site_dir(&state.data_dir, &slug);

    let site = Site {
        id: Uuid::new_v4().to_string(),
        name,
        slug,
        path: dir.to_string_lossy().to_string(),
        port,
        wp_version,
        php_version,
        status: "creating".into(),
        admin_user: DEFAULT_ADMIN_USER.into(),
        admin_pass: String::new(),
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        db.insert_site(&site)?;
    }

    match do_create(app, state, &site).await {
        Ok(site) => Ok(site),
        Err(e) => {
            emit(app, &site.id, "error", &format!("Creation failed: {e}"));
            let _ = cleanup(state, &site).await;
            Err(e)
        }
    }
}

async fn do_create(app: Option<&AppHandle>, state: &AppState, site: &Site) -> Result<Site, String> {
    let dir = site.dir();

    emit(app, &site.id, "files", "Writing project files...");
    std::fs::create_dir_all(dir.join("wp-content"))
        .map_err(|e| format!("failed to create site directory: {e}"))?;
    let db_password = random_password(24);
    std::fs::write(dir.join("docker-compose.yml"), render_compose(site))
        .map_err(|e| format!("failed to write docker-compose.yml: {e}"))?;
    std::fs::write(dir.join(".env"), render_env(site, &db_password))
        .map_err(|e| format!("failed to write .env: {e}"))?;
    wordpress::ensure_login_plugin(&dir)?;

    emit(
        app,
        &site.id,
        "pulling",
        "Downloading WordPress images (first run can take a few minutes)...",
    );
    docker::compose_pull(&dir, &["wordpress", "db", "wpcli"]).await?;

    emit(
        app,
        &site.id,
        "containers",
        "Starting Docker containers...",
    );
    docker::compose_up(&dir).await?;

    emit(app, &site.id, "waiting", "Waiting for WordPress to come online...");
    wait_for_port(site.port, 180).await?;

    // Install at the site's local domain when the router is enabled (M6),
    // including the `:port` suffix in fallback mode (plan 16).
    let install_url = router::site_public_url(state, site);
    let admin_pass = random_password(16);
    wordpress::install(&dir, site, &admin_pass, &install_url, app).await?;

    let mut site = site.clone();
    site.status = "running".into();
    site.admin_pass = admin_pass;
    {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        db.set_status(&site.id, "running")?;
        db.update_credentials(&site.id, &site.admin_user, &site.admin_pass)?;
    }
    // Add the new site to the router's Caddyfile + hosts block (no-op when disabled).
    router::refresh_routes(state).await;
    router::refresh_hosts(state).await;
    emit(
        app,
        &site.id,
        "done",
        &format!("{} is ready at {}", site.name, install_url),
    );
    Ok(site)
}

async fn cleanup(state: &AppState, site: &Site) -> Result<(), String> {
    let dir = site.dir();
    if dir.exists() {
        let _ = docker::compose_down(&dir, true).await;
        let _ = std::fs::remove_dir_all(&dir);
    }
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.delete_site(&site.id)
}

/// Wait until something (Apache) accepts TCP connections on the site port.
async fn wait_for_port(port: u16, timeout_secs: u64) -> Result<(), String> {
    let addr = format!("127.0.0.1:{port}");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);
    while std::time::Instant::now() < deadline {
        if tokio::net::TcpStream::connect(&addr).await.is_ok() {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
    Err("timed out waiting for WordPress to respond".into())
}

pub fn get(state: &AppState, id: &str) -> Result<Site, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.get_site(id)
}

pub fn detail(state: &AppState, id: &str) -> Result<SiteDetail, String> {
    let site = get(state, id)?;
    let dir = site.dir();
    let db_password = read_env_value(&dir, "DB_PASSWORD").unwrap_or_default();
    Ok(SiteDetail {
        db_port: site.db_port(),
        live_status: site.status.clone(),
        db_host: "127.0.0.1".into(),
        db_name: "wordpress".into(),
        db_user: "wordpress".into(),
        db_password,
        site,
    })
}

pub async fn start(state: &AppState, id: &str) -> Result<Site, String> {
    let site = get(state, id)?;
    docker::compose_up(&site.dir()).await?;
    {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        db.set_status(id, "running")?;
    }
    router::refresh_routes(state).await;
    get(state, id)
}

pub async fn stop(state: &AppState, id: &str) -> Result<Site, String> {
    let site = get(state, id)?;
    docker::compose_down(&site.dir(), false).await?;
    {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        db.set_status(id, "stopped")?;
    }
    router::refresh_routes(state).await;
    get(state, id)
}

/// Delete a site. Unless `delete_snapshots` is set, a `pre_delete` snapshot is
/// taken first and the site's snapshot directory survives the deletion — the
/// only copy of the data once the containers, volumes and files are gone
/// (plan 17; also the groundwork for a future "restore deleted site").
///
/// The snapshot is best effort: a site whose Docker stack is broken must still
/// be deletable, so a snapshot failure is reported through the event stream
/// rather than blocking the delete.
pub async fn delete(
    app: Option<&AppHandle>,
    state: &AppState,
    id: &str,
    delete_snapshots: bool,
) -> Result<(), String> {
    let site = get(state, id)?;
    let dir = site.dir();

    if !delete_snapshots && dir.exists() {
        emit(app, id, "snapshot", "Taking a snapshot before deleting...");
        if let Err(e) = crate::snapshot::create(
            app,
            state,
            id,
            crate::snapshot::KIND_PRE_DELETE,
            Some(format!("before deleting {}", site.name)),
        )
        .await
        {
            emit(
                app,
                id,
                "snapshot",
                &format!("Could not snapshot before deleting ({e}) — deleting anyway"),
            );
        }
    }

    if dir.exists() {
        // Best effort: even if Docker is down we still remove local state.
        let _ = docker::compose_down(&dir, true).await;
        std::fs::remove_dir_all(&dir).map_err(|e| format!("failed to remove site directory: {e}"))?;
    }
    if delete_snapshots {
        let _ = crate::snapshot::delete_all(&state.data_dir, id);
    }
    {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        db.delete_site(id)?;
    }
    // Drop the deleted site from the router's Caddyfile + hosts block
    // (no-op when disabled).
    router::refresh_routes(state).await;
    router::refresh_hosts(state).await;
    Ok(())
}

pub async fn list(state: &AppState) -> Result<Vec<SiteWithStatus>, String> {
    let sites = {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        db.list_sites()?
    };
    let mut out = Vec::new();
    for site in sites {
        let live_status = match docker::compose_ps(&site.dir()).await {
            Ok(containers) => {
                if containers
                    .iter()
                    .any(|c| c.service == "wordpress" && c.state == "running")
                {
                    "running".to_string()
                } else {
                    "stopped".to_string()
                }
            }
            // Docker unavailable/off: fall back to the stored status.
            Err(_) => site.status.clone(),
        };
        out.push(SiteWithStatus { site, live_status });
    }
    Ok(out)
}

pub async fn logs(state: &AppState, id: &str, tail: u32) -> Result<String, String> {
    let site = get(state, id)?;
    docker::compose_logs(&site.dir(), tail).await
}
