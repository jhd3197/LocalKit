//! End-to-end smoke test driver for the real LocalKit site lifecycle.
//! Runs outside the Tauri runtime (no AppHandle; events are skipped).
//!
//! Usage: cargo run --example smoke -- <create|verify|info|stop|start|clone|delete|cleanup>
//!
//! Uses a fixed smoke data dir + site name so subcommands can run as separate
//! invocations (each one reconstructs the same AppState).

use std::path::Path;
use std::sync::Mutex;

use localkit_lib::{db::Db, docker, site, snapshot, wordpress, AppState};

const SMOKE_NAME: &str = "Smoke Test";
const SMOKE_SLUG: &str = "smoke-test";
/// Plan 20 clone verification: a throwaway copy of the smoke site.
const CLONE_NAME: &str = "Smoke Clone";
const CLONE_SLUG: &str = "smoke-clone";

fn make_state() -> AppState {
    let data_dir = std::env::temp_dir().join("localkit-smoke");
    std::fs::create_dir_all(&data_dir).expect("create smoke data dir");
    let db = Db::open(&data_dir.join("localkit.db")).expect("open smoke db");
    AppState {
        db: Mutex::new(db),
        data_dir,
        terminals: localkit_lib::terminal::PtyManager::new(),
        transfers: Default::default(),
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

async fn wp(s: &site::Site, args: &[&str]) -> Result<String, String> {
    let mut full: Vec<&str> = vec!["wp"];
    full.extend_from_slice(args);
    docker::compose_run(&s.dir(), "wpcli", &full).await
}

fn read_db_password(dir: &Path) -> Option<String> {
    let content = std::fs::read_to_string(dir.join(".env")).ok()?;
    for line in content.lines() {
        if let Some((k, v)) = line.split_once('=') {
            if k.trim() == "DB_PASSWORD" {
                return Some(v.trim().to_string());
            }
        }
    }
    None
}

/// Clone verification (plan 20): create a marker post on the source, clone it,
/// and assert the post rode along, the clone answers HTTP, its DB password and
/// port are fresh, its admin login carried over, and the transient
/// `clone_source` snapshot was pruned.
async fn clone(state: &AppState) -> Result<(), String> {
    let source = find_site(state)?;
    // Idempotent: drop a clone left by a previous (killed) run.
    remove_clone(state).await;

    if source.status != "running" {
        site::start(state, &source.id).await?;
    }

    // Arrange: a uniquely-titled published post on the source.
    const MARKER: &str = "LocalKit clone smoke marker";
    let titles = wp(
        &source,
        &["post", "list", "--post_status=publish", "--field=post_title", "--format=csv"],
    )
    .await
    .unwrap_or_default();
    if !titles.contains(MARKER) {
        wp(
            &source,
            &["post", "create", &format!("--post_title={MARKER}"), "--post_status=publish"],
        )
        .await?;
    }

    // Act.
    let clone = site::clone_site(None, state, &source.id, CLONE_NAME.to_string()).await?;
    println!(
        "CLONED id={} slug={} port={} admin={}",
        clone.id, clone.slug, clone.port, clone.admin_user
    );

    // Assert: the clone serves HTTP.
    let url = format!("http://localhost:{}", clone.port);
    let home = http_code(&format!("{url}/"));
    assert!(
        ["200", "301", "302"].contains(&home.as_str()),
        "clone home returned unexpected status: {home}"
    );

    // Assert: the marker post rode along in the copied database.
    let clone_titles = wp(
        &clone,
        &["post", "list", "--post_status=publish", "--field=post_title", "--format=csv"],
    )
    .await?;
    assert!(
        clone_titles.contains(MARKER),
        "marker post missing from the clone: {clone_titles:?}"
    );
    println!("marker post present in the clone");

    // Assert: secrets are fresh (never copied), port is distinct.
    let src_pw = read_db_password(&source.dir()).ok_or("source .env missing DB_PASSWORD")?;
    let clone_pw = read_db_password(&clone.dir()).ok_or("clone .env missing DB_PASSWORD")?;
    assert_ne!(src_pw, clone_pw, "clone reused the source's DB password");
    assert_ne!(source.port, clone.port, "clone reused the source's port");
    println!("fresh DB password + distinct port confirmed");

    // Assert: the admin login carries over (the copied DB holds it).
    assert_eq!(clone.admin_user, source.admin_user, "admin user should carry over");
    assert_eq!(clone.admin_pass, source.admin_pass, "admin password should carry over");

    // Assert: the transient clone_source snapshot was pruned from the source.
    let snaps = snapshot::list(state, &source.id)?;
    assert!(
        snaps.iter().all(|s| s.kind != snapshot::KIND_CLONE_SOURCE),
        "a clone_source snapshot was left behind on the source"
    );
    println!("CLONE OK on {url}");

    // Tidy up so re-runs stay idempotent.
    remove_clone(state).await;
    Ok(())
}

/// Force-remove any clone leftovers (compose project + dir + db rows + snapshots).
async fn remove_clone(state: &AppState) {
    let sites = {
        let db = state.db.lock().expect("lock db");
        db.list_sites().unwrap_or_default()
    };
    for s in sites {
        if s.slug == CLONE_SLUG || s.slug.starts_with(&format!("{CLONE_SLUG}-")) {
            let _ = site::delete(None, state, &s.id, true).await;
            println!("cleaned clone {}", s.slug);
        }
    }
    let orphan = state.data_dir.join("sites").join(CLONE_SLUG);
    if orphan.exists() {
        let _ = docker::compose_down(&orphan, true).await;
        let _ = std::fs::remove_dir_all(&orphan);
    }
}

async fn delete(state: &AppState) -> Result<(), String> {
    let s = find_site(state)?;
    let dir = s.dir();
    // Keep the snapshots so `snapshot_smoke` can assert they survive the site.
    site::delete(None, state, &s.id, false).await?;
    assert!(!dir.exists(), "site dir still exists after delete");
    let db = state.db.lock().map_err(|e| e.to_string())?;
    assert!(db.list_sites()?.is_empty(), "db rows left after delete");
    println!("DELETE OK (dir removed, db row removed)");
    Ok(())
}

/// Force-remove any smoke-test leftovers (compose project + dir + db rows).
async fn cleanup(state: &AppState) -> Result<(), String> {
    // A clone from the `clone` subcommand is a smoke-test leftover too.
    remove_clone(state).await;
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
        "clone" => clone(&state).await,
        "delete" => delete(&state).await,
        "cleanup" => cleanup(&state).await,
        other => Err(format!("unknown command: {other}")),
    };
    if let Err(e) = result {
        eprintln!("SMOKE {cmd} FAILED: {e}");
        std::process::exit(1);
    }
}
