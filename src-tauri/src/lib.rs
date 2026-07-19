pub mod db;
pub mod docker;
pub mod router;
pub mod serverkit;
pub mod site;
pub mod sync;
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
}

#[derive(Serialize)]
struct AppInfo {
    data_dir: String,
    sites_dir: String,
    wp_versions: Vec<String>,
    php_versions: Vec<String>,
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
async fn delete_site(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<(), String> {
    site::delete(&state, &id).await?;
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
    wordpress::info(&s.dir()).await
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

// ---------------------------------------------------------------------------
// Local domains (M6) — shared Caddy router on ports 80/443
// ---------------------------------------------------------------------------

#[tauri::command]
async fn router_status(state: State<'_, AppState>) -> Result<router::RouterStatus, String> {
    router::status(&state).await
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
fn set_app_setting(state: State<AppState>, key: String, value: String) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.set_setting(&key, &value)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let data_dir = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("LocalKit");
    let db = Db::open(&data_dir.join("localkit.db")).expect("failed to open LocalKit database");

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // Second launch while running (e.g. hidden in tray): just focus.
            tray::show_main_window(app);
        }))
        .plugin(tauri_plugin_opener::init())
        .manage(AppState {
            db: Mutex::new(db),
            data_dir,
        })
        .setup(|app| {
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
            start_site,
            stop_site,
            delete_site,
            site_logs,
            wp_cli_info,
            save_serverkit_connection,
            list_serverkit_connections,
            delete_serverkit_connection,
            test_serverkit_connection,
            list_remote_wp_sites,
            create_remote_site,
            push_site_code,
            push_site_db,
            pull_site_db,
            list_sync_history,
            router_status,
            set_domains_enabled,
            trust_router_ca,
            get_app_setting,
            set_app_setting,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
