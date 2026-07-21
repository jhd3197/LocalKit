pub mod blueprint;
pub mod db;
pub mod docker;
pub mod dockerapp;
pub mod reconcile;
pub mod router;
pub mod serverkit;
pub mod site;
pub mod snapshot;
pub mod sync;
pub mod terminal;
pub mod transfer;
pub mod tray;
pub mod wordpress;

use std::path::PathBuf;
use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use db::Db;
use site::{Site, SiteDetail, SiteWithStatus};

pub struct AppState {
    pub db: Mutex<Db>,
    pub data_dir: PathBuf,
    pub terminals: terminal::PtyManager,
    /// Cancel flags for in-flight chunked syncs, keyed by site id (plan 19).
    pub transfers: transfer::CancelRegistry,
    /// Sites with an in-flight lifecycle command, shared with every command
    /// path so the reconciler skips them (plan 23).
    pub in_flight: reconcile::InFlight,
}

/// The base capability set advertised for a kind (plan 22). `docker`'s
/// `db_sync` is the code-only default here — it flips on per-site when a
/// recognized DB engine is in the compose.
#[derive(Serialize)]
struct KindInfo {
    kind: String,
    capabilities: site::Capabilities,
}

#[derive(Serialize)]
struct AppInfo {
    data_dir: String,
    sites_dir: String,
    wp_versions: Vec<String>,
    php_versions: Vec<String>,
    /// Every site kind and the capabilities it claims, so both frontends can
    /// gate UI on the same matrix the backend enforces.
    kinds: Vec<KindInfo>,
}

#[tauri::command]
async fn check_docker() -> docker::DockerStatus {
    docker::check().await
}

#[tauri::command]
fn app_info(state: State<AppState>) -> AppInfo {
    AppInfo {
        data_dir: state.data_dir.to_string_lossy().to_string(),
        sites_dir: state.data_dir.join("sites").to_string_lossy().to_string(),
        wp_versions: site::WP_VERSIONS.iter().map(|s| s.to_string()).collect(),
        php_versions: site::PHP_VERSIONS.iter().map(|s| s.to_string()).collect(),
        kinds: vec![
            KindInfo {
                kind: site::KIND_WORDPRESS.to_string(),
                capabilities: site::Capabilities::WORDPRESS,
            },
            KindInfo {
                kind: site::KIND_DOCKER.to_string(),
                capabilities: site::Capabilities::DOCKER,
            },
        ],
    }
}

#[tauri::command]
async fn list_sites(state: State<'_, AppState>) -> Result<Vec<SiteWithStatus>, String> {
    site::list(&state).await
}

#[tauri::command]
fn get_site(state: State<AppState>, id: String) -> Result<SiteDetail, String> {
    site::detail(&state, &id)
}

#[tauri::command]
async fn create_site(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
    wp_version: String,
    php_version: String,
) -> Result<Site, String> {
    let site = site::create(Some(&app), &state, name, wp_version, php_version).await?;
    tray::refresh(&app);
    Ok(site)
}

/// Inspect a folder as a candidate Docker project (plan 22): its services,
/// suggested app service + port, DB engine, and copy size. Read-only.
#[tauri::command]
async fn inspect_docker_project(
    path: String,
) -> Result<dockerapp::DockerProjectInspection, String> {
    dockerapp::inspect(std::path::Path::new(&path)).await
}

/// Import a Docker project as a new local site (plan 22): copy the folder into a
/// managed site dir, record the app service/port, and bring it up.
#[tauri::command]
async fn import_docker_project(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
    path: String,
    service: String,
    app_port: u16,
    include_all: Option<bool>,
) -> Result<Site, String> {
    let site = dockerapp::import_project(
        Some(&app),
        &state,
        name,
        std::path::PathBuf::from(path),
        service,
        app_port,
        include_all.unwrap_or(false),
    )
    .await?;
    tray::refresh(&app);
    Ok(site)
}

/// Clone an existing local site into a brand-new one (plan 20).
#[tauri::command]
async fn clone_site(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    new_name: String,
) -> Result<Site, String> {
    let site = site::clone_site(Some(&app), &state, &id, new_name).await?;
    // A new running site has to reach the tray menu like any other.
    tray::refresh(&app);
    Ok(site)
}

#[tauri::command]
async fn start_site(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<Site, String> {
    let site = site::start(&state, &id).await?;
    tray::refresh(&app);
    Ok(site)
}

#[tauri::command]
async fn stop_site(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<Site, String> {
    let site = site::stop(&state, &id).await?;
    tray::refresh(&app);
    Ok(site)
}

#[tauri::command]
async fn delete_site(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    delete_snapshots: Option<bool>,
) -> Result<(), String> {
    // Default: keep the snapshots (including the pre_delete one this takes).
    site::delete(Some(&app), &state, &id, delete_snapshots.unwrap_or(false)).await?;
    tray::refresh(&app);
    Ok(())
}

#[tauri::command]
async fn site_logs(state: State<'_, AppState>, id: String, tail: Option<u32>) -> Result<String, String> {
    site::logs(&state, &id, tail.unwrap_or(200)).await
}

#[tauri::command]
async fn wp_cli_info(state: State<'_, AppState>, id: String) -> Result<wordpress::WpInfo, String> {
    let s = site::get(&state, &id)?;
    s.require(s.capabilities.wp_tools, "WordPress info")?;
    wordpress::info(&s.dir()).await
}

// ---------------------------------------------------------------------------
// One-click WP Admin login (one-time token + MU plugin)
// ---------------------------------------------------------------------------

#[tauri::command]
async fn login_site(
    state: State<'_, AppState>,
    id: String,
    user_id: Option<u64>,
) -> Result<String, String> {
    let s = site::get(&state, &id)?;
    s.require(s.capabilities.one_click_login, "One-click login")?;
    let containers = docker::compose_ps(&s.dir()).await?;
    if !containers
        .iter()
        .any(|c| c.service == s.app_service() && c.state == "running")
    {
        return Err(format!(
            "\"{}\" is not running — start the site first.",
            s.name
        ));
    }
    let base = router::site_public_url(&state, &s);
    let user = user_id.map(|n| n.to_string());
    wordpress::login_url(&s.dir(), &s, user.as_deref(), &base).await
}

#[tauri::command]
async fn site_wp_users(
    state: State<'_, AppState>,
    id: String,
) -> Result<Vec<wordpress::WpUser>, String> {
    let s = site::get(&state, &id)?;
    s.require(s.capabilities.one_click_login, "The WordPress user list")?;
    wordpress::users(&s.dir()).await
}

// ---------------------------------------------------------------------------
// Snapshots (plan 17) — point-in-time DB + wp-content copies with restore
// ---------------------------------------------------------------------------

#[tauri::command]
fn list_snapshots(state: State<AppState>, site_id: String) -> Result<Vec<snapshot::Snapshot>, String> {
    snapshot::list(&state, &site_id)
}

#[tauri::command]
async fn create_snapshot(
    app: AppHandle,
    state: State<'_, AppState>,
    site_id: String,
    note: Option<String>,
) -> Result<snapshot::Snapshot, String> {
    // Standalone snapshots own their terminal event: `create` deliberately
    // stays silent on done/error so it can nest inside push/pull/delete.
    match snapshot::create(Some(&app), &state, &site_id, snapshot::KIND_MANUAL, note).await {
        Ok(snap) => {
            site::emit(
                Some(&app),
                &site_id,
                "done",
                &format!("Snapshot of {} taken", snap.site_name),
            );
            Ok(snap)
        }
        Err(e) => {
            site::emit(Some(&app), &site_id, "error", &format!("Snapshot failed: {e}"));
            Err(e)
        }
    }
}

#[tauri::command]
async fn restore_snapshot(
    app: AppHandle,
    state: State<'_, AppState>,
    site_id: String,
    snapshot_id: String,
) -> Result<(), String> {
    let result = snapshot::restore(Some(&app), &state, &site_id, &snapshot_id).await;
    // Restore can auto-start a stopped site, so the tray must be rebuilt
    // either way (a failure may still have started it).
    tray::refresh(&app);
    match result {
        Ok(message) => {
            site::emit(Some(&app), &site_id, "done", &message);
            Ok(())
        }
        Err(e) => {
            site::emit(Some(&app), &site_id, "error", &format!("Restore failed: {e}"));
            Err(e)
        }
    }
}

#[tauri::command]
fn delete_snapshot(state: State<AppState>, site_id: String, snapshot_id: String) -> Result<(), String> {
    snapshot::delete(&state, &site_id, &snapshot_id)
}

// ---------------------------------------------------------------------------
// Blueprints (plan 20) — save a site as a reusable template, create from one
// ---------------------------------------------------------------------------

#[tauri::command]
async fn save_blueprint(
    app: AppHandle,
    state: State<'_, AppState>,
    site_id: String,
    name: String,
    description: Option<String>,
) -> Result<blueprint::Blueprint, String> {
    blueprint::save(Some(&app), &state, &site_id, name, description).await
}

#[tauri::command]
fn list_blueprints(state: State<AppState>) -> Result<Vec<blueprint::Blueprint>, String> {
    blueprint::list(&state)
}

#[tauri::command]
fn delete_blueprint(state: State<AppState>, id: String) -> Result<(), String> {
    blueprint::delete(&state, &id)
}

#[tauri::command]
async fn create_site_from_blueprint(
    app: AppHandle,
    state: State<'_, AppState>,
    blueprint_id: String,
    name: Option<String>,
) -> Result<Site, String> {
    let site = blueprint::create_site(Some(&app), &state, &blueprint_id, name).await?;
    // A new running site has to reach the tray menu like any other.
    tray::refresh(&app);
    Ok(site)
}

// ---------------------------------------------------------------------------
// ServerKit connections (M3, read-only)
// ---------------------------------------------------------------------------

#[tauri::command]
fn save_serverkit_connection(
    state: State<AppState>,
    label: String,
    url: String,
    api_key: String,
) -> Result<serverkit::ServerKitConnection, String> {
    let label = label.trim().to_string();
    if label.is_empty() {
        return Err("Label is required".into());
    }
    if api_key.trim().is_empty() {
        return Err("API key is required".into());
    }
    let conn = serverkit::ServerKitConnection {
        id: uuid::Uuid::new_v4().to_string(),
        label,
        url: serverkit::normalize_base_url(&url)?,
        api_key: api_key.trim().to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.insert_connection(&conn)?;
    Ok(conn)
}

#[tauri::command]
fn list_serverkit_connections(
    state: State<AppState>,
) -> Result<Vec<serverkit::ServerKitConnection>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.list_connections()
}

#[tauri::command]
fn delete_serverkit_connection(state: State<AppState>, id: String) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.delete_connection(&id)
}

#[tauri::command]
async fn test_serverkit_connection(
    url: String,
    api_key: String,
) -> Result<serverkit::ServerKitInfo, String> {
    serverkit::test_connection(&url, &api_key).await
}

#[tauri::command]
async fn list_remote_wp_sites(
    state: State<'_, AppState>,
    id: String,
) -> Result<Vec<serverkit::RemoteWpSite>, String> {
    let conn = {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        db.get_connection(&id)?
    };
    serverkit::list_wp_sites(&conn.url, &conn.api_key).await
}

#[tauri::command]
async fn create_remote_site(
    state: State<'_, AppState>,
    connection_id: String,
    name: String,
) -> Result<serde_json::Value, String> {
    let conn = {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        db.get_connection(&connection_id)?
    };
    serverkit::create_remote_site(&conn.url, &conn.api_key, &name).await
}

#[tauri::command]
async fn push_site_code(
    app: AppHandle,
    state: State<'_, AppState>,
    connection_id: String,
    site_id: String,
    remote_site_id: i64,
) -> Result<(), String> {
    sync::push_code(Some(&app), &state, &connection_id, &site_id, remote_site_id).await
}

#[tauri::command]
async fn push_site_db(
    app: AppHandle,
    state: State<'_, AppState>,
    connection_id: String,
    site_id: String,
    remote_site_id: i64,
) -> Result<(), String> {
    sync::push_db(Some(&app), &state, &connection_id, &site_id, remote_site_id).await
}

#[tauri::command]
async fn pull_site_db(
    app: AppHandle,
    state: State<'_, AppState>,
    connection_id: String,
    site_id: String,
    remote_site_id: i64,
    remote_url: Option<String>,
) -> Result<(), String> {
    sync::pull_db(Some(&app), &state, &connection_id, &site_id, remote_site_id, remote_url).await
}

#[tauri::command]
fn list_sync_history(state: State<AppState>, site_id: String) -> Result<Vec<sync::SyncRecord>, String> {
    sync::history(&state, &site_id)
}

/// Ask the in-flight chunked sync for a site to stop (plan 19).
///
/// Returns whether there was one to cancel. The transfer notices between
/// chunks and unwinds through the normal error path; nothing on the server is
/// half-applied, because processing only ever runs after a completed upload
/// verifies its hash.
#[tauri::command]
fn cancel_sync(state: State<AppState>, site_id: String) -> bool {
    state.transfers.cancel(&site_id)
}

/// Clone a remote ServerKit site down as a brand-new local site (plan 18).
#[tauri::command]
async fn import_remote_site(
    app: AppHandle,
    state: State<'_, AppState>,
    connection_id: String,
    remote_site_id: i64,
    name: Option<String>,
) -> Result<Site, String> {
    let site = sync::import_site(Some(&app), &state, &connection_id, remote_site_id, name).await?;
    // A new running site has to reach the tray menu like any other.
    tray::refresh(&app);
    Ok(site)
}

// ---------------------------------------------------------------------------
// Local domains (M6) — shared Caddy router on ports 80/443 (configurable
// since plan 16, for coexistence with LocalWP & other port-80 owners)
// ---------------------------------------------------------------------------

#[tauri::command]
async fn router_status(state: State<'_, AppState>) -> Result<router::RouterStatus, String> {
    router::status(&state).await
}

#[tauri::command]
async fn set_router_ports(
    state: State<'_, AppState>,
    http: u16,
    https: u16,
) -> Result<router::RouterStatus, String> {
    router::set_ports(&state, http, https).await
}

#[tauri::command]
async fn set_domains_enabled(
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<router::RouterStatus, String> {
    router::set_enabled(&state, enabled).await
}

#[tauri::command]
async fn trust_router_ca(state: State<'_, AppState>) -> Result<router::RouterStatus, String> {
    router::trust_ca(&state).await
}

// ---------------------------------------------------------------------------
// App settings (generic KV over app_settings; used by the tray toggle)
// ---------------------------------------------------------------------------

#[tauri::command]
fn get_app_setting(state: State<AppState>, key: String) -> Result<Option<String>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.get_setting(&key)
}

#[tauri::command]
fn settings_get_all(
    state: State<AppState>,
) -> Result<std::collections::HashMap<String, String>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.get_all_settings()
}

#[tauri::command]
fn set_app_setting(state: State<AppState>, key: String, value: String) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.set_setting(&key, &value)
}

#[tauri::command]
fn delete_app_setting(state: State<AppState>, key: String) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.delete_setting(&key)
}

// ---------------------------------------------------------------------------
// Terminals (interactive shells inside site containers)
// ---------------------------------------------------------------------------

#[tauri::command]
async fn terminal_open(
    app: AppHandle,
    state: State<'_, AppState>,
    site_id: String,
    cols: Option<u32>,
    rows: Option<u32>,
) -> Result<String, String> {
    let site = site::get(&state, &site_id)?;
    site.require(site.capabilities.terminal, "Opening a terminal")?;
    let containers = docker::compose_ps(&site.dir()).await?;
    let running = containers
        .iter()
        .any(|c| c.service == site.app_service() && c.state == "running");
    if !running {
        return Err(format!(
            "\"{}\" is not running — start the site first.",
            site.name
        ));
    }
    state
        .terminals
        .open(&app, &site.dir(), site.app_service(), cols.unwrap_or(80), rows.unwrap_or(24))
}

#[tauri::command]
fn terminal_write(state: State<AppState>, terminal_id: String, data: String) -> Result<(), String> {
    state.terminals.write(&terminal_id, &data)
}

#[tauri::command]
fn terminal_resize(
    state: State<AppState>,
    terminal_id: String,
    cols: u32,
    rows: u32,
) -> Result<(), String> {
    state.terminals.resize(&terminal_id, cols, rows)
}

#[tauri::command]
fn terminal_close(state: State<AppState>, terminal_id: String) -> Result<(), String> {
    state.terminals.close(&terminal_id)
}

// ---------------------------------------------------------------------------
// Pre-paint settings injection (plan 13)
// ---------------------------------------------------------------------------

/// JS run before any frontend code: publishes the app_settings KV as
/// `window.__LOCALKIT_SETTINGS__` so the settings store seeds synchronously
/// (no preference flash on cold start).
fn build_settings_init_script(db: &Db) -> String {
    let settings = db.get_all_settings().unwrap_or_default();
    let json = serde_json::to_string(&settings).unwrap_or_else(|_| "{}".into());
    format!("window.__LOCALKIT_SETTINGS__ = Object.freeze({json});")
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let data_dir = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("LocalKit");
    let db = Db::open(&data_dir.join("localkit.db")).expect("failed to open LocalKit database");
    let settings_init_script = build_settings_init_script(&db);

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // Second launch while running (e.g. hidden in tray): just focus.
            tray::show_main_window(app);
        }))
        .plugin(tauri_plugin_opener::init())
        .manage(AppState {
            db: Mutex::new(db),
            data_dir,
            terminals: terminal::PtyManager::new(),
            transfers: Default::default(),
            in_flight: Default::default(),
        })
        .setup(move |app| {
            // Main window is built in code (not tauri.conf.json) so the
            // settings init script can attach before first paint.
            tauri::WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::App("index.html".into()))
                .title("LocalKit")
                .inner_size(1200.0, 800.0)
                .min_inner_size(900.0, 600.0)
                .initialization_script(&settings_init_script)
                .build()?;
            tray::setup(app.handle())?;
            Ok(())
        })
        .on_window_event(|window, event| {
            // Close-to-tray: hide instead of quitting when enabled (default).
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" && tray::run_in_background(&window.state::<AppState>()) {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            check_docker,
            app_info,
            list_sites,
            get_site,
            create_site,
            inspect_docker_project,
            import_docker_project,
            clone_site,
            start_site,
            stop_site,
            delete_site,
            site_logs,
            wp_cli_info,
            login_site,
            site_wp_users,
            list_snapshots,
            create_snapshot,
            restore_snapshot,
            delete_snapshot,
            save_blueprint,
            list_blueprints,
            delete_blueprint,
            create_site_from_blueprint,
            save_serverkit_connection,
            list_serverkit_connections,
            delete_serverkit_connection,
            test_serverkit_connection,
            list_remote_wp_sites,
            create_remote_site,
            push_site_code,
            push_site_db,
            pull_site_db,
            import_remote_site,
            list_sync_history,
            cancel_sync,
            router_status,
            set_domains_enabled,
            set_router_ports,
            trust_router_ca,
            get_app_setting,
            set_app_setting,
            delete_app_setting,
            settings_get_all,
            terminal_open,
            terminal_write,
            terminal_resize,
            terminal_close,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_init_script_publishes_kv() {
        let dir = std::env::temp_dir().join(format!("localkit-test-{}", std::process::id()));
        let db = Db::open(&dir.join("test.db")).unwrap();
        db.set_setting("siteView", "list").unwrap();
        db.set_setting("run_in_background", "true").unwrap();

        let script = build_settings_init_script(&db);
        assert!(script.starts_with("window.__LOCALKIT_SETTINGS__ = Object.freeze("));
        assert!(script.contains("\"siteView\":\"list\""));
        assert!(script.contains("\"run_in_background\":\"true\""));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
