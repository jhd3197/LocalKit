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

/// Site kinds (plan 22). WordPress is the reference implementation with every
/// capability; `docker` is a bring-your-own-compose project. The stored default
/// is `wordpress`, so every pre-plan-22 row migrates cleanly. (`php`/Laravel
/// arrives with plan 26.)
pub const KIND_WORDPRESS: &str = "wordpress";
pub const KIND_DOCKER: &str = "docker";

/// Per-kind settings, persisted as the `config_json` column (plan 22).
///
/// Every field is de-hardcoded from a WordPress assumption LocalKit used to
/// bake in: the terminal/log service name, the code-sync path, the router
/// upstream port, and (for docker apps) which compose service is a recognized
/// database engine. The defaults ARE the WordPress values, so a legacy row with
/// `config_json = '{}'` deserializes to exactly the behaviour it had before.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SiteConfig {
    /// Compose service the terminal shells into and single-service logs read.
    #[serde(default = "SiteConfig::default_service")]
    pub service: String,
    /// Path under the site directory that code sync + snapshots archive.
    #[serde(default = "SiteConfig::default_sync_path")]
    pub sync_path: String,
    /// Host port the router proxies to; `None` = the site's own `port`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_port: Option<u16>,
    /// Recognized DB engine in a docker app's compose (`mysql`|`mariadb`|
    /// `postgres`), which flips on `db_sync`. `None` = a code-only app.
    /// WordPress leaves this unset — it always has `db_sync` via wp-cli.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub db_engine: Option<String>,
    /// The compose service of that DB engine, for native dumps (plan 22 phase 2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub db_service: Option<String>,
}

impl Default for SiteConfig {
    fn default() -> Self {
        Self {
            service: Self::default_service(),
            sync_path: Self::default_sync_path(),
            app_port: None,
            db_engine: None,
            db_service: None,
        }
    }
}

impl SiteConfig {
    fn default_service() -> String {
        KIND_WORDPRESS.to_string()
    }
    fn default_sync_path() -> String {
        "wp-content".to_string()
    }
    /// The host port the router should proxy to — the app's own port when a
    /// docker project publishes on a different one, else the site port.
    pub fn upstream_port(&self, site_port: u16) -> u16 {
        self.app_port.unwrap_or(site_port)
    }
}

/// What a site's kind (plus config) supports. Every feature in the app checks
/// one of these instead of assuming WordPress (plan 22). WordPress = all true;
/// docker = `domains, terminal, logs, snapshots, code_sync` (and `db_sync` when
/// a recognized DB engine is in its compose).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct Capabilities {
    pub domains: bool,
    pub terminal: bool,
    pub logs: bool,
    pub snapshots: bool,
    pub db_gui: bool,
    pub db_sync: bool,
    pub code_sync: bool,
    pub one_click_login: bool,
    pub wp_tools: bool,
    pub search_replace: bool,
}

impl Capabilities {
    pub const WORDPRESS: Self = Self {
        domains: true,
        terminal: true,
        logs: true,
        snapshots: true,
        db_gui: true,
        db_sync: true,
        code_sync: true,
        one_click_login: true,
        wp_tools: true,
        search_replace: true,
    };
    pub const DOCKER: Self = Self {
        domains: true,
        terminal: true,
        logs: true,
        snapshots: true,
        db_gui: false,
        db_sync: false,
        code_sync: true,
        one_click_login: false,
        wp_tools: false,
        search_replace: false,
    };

    /// Derive the capability set for a kind + its config. Every kind × every
    /// capability is an explicit decision here (unit-tested), never an `if`
    /// scattered through a feature.
    pub fn for_kind(kind: &str, config: &SiteConfig) -> Self {
        match kind {
            KIND_DOCKER => {
                let mut c = Self::DOCKER;
                // A recognized database engine in the copied compose earns the
                // site engine-native DB snapshots/dumps (plan 22 phase 2).
                if config.db_engine.is_some() {
                    c.db_sync = true;
                }
                c
            }
            // `wordpress` and any unknown/legacy kind fall back to the fully
            // capable WordPress set — the safe default for a pre-plan-22 row.
            _ => Self::WORDPRESS,
        }
    }
}

impl Default for Capabilities {
    fn default() -> Self {
        Self::WORDPRESS
    }
}

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
    /// Plan 18 — where this site came from. Both are set together when a site
    /// is imported from a ServerKit server, and `None` on hand-made sites;
    /// they let a future pull default to the right remote.
    #[serde(default)]
    pub connection_id: Option<String>,
    #[serde(default)]
    pub remote_site_id: Option<i64>,
    /// Plan 22 — stack kind (`wordpress` | `docker`) and its per-kind settings.
    /// `kind` defaults to WordPress so legacy rows migrate cleanly; `config`
    /// defaults to the WordPress values (service `wordpress`, sync path
    /// `wp-content`).
    #[serde(default = "default_kind")]
    pub kind: String,
    #[serde(default)]
    pub config: SiteConfig,
    /// Derived, read-only: what this site supports. Recomputed from `kind` +
    /// `config` at every read (never persisted), so it can never drift.
    #[serde(default, skip_deserializing)]
    pub capabilities: Capabilities,
}

fn default_kind() -> String {
    KIND_WORDPRESS.to_string()
}

impl Site {
    pub fn db_port(&self) -> u16 {
        self.port + DB_PORT_OFFSET
    }

    pub fn dir(&self) -> PathBuf {
        PathBuf::from(&self.path)
    }

    /// Recompute `capabilities` from the current `kind`/`config`. Call after
    /// building or mutating a `Site` so the derived field stays in sync.
    pub fn refresh_capabilities(&mut self) {
        self.capabilities = Capabilities::for_kind(&self.kind, &self.config);
    }

    /// The live-status service to watch — `wordpress` for a WP site, the chosen
    /// app service for a docker project.
    pub fn app_service(&self) -> &str {
        &self.config.service
    }

    /// Guard a capability-gated command with a clean, user-displayable refusal
    /// (the frontends hide the affordance; this catches a direct invoke / CLI).
    pub fn require(&self, cap: bool, action: &str) -> Result<(), String> {
        if cap {
            Ok(())
        } else {
            Err(format!(
                "{action} is not supported for {} sites.",
                self.kind
            ))
        }
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
    /// Byte counters, present only during a chunked transfer (plan 19).
    /// Absent everywhere else, so every non-transfer stage keeps rendering
    /// as the plain stage message it always was.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes_done: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes_total: Option<u64>,
}

/// Emit a progress event to the frontend. `app` is optional so the lifecycle
/// can also be driven from tests / example binaries / the `lk` CLI without a
/// Tauri runtime; in that case progress is printed to stderr instead.
pub(crate) fn emit(app: Option<&AppHandle>, id: &str, stage: &str, message: &str) {
    dispatch(app, id, stage, message, None, None);
}

/// Emit a transfer-progress event carrying byte counters.
///
/// These fire once per chunk, so the frontend gets a real byte readout instead
/// of one coarse "Uploading..." that sits there for ten minutes.
pub(crate) fn emit_bytes(
    app: Option<&AppHandle>,
    id: &str,
    stage: &str,
    message: &str,
    done: u64,
    total: u64,
) {
    dispatch(app, id, stage, message, Some(done), Some(total));
}

fn dispatch(
    app: Option<&AppHandle>,
    id: &str,
    stage: &str,
    message: &str,
    bytes_done: Option<u64>,
    bytes_total: Option<u64>,
) {
    match app {
        Some(app) => {
            let _ = app.emit(
                "site-event",
                SiteEvent {
                    id: id.to_string(),
                    stage: stage.to_string(),
                    message: message.to_string(),
                    bytes_done,
                    bytes_total,
                },
            );
        }
        None => match (bytes_done, bytes_total) {
            (Some(done), Some(total)) => eprintln!(
                "[{stage}] {message} ({} / {})",
                crate::transfer::human_bytes(done),
                crate::transfer::human_bytes(total)
            ),
            _ => eprintln!("[{stage}] {message}"),
        },
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

/// Where a new site came from (plan 18): `None` for a hand-made site, or the
/// connection + remote site id it was imported from.
pub type Origin = Option<(String, i64)>;

/// Reserve a site: validate versions, allocate a unique slug and free ports,
/// and insert the `creating` row. Shared by `create` and `sync::import_site` —
/// both need an identical reservation, and doing it in one place is what keeps
/// slug/port allocation race-free across the two entry points.
///
/// `kind`/`config` carry the plan-22 stack (WordPress callers pass
/// `KIND_WORDPRESS` + `SiteConfig::default()`). WP/PHP version validation only
/// runs for the WordPress kind — a docker project has no such versions.
pub(crate) async fn reserve(
    state: &AppState,
    name: String,
    kind: String,
    wp_version: String,
    php_version: String,
    config: SiteConfig,
    origin: Origin,
) -> Result<Site, String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("Site name is required".into());
    }
    if kind == KIND_WORDPRESS {
        if !WP_VERSIONS.contains(&wp_version.as_str()) {
            return Err(format!("unsupported WordPress version: {wp_version}"));
        }
        if !PHP_VERSIONS.contains(&php_version.as_str()) {
            return Err(format!("unsupported PHP version: {php_version}"));
        }
    }

    let slug = unique_slug(state, &slugify(&name))?;
    let port = free_port(state).await?;
    let dir = site_dir(&state.data_dir, &slug);
    let (connection_id, remote_site_id) = match origin {
        Some((c, r)) => (Some(c), Some(r)),
        None => (None, None),
    };

    let mut site = Site {
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
        connection_id,
        remote_site_id,
        kind,
        config,
        capabilities: Capabilities::default(),
    };
    site.refresh_capabilities();
    {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        db.insert_site(&site)?;
    }
    Ok(site)
}

/// Write a reserved site's project files: directory, compose file, `.env`, and
/// the one-click-login MU plugin.
pub(crate) fn write_project_files(site: &Site) -> Result<(), String> {
    let dir = site.dir();
    std::fs::create_dir_all(dir.join("wp-content"))
        .map_err(|e| format!("failed to create site directory: {e}"))?;
    let db_password = random_password(24);
    std::fs::write(dir.join("docker-compose.yml"), render_compose(site))
        .map_err(|e| format!("failed to write docker-compose.yml: {e}"))?;
    std::fs::write(dir.join(".env"), render_env(site, &db_password))
        .map_err(|e| format!("failed to write .env: {e}"))?;
    wordpress::ensure_login_plugin(&dir)
}

pub async fn create(
    app: Option<&AppHandle>,
    state: &AppState,
    name: String,
    wp_version: String,
    php_version: String,
) -> Result<Site, String> {
    let site = reserve(
        state,
        name,
        KIND_WORDPRESS.to_string(),
        wp_version,
        php_version,
        SiteConfig::default(),
        None,
    )
    .await?;

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
    write_project_files(site)?;

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

// ---------------------------------------------------------------------------
// Clone (plan 20)
// ---------------------------------------------------------------------------

/// Clone an existing local site into a brand-new one.
///
/// Built directly on the plan-17 snapshot engine: the source is snapshotted,
/// a fresh target is provisioned (unique slug, fresh ports, fresh DB password
/// and WP salts — secrets are never copied), and the snapshot's data is laid
/// down on top, then its baked-in URLs are search-replaced to the clone's.
///
/// Emits the same `site-event` stages the create/import flows do, so the
/// progress toast works unchanged: `snapshot` (against the source) →
/// `files` → `containers` → `waiting` → `import` → `done` (against the clone).
pub async fn clone_site(
    app: Option<&AppHandle>,
    state: &AppState,
    source_id: &str,
    new_name: String,
) -> Result<Site, String> {
    let source = get(state, source_id)?;

    // 1. Snapshot the source. This reuses the retry-heavy DB export and the
    //    shared archive format, and gives the clone a consistent point-in-time
    //    copy. `snapshot::create` emits its own `snapshot`-stage progress.
    let snap = crate::snapshot::create(
        app,
        state,
        source_id,
        crate::snapshot::KIND_CLONE_SOURCE,
        Some(format!("cloning {}", source.name)),
    )
    .await
    .map_err(|e| format!("could not snapshot the source site: {e}"))?;

    // 2. Reserve the target: unique slug, fresh ports, `creating` row. Same
    //    versions as the source so the snapshot's DB/plugins land on a matching
    //    stack. A hand-made clone has no remote origin.
    let target = match reserve(
        state,
        new_name,
        source.kind.clone(),
        source.wp_version.clone(),
        source.php_version.clone(),
        source.config.clone(),
        None,
    )
    .await
    {
        Ok(t) => t,
        Err(e) => {
            // Nothing was provisioned yet; just drop the transient snapshot.
            let _ = crate::snapshot::delete(state, source_id, &snap.id);
            return Err(e);
        }
    };

    match do_clone(app, state, &source, &snap.id, &target).await {
        Ok(site) => {
            // The clone_source snapshot is an implementation detail — prune it
            // aggressively the moment it has served its purpose.
            let _ = crate::snapshot::delete(state, source_id, &snap.id);
            let url = router::site_public_url(state, &site);
            emit(
                app,
                &site.id,
                "done",
                &format!("{} cloned from {} — now running at {url}", site.name, source.name),
            );
            Ok(site)
        }
        Err(e) => {
            let _ = crate::snapshot::delete(state, source_id, &snap.id);
            emit(app, &target.id, "error", &format!("Clone failed: {e}"));
            let _ = cleanup(state, &target).await;
            Err(e)
        }
    }
}

/// The provisioning half of a clone: everything after the source snapshot and
/// the target reservation, so a failure here can be cleaned up wholesale.
async fn do_clone(
    app: Option<&AppHandle>,
    state: &AppState,
    source: &Site,
    snapshot_id: &str,
    target: &Site,
) -> Result<Site, String> {
    let dir = target.dir();
    let id = target.id.as_str();

    emit(app, id, "files", "Writing project files...");
    write_project_files(target)?;

    // The source runs the same WP/PHP versions, so its images are already
    // pulled; `compose_up` fetches anything missing rather than stalling
    // silently, but there is normally nothing to fetch.
    emit(app, id, "containers", "Starting Docker containers...");
    docker::compose_up(&dir).await?;

    emit(app, id, "waiting", "Waiting for WordPress to come online...");
    wait_for_port(target.port, 180).await?;
    // The port answering is not the same as WordPress being ready (see
    // `wordpress::wait_for_config`); without this the first wp-cli call races
    // the image entrypoint still writing wp-config.php.
    wordpress::wait_for_config(&dir, 24).await?;

    // 3. Lay the source's database + wp-content down onto the fresh target.
    emit(app, id, "import", &format!("Copying {}'s content...", source.name));
    crate::snapshot::restore_into(state, &source.id, snapshot_id, target).await?;
    // The archive brought the source's mu-plugins over the one just written;
    // one-click login must survive the clone.
    wordpress::ensure_login_plugin(&dir)?;

    // 4. Rewrite the source's baked-in URLs to the clone's own public URL.
    let source_url = router::site_public_url(state, source);
    let target_url = router::site_public_url(state, target);
    emit(app, id, "import", "Rewriting URLs to the clone...");
    wordpress::update_site_urls(&dir, &target_url).await?;
    if source_url != target_url {
        wordpress::search_replace(&dir, &source_url, &target_url).await?;
    }
    // Permalinks are rules tied to the old host; regenerate or every page 404s.
    // Best effort — a rewrite/cache hiccup must not throw away a live clone.
    let _ = docker::compose_run(&dir, "wpcli", &["wp", "rewrite", "flush"]).await;
    let _ = docker::compose_run(&dir, "wpcli", &["wp", "cache", "flush"]).await;

    // 5. The clone's admin login is the source's: the copied database carries
    //    the source's users table, so the source's WP admin password works
    //    here too. (The MySQL/WP secrets in `.env`/wp-config are fresh — those
    //    are what "never copy secrets" refers to.)
    let mut site = target.clone();
    site.status = "running".into();
    site.admin_user = source.admin_user.clone();
    site.admin_pass = source.admin_pass.clone();
    {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        db.set_status(id, "running")?;
        db.update_credentials(id, &site.admin_user, &site.admin_pass)?;
    }
    // A new running site joins the router's Caddyfile + hosts block (no-op when
    // local domains are disabled).
    router::refresh_routes(state).await;
    router::refresh_hosts(state).await;
    Ok(site)
}

/// Undo a partial creation: tear the compose project down, remove the files,
/// and drop the DB row. Shared with the import flow — a failed import must not
/// leave a half-built site on the dashboard.
pub(crate) async fn cleanup(state: &AppState, site: &Site) -> Result<(), String> {
    let dir = site.dir();
    if dir.exists() {
        let _ = docker::compose_down(&dir, true).await;
        let _ = std::fs::remove_dir_all(&dir);
    }
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.delete_site(&site.id)
}

/// Wait until something (Apache) accepts TCP connections on the site port.
pub(crate) async fn wait_for_port(port: u16, timeout_secs: u64) -> Result<(), String> {
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
                    .any(|c| c.service == site.app_service() && c.state == "running")
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

// ---------------------------------------------------------------------------
// Tests — the plan-22 capability matrix + config serde defaults
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// The WordPress reference stack claims every capability — the whole point
    /// of the model is that WP is not an `if` branch but the maximal kind.
    #[test]
    fn wordpress_claims_every_capability() {
        let caps = Capabilities::for_kind(KIND_WORDPRESS, &SiteConfig::default());
        assert_eq!(caps, Capabilities::WORDPRESS);
        for on in [
            caps.domains,
            caps.terminal,
            caps.logs,
            caps.snapshots,
            caps.db_gui,
            caps.db_sync,
            caps.code_sync,
            caps.one_click_login,
            caps.wp_tools,
            caps.search_replace,
        ] {
            assert!(on, "WordPress must claim every capability");
        }
    }

    /// A plain docker app gets lifecycle + domains + terminal + logs +
    /// snapshots (code) but never the WordPress-only affordances.
    #[test]
    fn docker_claims_only_the_generic_capabilities() {
        let caps = Capabilities::for_kind(KIND_DOCKER, &SiteConfig::default());
        assert!(caps.domains && caps.terminal && caps.logs && caps.snapshots && caps.code_sync);
        assert!(
            !caps.db_gui
                && !caps.db_sync
                && !caps.one_click_login
                && !caps.wp_tools
                && !caps.search_replace,
            "a code-only docker app must not claim WordPress capabilities"
        );
    }

    /// A recognized DB engine in a docker app's compose flips on `db_sync` (and
    /// only `db_sync` — not the WP tooling).
    #[test]
    fn a_recognized_db_engine_flips_on_db_sync_only() {
        let config = SiteConfig {
            db_engine: Some("mariadb".into()),
            db_service: Some("db".into()),
            ..SiteConfig::default()
        };
        let caps = Capabilities::for_kind(KIND_DOCKER, &config);
        assert!(caps.db_sync, "a DB engine earns db_sync");
        assert!(!caps.wp_tools && !caps.one_click_login && !caps.search_replace);
    }

    /// An unknown/legacy kind falls back to the fully-capable WordPress set —
    /// the safe default for a row written before this kind existed.
    #[test]
    fn an_unknown_kind_falls_back_to_wordpress() {
        assert_eq!(
            Capabilities::for_kind("something-new", &SiteConfig::default()),
            Capabilities::WORDPRESS
        );
    }

    /// A legacy `config_json` of `{}` (what migration 6 back-fills) deserializes
    /// to the WordPress `SiteConfig` — service `wordpress`, sync path
    /// `wp-content`, no app-port override.
    #[test]
    fn empty_config_json_is_the_wordpress_default() {
        let config: SiteConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(config, SiteConfig::default());
        assert_eq!(config.service, "wordpress");
        assert_eq!(config.sync_path, "wp-content");
        assert_eq!(config.app_port, None);
        assert_eq!(config.upstream_port(8081), 8081);
    }

    /// An explicit app port overrides the site port for the router upstream;
    /// unknown JSON fields are ignored rather than failing the read.
    #[test]
    fn config_serde_round_trips_and_tolerates_extra_fields() {
        let json = r#"{"service":"app","sync_path":"data","app_port":3000,"future_field":true}"#;
        let config: SiteConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.service, "app");
        assert_eq!(config.sync_path, "data");
        assert_eq!(config.upstream_port(8081), 3000);
        // Round-trips back to JSON and parses to the same value.
        let back: SiteConfig = serde_json::from_str(&serde_json::to_string(&config).unwrap()).unwrap();
        assert_eq!(back, config);
    }

    #[test]
    fn require_refuses_a_missing_capability_with_the_kind_named() {
        let mut s = Site {
            id: "d".into(),
            name: "API".into(),
            slug: "api".into(),
            path: "/tmp/api".into(),
            port: 8081,
            wp_version: String::new(),
            php_version: String::new(),
            status: "running".into(),
            admin_user: DEFAULT_ADMIN_USER.into(),
            admin_pass: String::new(),
            created_at: "2026-01-01T00:00:00Z".into(),
            connection_id: None,
            remote_site_id: None,
            kind: KIND_DOCKER.into(),
            config: SiteConfig::default(),
            capabilities: Capabilities::default(),
        };
        s.refresh_capabilities();
        let err = s.require(s.capabilities.wp_tools, "WordPress info").unwrap_err();
        assert!(err.contains("docker"), "the refusal names the kind: {err}");
        assert!(s.require(s.capabilities.terminal, "Terminal").is_ok());
    }
}
