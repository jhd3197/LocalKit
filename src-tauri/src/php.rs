//! Generated PHP/Laravel stack (plan 26).
//!
//! The second first-class multi-stack kind after WordPress. Unlike a
//! bring-your-own docker app (`dockerapp.rs`), LocalKit *generates* the compose
//! project — `app` (php-fpm, built from a tiny Dockerfile that adds `pdo_mysql`
//! + Composer so a fresh Laravel app can talk to the bundled database), `web`
//! (nginx serving the `public/` webroot), `db` (mariadb, same template as the
//! WordPress stack) and a profile-gated `adminer`.
//!
//! Creation is one of two shapes:
//!   * an empty Laravel-ready skeleton (a `public/index.php` webroot the user
//!     then runs `composer create-project` over from the built-in terminal), or
//!   * importing an existing PHP project folder into the site's `app/` directory
//!     (the same ignore-list copy as the docker import).
//!
//! There is no framework installer inside the app — the terminal is right there.
//! The database is synced engine-native (mysqldump/mysql), not via wp-cli, so
//! `php` claims `db_sync` (see `site::Capabilities::PHP` and `dbsync.rs`).

use std::path::{Path, PathBuf};
use tauri::AppHandle;

use crate::{
    docker, dockerapp, router,
    site::{self, SiteConfig},
    AppState,
};

/// Compose service the terminal/logs target and code sync use for a php site.
pub const APP_SERVICE: &str = "app";
/// The php site's code lives here under the site dir (bind-mounted to the app +
/// web containers); this is also its `sync_path`, so snapshots/code-sync archive
/// the application code and not the generated infra files.
pub const APP_DIR: &str = "app";

/// The engine + service of the bundled database — recorded in `SiteConfig` so
/// the engine-native DB sync (plan 26 phase 2) dispatches without re-detecting.
const DB_ENGINE: &str = "mariadb";
const DB_SERVICE: &str = "db";
/// Laravel's conventional database name/user; the app container gets these as
/// `DB_DATABASE`/`DB_USERNAME` so a `.env`-less first run still connects.
const DB_NAME: &str = "laravel";
const DB_USER: &str = "laravel";

/// The `SiteConfig` every php site carries. Deterministic, so a rewrite (Adminer
/// on-demand start) reproduces it exactly. Shared with the import flow (plan 26
/// phase 3), which reserves a php site the same way a fresh create does.
pub(crate) fn config() -> SiteConfig {
    SiteConfig {
        service: APP_SERVICE.to_string(),
        sync_path: APP_DIR.to_string(),
        app_port: None,
        db_engine: Some(DB_ENGINE.to_string()),
        db_service: Some(DB_SERVICE.to_string()),
    }
}

// ---------------------------------------------------------------------------
// Templates
// ---------------------------------------------------------------------------

/// The generated compose project for a php site. Mirrors the WordPress template's
/// conventions (mariadb `db` block, profile-gated `adminer` on `db_port + 1000`),
/// but the app is php-fpm behind nginx instead of the apache wordpress image.
pub fn render_compose(site: &site::Site) -> String {
    format!(
        r#"name: localkit-{slug}

services:
  # php-fpm, built from ./docker/Dockerfile so pdo_mysql + Composer are present
  # (the stock php-fpm image ships neither, and a Laravel app needs both).
  app:
    build:
      context: ./docker
      dockerfile: Dockerfile
    restart: unless-stopped
    working_dir: /var/www/html
    volumes:
      - ./{app_dir}:/var/www/html
    environment:
      DB_CONNECTION: mysql
      DB_HOST: db
      DB_PORT: "3306"
      DB_DATABASE: ${{DB_NAME}}
      DB_USERNAME: ${{DB_USER}}
      DB_PASSWORD: ${{DB_PASSWORD}}
    depends_on:
      db:
        condition: service_healthy

  web:
    image: nginx:alpine
    restart: unless-stopped
    ports:
      - "${{WEB_PORT}}:80"
    volumes:
      - ./{app_dir}:/var/www/html
      - ./nginx.conf:/etc/nginx/conf.d/default.conf:ro
    depends_on:
      - app

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

  # Adminer database GUI (plan 24) — profile-gated + off by default, started on
  # demand from Tools -> Database. Deterministic host port (db_port + 1000).
  adminer:
    image: adminer:4-standalone
    profiles: ["tools"]
    restart: unless-stopped
    ports:
      - "{adminer_port}:8080"
    environment:
      ADMINER_DEFAULT_SERVER: db
    depends_on:
      db:
        condition: service_healthy

volumes:
  db-data:
"#,
        slug = site.slug,
        app_dir = APP_DIR,
        adminer_port = site.adminer_port(),
    )
}

/// The php-fpm image, built once per site. `FROM php:<ver>-fpm` keeps the app on
/// the allowlisted PHP version (`PHP_VERSIONS`), then adds only what a Laravel
/// app can't run without: `pdo_mysql` for the bundled mariadb, `zip`/`unzip`/git
/// for `composer create-project`, and Composer itself. Exotic extensions are a
/// documented "edit ./docker/Dockerfile" path (plan 26 risks).
fn render_dockerfile(php_version: &str) -> String {
    format!(
        r#"# Generated by LocalKit (plan 26). Edit to add PHP extensions your app needs.
FROM php:{php}-fpm

RUN set -eux; \
    apt-get update; \
    apt-get install -y --no-install-recommends git unzip libzip-dev; \
    docker-php-ext-install pdo_mysql zip; \
    rm -rf /var/lib/apt/lists/*

COPY --from=composer:2 /usr/bin/composer /usr/bin/composer
"#,
        php = php_version,
    )
}

/// nginx vhost pointing at `webroot` (the `public/` dir for a Laravel-style
/// project, else the app root). Standard `try_files ... /index.php` front
/// controller so pretty URLs work out of the box.
fn render_nginx(webroot: &str) -> String {
    format!(
        r#"server {{
    listen 80;
    server_name _;
    root {webroot};
    index index.php index.html;
    client_max_body_size 64m;

    location / {{
        try_files $uri $uri/ /index.php?$query_string;
    }}

    location ~ \.php$ {{
        fastcgi_pass app:9000;
        fastcgi_index index.php;
        include fastcgi_params;
        fastcgi_param SCRIPT_FILENAME $document_root$fastcgi_script_name;
    }}

    location ~ /\.(?!well-known).* {{
        deny all;
    }}
}}
"#,
        webroot = webroot,
    )
}

fn render_env(site: &site::Site, db_password: &str) -> String {
    format!(
        "WEB_PORT={}\nDB_PORT={}\nDB_NAME={DB_NAME}\nDB_USER={DB_USER}\nDB_PASSWORD={}\n",
        site.port,
        site.db_port(),
        db_password,
    )
}

/// The skeleton webroot page for an empty create: confirms the stack works and
/// checks database connectivity, so the site answers with something real the
/// moment the containers come up (before the user has run Composer).
const SKELETON_INDEX: &str = r#"<?php
// LocalKit PHP/Laravel starter. Replace this with your app
// (e.g. `composer create-project laravel/laravel .` from the terminal).
$connected = false;
$err = '';
try {
    $pdo = new PDO(
        sprintf('mysql:host=%s;dbname=%s', getenv('DB_HOST') ?: 'db', getenv('DB_DATABASE') ?: 'laravel'),
        getenv('DB_USERNAME') ?: 'laravel',
        getenv('DB_PASSWORD') ?: '',
        [PDO::ATTR_TIMEOUT => 3]
    );
    $connected = true;
} catch (Throwable $e) {
    $err = $e->getMessage();
}
?>
<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>LocalKit PHP site</title>
  <style>
    body { font-family: system-ui, sans-serif; background: #0D0F16; color: #E7E9F0;
           display: grid; place-items: center; min-height: 100vh; margin: 0; }
    .card { background: #151822; border: 1px solid #2A2F40; border-radius: 16px;
            padding: 2.5rem 3rem; max-width: 32rem; }
    h1 { margin: 0 0 .5rem; font-size: 1.4rem; }
    code { background: #0D0F16; padding: .15rem .4rem; border-radius: 6px; color: #B8AFFA; }
    .ok { color: #34d399; } .bad { color: #f87171; }
    p { line-height: 1.6; color: #9097AB; }
  </style>
</head>
<body>
  <div class="card">
    <h1>Your PHP stack is running 🎉</h1>
    <p>PHP <?= htmlspecialchars(PHP_VERSION) ?> via php-fpm + nginx.</p>
    <p>Database:
      <?php if ($connected): ?>
        <span class="ok">connected</span>
      <?php else: ?>
        <span class="bad">not reachable</span> — <?= htmlspecialchars($err) ?>
      <?php endif; ?>
    </p>
    <p>Drop your code into the <code>app/</code> directory, or run
       <code>composer create-project laravel/laravel .</code> from the terminal.</p>
  </div>
</body>
</html>
"#;

// ---------------------------------------------------------------------------
// Create
// ---------------------------------------------------------------------------

/// Create a new PHP/Laravel site.
///
/// `source` is `None` for an empty Laravel-ready skeleton, or a folder to import
/// existing code from (copied into the site's `app/` dir, docker-import excludes
/// applied unless `include_all`). Emits the same `site-event` stages the other
/// create flows do so the progress toast works unchanged.
pub async fn create_php_site(
    app: Option<&AppHandle>,
    state: &AppState,
    name: String,
    php_version: String,
    source: Option<PathBuf>,
    include_all: bool,
) -> Result<site::Site, String> {
    if !site::PHP_VERSIONS.contains(&php_version.as_str()) {
        return Err(format!("unsupported PHP version: {php_version}"));
    }
    if let Some(src) = source.as_deref() {
        if !src.is_dir() {
            return Err(format!("{} is not a folder", src.display()));
        }
    }

    let site = site::reserve(
        state,
        name,
        site::KIND_PHP.to_string(),
        String::new(),
        php_version,
        config(),
        None,
    )
    .await?;

    // Own this site's status until the create finishes (plan 23).
    let _guard = state.in_flight.guard(&site.id);
    match do_create(app, state, &site, source.as_deref(), include_all).await {
        Ok(site) => {
            let url = router::site_public_url(state, &site);
            site::emit(
                app,
                &site.id,
                "done",
                &format!("{} is ready at {url}", site.name),
            );
            Ok(site)
        }
        Err(e) => {
            site::emit(app, &site.id, "error", &format!("Creation failed: {e}"));
            let _ = site::cleanup(state, &site).await;
            Err(e)
        }
    }
}

async fn do_create(
    app: Option<&AppHandle>,
    state: &AppState,
    site: &site::Site,
    source: Option<&Path>,
    include_all: bool,
) -> Result<site::Site, String> {
    let dir = site.dir();
    let id = site.id.as_str();

    site::emit(app, id, "files", "Writing project files...");
    write_project_files(site, source, include_all)?;

    site::emit(
        app,
        id,
        "pulling",
        "Building the PHP image (first run can take a few minutes)...",
    );
    // Pull the base images up front for a labeled stage; best-effort because
    // `build`/`up` fetch anything still missing anyway.
    let _ = docker::compose_pull(&dir, &["web", "db"]).await;
    docker::compose_build(&dir).await?;

    site::emit(app, id, "containers", "Starting containers...");
    docker::compose_up(&dir).await?;

    // The web port answering means nginx is up; a php app may still be a blank
    // skeleton, so don't fail the create if it never responds (mirrors docker).
    site::emit(app, id, "waiting", "Waiting for the app to come online...");
    let _ = site::wait_for_port(site.port, 180).await;

    let mut running = site.clone();
    running.status = "running".into();
    {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        db.set_status(id, "running")?;
    }
    // Last step: the completion marker (plan 23) — its absence flags a killed
    // create.
    site::mark_complete(&dir);
    router::refresh_routes(state).await;
    router::refresh_hosts(state).await;
    Ok(running)
}

/// Write a php site's generated files: the `app/` code directory (an empty
/// skeleton or a copy of `source`) plus the generated infra (`docker/Dockerfile`,
/// `nginx.conf`, `docker-compose.yml`, `.env`).
fn write_project_files(
    site: &site::Site,
    source: Option<&Path>,
    include_all: bool,
) -> Result<(), String> {
    let app_dir = ensure_dirs(site)?;
    match source {
        Some(src) => {
            let excludes: &[&str] = if include_all { &[] } else { dockerapp::DEFAULT_EXCLUDES };
            dockerapp::copy_tree(src, &app_dir, excludes)?;
        }
        None => {
            let public = app_dir.join("public");
            std::fs::create_dir_all(&public)
                .map_err(|e| format!("failed to create the webroot: {e}"))?;
            std::fs::write(public.join("index.php"), SKELETON_INDEX)
                .map_err(|e| format!("failed to write the skeleton index: {e}"))?;
        }
    }
    write_infra(site)
}

/// Create a php site's directory skeleton: the site dir, `docker/`, and an empty
/// `app/`. Returns the `app/` path. Shared by create and import (plan 26 phase 3
/// extracts the remote code into `app/` between this and `write_infra`).
pub(crate) fn ensure_dirs(site: &site::Site) -> Result<PathBuf, String> {
    let dir = site.dir();
    std::fs::create_dir_all(dir.join("docker"))
        .map_err(|e| format!("failed to create the project directory: {e}"))?;
    let app_dir = dir.join(APP_DIR);
    std::fs::create_dir_all(&app_dir)
        .map_err(|e| format!("failed to create the app directory: {e}"))?;
    Ok(app_dir)
}

/// Write the generated infra files (Dockerfile, nginx.conf, compose, `.env`).
/// Called *after* `app/` is populated so the nginx webroot is detected from the
/// real project layout — a Laravel-style `public/` serves from there, a plain
/// PHP project without one serves from the app root.
pub(crate) fn write_infra(site: &site::Site) -> Result<(), String> {
    let dir = site.dir();
    let app_dir = dir.join(APP_DIR);
    let webroot = if app_dir.join("public").is_dir() {
        "/var/www/html/public"
    } else {
        "/var/www/html"
    };
    let db_password = site::random_password(24);
    std::fs::write(dir.join("docker").join("Dockerfile"), render_dockerfile(&site.php_version))
        .map_err(|e| format!("failed to write the Dockerfile: {e}"))?;
    std::fs::write(dir.join("nginx.conf"), render_nginx(webroot))
        .map_err(|e| format!("failed to write nginx.conf: {e}"))?;
    std::fs::write(dir.join("docker-compose.yml"), render_compose(site))
        .map_err(|e| format!("failed to write docker-compose.yml: {e}"))?;
    std::fs::write(dir.join(".env"), render_env(site, &db_password))
        .map_err(|e| format!("failed to write .env: {e}"))?;
    Ok(())
}

/// Best-effort patch of `APP_URL` in the app's own `.env` (Laravel convention)
/// after a pull/import (plan 26). Unlike WordPress there is no serialization-safe
/// search-replace to run — URL config is the app's own concern — so this only
/// touches the one well-known key, and only if an `app/.env` exists. Never fails
/// the sync: a php app may not be Laravel, or may have no `.env` at all.
pub fn patch_app_url(site_dir: &Path, sync_path: &str, url: &str) {
    let env_path = site_dir.join(sync_path).join(".env");
    let Ok(existing) = std::fs::read_to_string(&env_path) else {
        return; // no app/.env — nothing to patch
    };
    let mut replaced = false;
    let mut out: Vec<String> = existing
        .lines()
        .map(|line| {
            if line.trim_start().starts_with("APP_URL=") {
                replaced = true;
                format!("APP_URL={url}")
            } else {
                line.to_string()
            }
        })
        .collect();
    if !replaced {
        out.push(format!("APP_URL={url}"));
    }
    let mut body = out.join("\n");
    if existing.ends_with('\n') {
        body.push('\n');
    }
    let _ = std::fs::write(&env_path, body);
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::site::{Capabilities, Site};

    fn php_site() -> Site {
        let mut s = Site {
            id: "p".into(),
            name: "Shop".into(),
            slug: "shop".into(),
            path: "/tmp/shop".into(),
            port: 8081,
            wp_version: String::new(),
            php_version: "8.3".into(),
            status: "running".into(),
            status_updated_at: "2026-01-01T00:00:00Z".into(),
            admin_user: String::new(),
            admin_pass: String::new(),
            created_at: "2026-01-01T00:00:00Z".into(),
            connection_id: None,
            remote_site_id: None,
            kind: site::KIND_PHP.into(),
            config: config(),
            capabilities: Capabilities::default(),
        };
        s.refresh_capabilities();
        s
    }

    #[test]
    fn compose_has_the_php_web_and_db_services_plus_profile_gated_adminer() {
        let yml = render_compose(&php_site());
        assert!(yml.contains("name: localkit-shop"), "{yml}");
        assert!(yml.contains("dockerfile: Dockerfile"), "app builds from a Dockerfile");
        assert!(yml.contains("image: nginx:alpine"), "web service");
        assert!(yml.contains("image: mariadb:11"), "db service");
        // Adminer is profile-gated on db_port + 1000 (18081 + 1000).
        assert!(yml.contains("\"19081:8080\""), "adminer host port:\n{yml}");
        assert_eq!(yml.matches("profiles: [\"tools\"]").count(), 1, "only adminer is gated");
    }

    #[test]
    fn env_records_laravel_db_credentials_and_the_ports() {
        let env = render_env(&php_site(), "s3cret");
        assert!(env.contains("WEB_PORT=8081"));
        assert!(env.contains("DB_PORT=18081"));
        assert!(env.contains("DB_NAME=laravel"));
        assert!(env.contains("DB_USER=laravel"));
        assert!(env.contains("DB_PASSWORD=s3cret"));
    }

    #[test]
    fn dockerfile_pins_the_php_version_and_adds_pdo_mysql_and_composer() {
        let df = render_dockerfile("8.2");
        assert!(df.contains("FROM php:8.2-fpm"));
        assert!(df.contains("docker-php-ext-install pdo_mysql"));
        assert!(df.contains("composer:2"));
    }

    #[test]
    fn nginx_points_at_the_given_webroot_with_a_front_controller() {
        let conf = render_nginx("/var/www/html/public");
        assert!(conf.contains("root /var/www/html/public;"));
        assert!(conf.contains("try_files $uri $uri/ /index.php?$query_string;"));
        assert!(conf.contains("fastcgi_pass app:9000;"));
    }

    #[test]
    fn patch_app_url_replaces_or_appends_only_when_an_app_env_exists() {
        let base = std::env::temp_dir().join(format!("lk-php-appurl-{}", std::process::id()));
        let app = base.join("app");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&app).unwrap();

        // No app/.env → no-op, no panic, no file created.
        patch_app_url(&base, "app", "http://shop.test");
        assert!(!app.join(".env").exists());

        // Existing APP_URL is replaced; other keys are untouched.
        std::fs::write(app.join(".env"), "APP_NAME=Shop\nAPP_URL=http://old\nDB_HOST=db\n").unwrap();
        patch_app_url(&base, "app", "http://shop.test");
        let env = std::fs::read_to_string(app.join(".env")).unwrap();
        assert!(env.contains("APP_URL=http://shop.test"));
        assert!(!env.contains("http://old"));
        assert!(env.contains("APP_NAME=Shop") && env.contains("DB_HOST=db"));

        // Missing APP_URL is appended.
        std::fs::write(app.join(".env"), "APP_NAME=Shop\n").unwrap();
        patch_app_url(&base, "app", "http://shop.test");
        let env = std::fs::read_to_string(app.join(".env")).unwrap();
        assert!(env.contains("APP_URL=http://shop.test"));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn config_is_the_deterministic_php_shape() {
        let cfg = config();
        assert_eq!(cfg.service, "app");
        assert_eq!(cfg.sync_path, "app");
        assert_eq!(cfg.db_engine.as_deref(), Some("mariadb"));
        assert_eq!(cfg.db_service.as_deref(), Some("db"));
        assert_eq!(cfg.app_port, None);
    }
}
