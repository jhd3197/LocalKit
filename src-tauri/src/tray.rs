//! M8 — system tray: close-to-tray + a tray menu with quick site actions.
//!
//! The tray keeps LocalKit alive after the window is closed so Docker sites
//! keep running. Menu: Show · per-site Open/Start/Stop · Quit. The menu and
//! tooltip are rebuilt (`refresh`) after every lifecycle change; statuses come
//! from the DB (updated by site::start/stop), never from a live docker ps.

use tauri::menu::{Menu, MenuBuilder, MenuItemBuilder, SubmenuBuilder};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};
use tauri_plugin_opener::OpenerExt;

use crate::{router, site, AppState};

/// app_settings key: close-to-tray ("true"/"false", default on).
pub const KEY_RUN_IN_BACKGROUND: &str = "run_in_background";

const TRAY_ID: &str = "main";
const ID_SHOW: &str = "show";
const ID_QUIT: &str = "quit";
const ID_OPEN: &str = "open:";
const ID_START: &str = "start:";
const ID_STOP: &str = "stop:";

/// Read the close-to-tray preference. Defaults to on when unset/unreadable.
pub fn run_in_background(state: &AppState) -> bool {
    let Ok(db) = state.db.lock() else {
        return true;
    };
    db.get_setting(KEY_RUN_IN_BACKGROUND)
        .ok()
        .flatten()
        .as_deref()
        != Some("false")
}

/// Build the tray icon + menu. Called once from `.setup()`.
pub fn setup(app: &AppHandle) -> tauri::Result<()> {
    let menu = build_menu(app)?;
    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .menu(&menu)
        .tooltip(&tooltip(app))
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| on_menu_event(app, event.id().as_ref()))
        .on_tray_icon_event(|tray, event| {
            // Left-click restores the window (Windows/Linux convention).
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        });
    if let Some(icon) = tray_icon() {
        builder = builder.icon(icon);
    }
    builder.build(app)?;
    Ok(())
}

/// Rebuild the menu + tooltip. Cheap; call after any site lifecycle change.
pub fn refresh(app: &AppHandle) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    if let Ok(menu) = build_menu(app) {
        let _ = tray.set_menu(Some(menu));
    }
    let _ = tray.set_tooltip(Some(tooltip(app)));
}

/// Show + focus the main window (tray Show item, left-click, 2nd instance).
pub fn show_main_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

fn tray_icon() -> Option<tauri::image::Image<'static>> {
    tauri::image::Image::from_bytes(include_bytes!("../icons/icon.ico")).ok()
}

fn tooltip(app: &AppHandle) -> String {
    let running = sites(app).iter().filter(|s| s.status == "running").count();
    let noun = if running == 1 { "site" } else { "sites" };
    format!("LocalKit — {running} {noun} running")
}

fn sites(app: &AppHandle) -> Vec<site::Site> {
    let state = app.state::<AppState>();
    let Ok(db) = state.db.lock() else {
        return Vec::new();
    };
    db.list_sites().unwrap_or_default()
}

/// URL a site's "Open in browser" should hit: `<slug>.test` when local
/// domains are on, otherwise the always-working `localhost:<port>`.
fn site_url(state: &AppState, site: &site::Site) -> String {
    let (domains_on, trusted) = router::enabled_and_trusted(state);
    if domains_on {
        router::site_url(&site.slug, trusted)
    } else {
        format!("http://localhost:{}", site.port)
    }
}

fn build_menu(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let show = MenuItemBuilder::with_id(ID_SHOW, "Show LocalKit").build(app)?;
    let quit = MenuItemBuilder::with_id(ID_QUIT, "Quit LocalKit").build(app)?;

    let sites = sites(app);
    let sites_menu = if sites.is_empty() {
        let empty = MenuItemBuilder::new("No sites yet").enabled(false).build(app)?;
        SubmenuBuilder::with_id(app, "sites", "Sites")
            .item(&empty)
            .build()?
    } else {
        let state = app.state::<AppState>();
        let mut submenu = SubmenuBuilder::with_id(app, "sites", "Sites");
        for site in &sites {
            let running = site.status == "running";
            let dot = if running { "●" } else { "○" };
            let open = MenuItemBuilder::with_id(
                format!("{ID_OPEN}{}", site.id),
                format!("Open {} in browser", site_url(&state, site)),
            )
            .enabled(running)
            .build(app)?;
            let toggle = if running {
                MenuItemBuilder::with_id(format!("{ID_STOP}{}", site.id), "Stop").build(app)?
            } else {
                MenuItemBuilder::with_id(format!("{ID_START}{}", site.id), "Start").build(app)?
            };
            let site_menu = SubmenuBuilder::with_id(app, format!("site:{}", site.id), format!("{dot} {}", site.slug))
                .item(&open)
                .item(&toggle)
                .build()?;
            submenu = submenu.item(&site_menu);
        }
        submenu.build()?
    };

    MenuBuilder::new(app)
        .item(&show)
        .separator()
        .item(&sites_menu)
        .separator()
        .item(&quit)
        .build()
}

fn on_menu_event(app: &AppHandle, id: &str) {
    match id {
        ID_SHOW => show_main_window(app),
        // Real quit. Containers are intentionally left running (see plan 8).
        ID_QUIT => app.exit(0),
        _ if id.starts_with(ID_OPEN) => {
            let site_id = &id[ID_OPEN.len()..];
            let state = app.state::<AppState>();
            let url = sites(app)
                .into_iter()
                .find(|s| s.id == site_id)
                .map(|s| site_url(&state, &s));
            if let Some(url) = url {
                let _ = app.opener().open_url(&url, None::<&str>);
            }
        }
        _ if id.starts_with(ID_START) => {
            spawn_lifecycle(app, id[ID_START.len()..].to_string(), true);
        }
        _ if id.starts_with(ID_STOP) => {
            spawn_lifecycle(app, id[ID_STOP.len()..].to_string(), false);
        }
        _ => {}
    }
}

fn spawn_lifecycle(app: &AppHandle, site_id: String, start: bool) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        {
            let state = app.state::<AppState>();
            let result = if start {
                site::start(&state, &site_id).await
            } else {
                site::stop(&state, &site_id).await
            };
            if let Err(e) = result {
                eprintln!("[tray] site {} failed: {e}", if start { "start" } else { "stop" });
            }
        }
        refresh(&app);
    });
}
