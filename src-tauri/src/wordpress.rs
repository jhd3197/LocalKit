//! WordPress management via wp-cli.
//!
//! The stock `wordpress` image does NOT bundle wp-cli, so each site compose
//! file includes a profile-gated `wpcli` service (the official
//! `wordpress:cli` image) and we run commands with
//! `docker compose run --rm -T wpcli wp <args...>`.

use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::docker;
use crate::site::Site;

#[derive(Debug, Clone, Serialize)]
pub struct PluginInfo {
    pub name: String,
    pub status: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct WpInfo {
    pub core_version: String,
    pub plugins: Vec<PluginInfo>,
}

#[derive(Deserialize)]
struct RawPlugin {
    name: String,
    status: String,
    #[serde(default)]
    version: String,
}

/// Run wp-cli inside the site project. The `wordpress:cli` image entrypoint
/// expects `wp` as the first argument (its `wp` CMD is replaced by run args).
async fn wp(dir: &Path, args: &[&str]) -> Result<String, String> {
    let mut full: Vec<&str> = Vec::with_capacity(args.len() + 1);
    full.push("wp");
    full.extend_from_slice(args);
    docker::compose_run(dir, "wpcli", &full).await
}

/// Auto-install WordPress with generated admin credentials.
/// `url` is the public URL the site will be reached at (localhost:<port> or,
/// when local domains are enabled, http(s)://<slug>.test).
/// Retries while the containers/DB finish their first boot.
pub async fn install(dir: &Path, site: &Site, admin_pass: &str, url: &str) -> Result<(), String> {
    let url_arg = format!("--url={url}");
    let title_arg = format!("--title={}", site.name);
    let user_arg = format!("--admin_user={}", site.admin_user);
    let pass_arg = format!("--admin_password={admin_pass}");
    let email_arg = format!("--admin_email=admin@{}.local", site.slug);
    let install_args = [
        "core",
        "install",
        &url_arg,
        &title_arg,
        &user_arg,
        &pass_arg,
        &email_arg,
        "--skip-email",
    ];

    let mut last_err = String::new();
    for _ in 0..12 {
        // Already installed? Nothing to do.
        if wp(dir, &["core", "is-installed"]).await.is_ok() {
            return Ok(());
        }
        match wp(dir, &install_args).await {
            Ok(_) => return Ok(()),
            Err(e) => {
                last_err = e;
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            }
        }
    }
    Err(format!("WordPress install failed: {last_err}"))
}

/// Read-only info for the UI: core version + plugin list.
pub async fn info(dir: &Path) -> Result<WpInfo, String> {
    let core_version = wp(dir, &["core", "version"]).await?.trim().to_string();
    let plugins_json = wp(dir, &["plugin", "list", "--format=json"]).await?;
    let raw: Vec<RawPlugin> = serde_json::from_str(&plugins_json)
        .map_err(|e| format!("failed to parse plugin list: {e}"))?;
    Ok(WpInfo {
        core_version,
        plugins: raw
            .into_iter()
            .map(|p| PluginInfo {
                name: p.name,
                status: p.status,
                version: p.version,
            })
            .collect(),
    })
}

/// Export the site database to a SQL file on the host.
pub async fn export_db(dir: &Path, dest: &Path) -> Result<(), String> {
    let sql = wp(dir, &["db", "export", "-"]).await?;
    std::fs::write(dest, sql).map_err(|e| format!("failed to write database dump: {e}"))
}

/// Import a SQL dump through wp-cli's stdin (`wp db import -`).
pub async fn import_db(dir: &Path, sql: &[u8]) -> Result<(), String> {
    docker::compose_run_stdin(dir, "wpcli", &["wp", "db", "import", "-"], sql)
        .await
        .map(|_| ())
}

/// Serialization-safe URL rewrite across all tables.
pub async fn search_replace(dir: &Path, from: &str, to: &str) -> Result<(), String> {
    wp(dir, &["search-replace", from, to, "--all-tables"])
        .await
        .map(|_| ())
}

/// Point home/siteurl at the site's local URL.
pub async fn update_site_urls(dir: &Path, url: &str) -> Result<(), String> {
    wp(dir, &["option", "update", "home", url]).await?;
    wp(dir, &["option", "update", "siteurl", url]).await?;
    Ok(())
}
