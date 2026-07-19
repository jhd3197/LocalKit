//! End-to-end smoke test driver for the real LocalKit site lifecycle.
//! Runs outside the Tauri runtime (no AppHandle; events are skipped).
//!
//! Usage: cargo run --example smoke -- <create|verify|info|stop|start|delete|cleanup>
//!
//! Uses a fixed smoke data dir + site name so subcommands can run as separate
//! invocations (each one reconstructs the same AppState).

use std::sync::Mutex;

use localkit_lib::{db::Db, docker, site, wordpress, AppState};

const SMOKE_NAME: &str = "Smoke Test";
const SMOKE_SLUG: &str = "smoke-test";

fn make_state() -> AppState {
    let data_dir = std::env::temp_dir().join("localkit-smoke");
    std::fs::create_dir_all(&data_dir).expect("create smoke data dir");
    let db = Db::open(&data_dir.join("localkit.db")).expect("open smoke db");
    AppState {
        db: Mutex::new(db),
        data_dir,
    }
}

fn find_site(state: &AppState) -> Result<site::Site, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.list_sites()?
        .into_iter()
        .find(|s| s.slug == SMOKE_SLUG || s.slug.starts_with(&format!("{SMOKE_SLUG}-")))
        .ok_or_else(|| "smoke site not found in db".to_string())
}

fn http_code(url: &str) -> String {
    std::process::Command::new("curl")
        .args(["-s", "-o", "NUL", "-w", "%{http_code}", "--max-time", "20", url])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|e| format!("curl failed: {e}"))
}

async fn create(state: &AppState) -> Result<(), String> {
    // Idempotent: remove any stale smoke site from a previous (killed) run.
    let _ = cleanup(state).await;
    let s = site::create(
        None,
        state,
        SMOKE_NAME.to_string(),
        "6.7".to_string(),
        "8.3".to_string(),
    )
    .await?;
    println!(
        "CREATED id={} slug={} port={} db_port={} admin={} pass={}",
        s.id,
        s.slug,
        s.port,
        s.db_port(),
        s.admin_user,
        s.admin_pass
    );
    Ok(())
}

async fn verify(state: &AppState) -> Result<(), String> {
    let s = find_site(state)?;
    let url = format!("http://localhost:{}", s.port);

    let home = http_code(&format!("{url}/"));
    let admin = http_code(&format!("{url}/wp-admin/"));
    println!("HTTP / -> {home}   /wp-admin/ -> {admin}");
    assert!(
        ["200", "301", "302"].contains(&home.as_str()),
        "unexpected HTTP status for /: {home}"
    );
    assert!(
        ["200", "301", "302"].contains(&admin.as_str()),
        "unexpected HTTP status for /wp-admin/: {admin}"
    );

    // Credentials stored?
    assert!(!s.admin_user.is_empty(), "admin_user empty");
    assert!(!s.admin_pass.is_empty(), "admin_pass empty");
    println!("creds stored: user={} pass=<{} chars>", s.admin_user, s.admin_pass.len());

    // Bind-mounted wp-content populated with real WP files?
    let wpc = s.dir().join("wp-content");
    for rel in ["index.php", "plugins", "themes"] {
        assert!(wpc.join(rel).exists(), "missing wp-content/{rel}");
    }
    println!("wp-content bind mount populated at {}", wpc.display());

    // DB row status
    assert_eq!(s.status, "running", "db status should be running");
    println!("VERIFY OK on {url}");
    Ok(())
}

async fn info(state: &AppState) -> Result<(), String> {
    let s = find_site(state)?;
    let info = wordpress::info(&s.dir()).await?;
    println!("core_version={}", info.core_version);
    for p in &info.plugins {
        println!("plugin: {} {} ({})", p.name, p.version, p.status);
    }
    assert!(!info.core_version.is_empty(), "empty core version");
    assert!(!info.plugins.is_empty(), "empty plugin list");
    println!("INFO OK");
    Ok(())
}

async fn stop(state: &AppState) -> Result<(), String> {
    let s = find_site(state)?;
    let s = site::stop(state, &s.id).await?;
    println!("STOPPED status={}", s.status);
    assert_eq!(s.status, "stopped");
    Ok(())
}

async fn start(state: &AppState) -> Result<(), String> {
    let s = find_site(state)?;
    let s = site::start(state, &s.id).await?;
    println!("STARTED status={}", s.status);
    assert_eq!(s.status, "running");
    Ok(())
}

async fn delete(state: &AppState) -> Result<(), String> {
    let s = find_site(state)?;
    let dir = s.dir();
    site::delete(state, &s.id).await?;
    assert!(!dir.exists(), "site dir still exists after delete");
    let db = state.db.lock().map_err(|e| e.to_string())?;
    assert!(db.list_sites()?.is_empty(), "db rows left after delete");
    println!("DELETE OK (dir removed, db row removed)");
    Ok(())
}

/// Force-remove any smoke-test leftovers (compose project + dir + db rows).
async fn cleanup(state: &AppState) -> Result<(), String> {
    let sites = {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        db.list_sites()?
    };
    for s in sites {
        if s.slug == SMOKE_SLUG || s.slug.starts_with(&format!("{SMOKE_SLUG}-")) {
            let dir = s.dir();
            if dir.exists() {
                let _ = docker::compose_down(&dir, true).await;
                let _ = std::fs::remove_dir_all(&dir);
            }
            let db = state.db.lock().map_err(|e| e.to_string())?;
            let _ = db.delete_site(&s.id);
            println!("cleaned stale site {}", s.slug);
        }
    }
    // Also handle an orphaned project dir with no db row.
    let orphan = state.data_dir.join("sites").join(SMOKE_SLUG);
    if orphan.exists() {
        let _ = docker::compose_down(&orphan, true).await;
        let _ = std::fs::remove_dir_all(&orphan);
        println!("cleaned orphan dir {}", orphan.display());
    }
    Ok(())
}

#[tokio::main]
async fn main() {
    let cmd = std::env::args().nth(1).unwrap_or_else(|| "verify".to_string());
    let status = docker::check().await;
    if !status.available {
        eprintln!("docker unavailable: {:?}", status.error);
        std::process::exit(2);
    }
    let state = make_state();
    let result = match cmd.as_str() {
        "create" => create(&state).await,
        "verify" => verify(&state).await,
        "info" => info(&state).await,
        "stop" => stop(&state).await,
        "start" => start(&state).await,
        "delete" => delete(&state).await,
        "cleanup" => cleanup(&state).await,
        other => Err(format!("unknown command: {other}")),
    };
    if let Err(e) = result {
        eprintln!("SMOKE {cmd} FAILED: {e}");
        std::process::exit(1);
    }
}
