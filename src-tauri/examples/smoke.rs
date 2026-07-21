//! End-to-end smoke test driver for the real LocalKit site lifecycle.
//! Runs outside the Tauri runtime (no AppHandle; events are skipped).
//!
//! Usage: cargo run --example smoke -- <create|verify|info|stop|start|reconcile|clone|blueprint|delete|cleanup>
//!
//! Uses a fixed smoke data dir + site name so subcommands can run as separate
//! invocations (each one reconstructs the same AppState).

use std::path::Path;
use std::sync::Mutex;

use localkit_lib::{blueprint, db::Db, docker, reconcile, site, snapshot, wordpress, AppState};

const SMOKE_NAME: &str = "Smoke Test";
const SMOKE_SLUG: &str = "smoke-test";
/// Plan 20 clone verification: a throwaway copy of the smoke site.
const CLONE_NAME: &str = "Smoke Clone";
const CLONE_SLUG: &str = "smoke-clone";
/// Plan 20 blueprint verification: a template + a site stamped from it.
const BP_NAME: &str = "Smoke Blueprint";
const BP_FROM_NAME: &str = "Smoke From BP";
const BP_FROM_SLUG: &str = "smoke-from-bp";

fn make_state() -> AppState {
    let data_dir = std::env::temp_dir().join("localkit-smoke");
    std::fs::create_dir_all(&data_dir).expect("create smoke data dir");
    let db = Db::open(&data_dir.join("localkit.db")).expect("open smoke db");
    AppState {
        db: Mutex::new(db),
        data_dir,
        terminals: localkit_lib::terminal::PtyManager::new(),
        transfers: Default::default(),
        in_flight: Default::default(),
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

/// Backdate a site's status write via a second connection to the smoke DB, so
/// the reconciler's 60 s grace window does not shield an "external stop". This
/// is the one thing the public `Db::set_status` (which always stamps `now`)
/// deliberately won't do — hence the raw UPDATE, kept here in the dev tool.
fn force_status(state: &AppState, id: &str, status: &str, ts: &str) -> Result<(), String> {
    let conn = rusqlite::Connection::open(state.data_dir.join("localkit.db"))
        .map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE sites SET status = ?1, status_updated_at = ?2 WHERE id = ?3",
        rusqlite::params![status, ts, id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn db_status(state: &AppState, id: &str) -> Result<String, String> {
    Ok(state.db.lock().map_err(|e| e.to_string())?.get_site(id)?.status)
}

/// Stop a site's containers *without* removing them (`docker compose stop`),
/// simulating an external `docker stop` — LocalKit's own stop uses `down`.
fn compose_stop(dir: &Path) -> Result<(), String> {
    let out = std::process::Command::new("docker")
        .args(["compose", "stop"])
        .current_dir(dir)
        .output()
        .map_err(|e| format!("docker compose stop failed to run: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).to_string())
    }
}

/// Reconciler verification (plan 23) against real Docker drift: stop the
/// site's containers behind LocalKit's back and confirm the reconciler settles
/// running→stopped, then bring them back and confirm it settles stopped→
/// running. The DB is manipulated directly to create the drift a crash / an
/// external `docker stop` would leave.
async fn reconcile_smoke(state: &AppState) -> Result<(), String> {
    let s = find_site(state)?;
    // Start from a known-up state.
    docker::compose_up(&s.dir()).await?;
    site::start(state, &s.id).await?;

    // --- External stop: containers down, DB still says running (backdated past
    //     the grace window so the reconciler is allowed to downgrade). ---
    println!("stopping containers externally (docker compose stop)...");
    compose_stop(&s.dir())?;
    force_status(state, &s.id, "running", "2000-01-01T00:00:00+00:00")?;
    let events = reconcile::reconcile_once(state).await;
    println!("after external stop -> {} settle(s): {events:?}", events.len());
    assert_eq!(db_status(state, &s.id)?, "stopped", "external stop must settle to stopped");
    assert!(
        events.iter().any(|e| e.to == "stopped" && e.reason == "external stop"),
        "expected an external-stop settle event"
    );

    // --- External start: containers up, DB still says stopped. ---
    println!("starting containers externally (docker compose up -d)...");
    docker::compose_up(&s.dir()).await?;
    // Give the container a moment to report `running` to `docker ps`.
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    force_status(state, &s.id, "stopped", "2000-01-01T00:00:00+00:00")?;
    let events = reconcile::reconcile_once(state).await;
    println!("after external start -> {} settle(s): {events:?}", events.len());
    assert_eq!(db_status(state, &s.id)?, "running", "external start must settle to running");

    // --- Forward-only: a fresh command write must NOT be clobbered by a stale
    //     reconcile observation. Stop the containers but keep a *now* running
    //     write; the reconciler must leave it alone (grace window). ---
    compose_stop(&s.dir())?;
    state.db.lock().map_err(|e| e.to_string())?.set_status(&s.id, "running")?;
    let events = reconcile::reconcile_once(state).await;
    assert_eq!(db_status(state, &s.id)?, "running", "a fresh running write must survive the grace window");
    assert!(events.is_empty(), "grace window should suppress the downgrade");
    println!("forward-only grace window held: fresh running write survived");

    // Leave the smoke site genuinely running for the next subcommand.
    site::start(state, &s.id).await?;
    println!("RECONCILE OK");
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

/// Blueprint verification (plan 20): save the smoke site as a blueprint, assert
/// its artifacts landed and the transient snapshot was pruned, then stamp a new
/// site out of it and assert the source's content rode along.
async fn blueprint_smoke(state: &AppState) -> Result<(), String> {
    let source = find_site(state)?;
    // Idempotent: drop leftovers from a previous run.
    remove_from_bp(state).await;
    for bp in blueprint::list(state)?.iter().filter(|b| b.manifest.name == BP_NAME) {
        let _ = blueprint::delete(state, &bp.id);
    }

    if source.status != "running" {
        site::start(state, &source.id).await?;
    }

    // Arrange: a uniquely-titled published post on the source.
    const MARKER: &str = "LocalKit blueprint smoke marker";
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

    // Save.
    let bp = blueprint::save(
        None,
        state,
        &source.id,
        BP_NAME.to_string(),
        Some("smoke blueprint".into()),
    )
    .await?;
    println!(
        "BLUEPRINT id={} plugins={} theme={} db={} B code={} B",
        bp.id,
        bp.manifest.plugins.len(),
        bp.manifest.theme,
        bp.db_bytes,
        bp.code_bytes
    );

    // Assert: artifacts landed.
    let dir = blueprint::blueprints_root(&state.data_dir).join(&bp.id);
    for f in ["blueprint.json", "db.sql.gz", "wp-content.tar.gz"] {
        assert!(dir.join(f).exists(), "blueprint missing {f}");
    }
    assert!(bp.db_bytes > 0, "empty blueprint database dump");
    assert!(bp.code_bytes > 0, "empty blueprint wp-content archive");

    // Assert: the transient blueprint_source snapshot was pruned.
    let snaps = snapshot::list(state, &source.id)?;
    assert!(
        snaps.iter().all(|s| s.kind != snapshot::KIND_BLUEPRINT_SOURCE),
        "a blueprint_source snapshot was left behind"
    );

    // Act: stamp a new site out of the blueprint.
    let created = blueprint::create_site(None, state, &bp.id, Some(BP_FROM_NAME.to_string())).await?;
    println!(
        "CREATED FROM BLUEPRINT id={} slug={} port={} admin={}",
        created.id, created.slug, created.port, created.admin_user
    );

    // Assert: it serves HTTP and carries the source's content.
    let url = format!("http://localhost:{}", created.port);
    let home = http_code(&format!("{url}/"));
    assert!(
        ["200", "301", "302"].contains(&home.as_str()),
        "blueprint site home returned unexpected status: {home}"
    );
    let created_titles = wp(
        &created,
        &["post", "list", "--post_status=publish", "--field=post_title", "--format=csv"],
    )
    .await?;
    assert!(
        created_titles.contains(MARKER),
        "marker post missing from the blueprint site: {created_titles:?}"
    );
    println!("BLUEPRINT SMOKE OK on {url}");

    // Tidy up.
    remove_from_bp(state).await;
    let _ = blueprint::delete(state, &bp.id);
    Ok(())
}

async fn remove_from_bp(state: &AppState) {
    let sites = {
        let db = state.db.lock().expect("lock db");
        db.list_sites().unwrap_or_default()
    };
    for s in sites {
        if s.slug == BP_FROM_SLUG || s.slug.starts_with(&format!("{BP_FROM_SLUG}-")) {
            let _ = site::delete(None, state, &s.id, true).await;
            println!("cleaned blueprint site {}", s.slug);
        }
    }
    let orphan = state.data_dir.join("sites").join(BP_FROM_SLUG);
    if orphan.exists() {
        let _ = docker::compose_down(&orphan, true).await;
        let _ = std::fs::remove_dir_all(&orphan);
    }
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
    // Sites the `clone` / `blueprint` subcommands leave behind are leftovers too.
    remove_clone(state).await;
    remove_from_bp(state).await;
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
        "reconcile" => reconcile_smoke(&state).await,
        "clone" => clone(&state).await,
        "blueprint" => blueprint_smoke(&state).await,
        "delete" => delete(&state).await,
        "cleanup" => cleanup(&state).await,
        other => Err(format!("unknown command: {other}")),
    };
    if let Err(e) = result {
        eprintln!("SMOKE {cmd} FAILED: {e}");
        std::process::exit(1);
    }
}
