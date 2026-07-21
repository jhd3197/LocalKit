//! End-to-end smoke test for snapshots + restore (plan 17).
//! Runs outside the Tauri runtime (no AppHandle; progress prints to stderr).
//!
//! Usage:
//!   cargo run --example smoke -- create        # once, to have a site
//!   cargo run --example snapshot_smoke         # or `-- run`
//!   cargo run --example snapshot_smoke -- clean
//!
//! Shares the `smoke` example's data dir and site, so it exercises the same
//! WordPress install the lifecycle smoke test builds.

use std::sync::Mutex;

use localkit_lib::{db::Db, docker, site, snapshot, AppState};

const SMOKE_SLUG: &str = "smoke-test";
/// Dropped into wp-content to prove the code archive round-trips, not just the DB.
const CANARY: &str = "localkit-snapshot-canary.txt";

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
        .ok_or_else(|| "smoke site not found — run `cargo run --example smoke -- create` first".into())
}

async fn wp(s: &site::Site, args: &[&str]) -> Result<String, String> {
    let mut full: Vec<&str> = vec!["wp"];
    full.extend_from_slice(args);
    docker::compose_run(&s.dir(), "wpcli", &full).await
}

/// Does post 1 still exist? (`wp post get` fails once it is really gone.)
async fn post_exists(s: &site::Site) -> bool {
    wp(s, &["post", "get", "1", "--field=ID"])
        .await
        .map(|out| out.trim() == "1")
        .unwrap_or(false)
}

async fn run(state: &AppState) -> Result<(), String> {
    let s = find_site(state)?;
    println!("site: {} ({})", s.name, s.slug);

    // The DB import needs the stack up.
    if s.status != "running" {
        println!("starting the site...");
        site::start(state, &s.id).await?;
    }

    // --- arrange: a known post + a known file in wp-content -----------------
    if !post_exists(&s).await {
        wp(&s, &["post", "create", "--post_title=Hello world!", "--post_status=publish"]).await?;
    }
    let canary = s.dir().join("wp-content").join(CANARY);
    std::fs::write(&canary, b"present at snapshot time\n")
        .map_err(|e| format!("failed to write canary: {e}"))?;
    assert!(post_exists(&s).await, "post 1 should exist before the snapshot");

    // --- snapshot -----------------------------------------------------------
    let snap = snapshot::create(
        None,
        state,
        &s.id,
        snapshot::KIND_MANUAL,
        Some("snapshot smoke test".into()),
    )
    .await?;
    println!(
        "SNAPSHOT id={} kind={} db={} B code={} B",
        snap.id, snap.kind, snap.db_bytes, snap.code_bytes
    );
    assert!(snap.db_bytes > 0, "empty database dump");
    assert!(snap.code_bytes > 0, "empty wp-content archive");

    // --- break it -----------------------------------------------------------
    wp(&s, &["post", "delete", "1", "--force"]).await?;
    std::fs::remove_file(&canary).map_err(|e| format!("failed to remove canary: {e}"))?;
    assert!(!post_exists(&s).await, "post 1 should be gone after the delete");
    assert!(!canary.exists(), "canary should be gone after the delete");
    println!("broke the site: post 1 deleted, {CANARY} removed");

    // --- restore ------------------------------------------------------------
    let message = snapshot::restore(None, state, &s.id, &snap.id).await?;
    println!("RESTORE {message}");
    assert!(post_exists(&s).await, "post 1 should be back after the restore");
    assert!(canary.exists(), "{CANARY} should be back after the restore");

    // Restoring is destructive too, so it snapshots first.
    let all = snapshot::list(state, &s.id)?;
    assert!(
        all.iter().any(|x| x.kind == snapshot::KIND_PRE_RESTORE),
        "restore should have taken a pre_restore snapshot"
    );
    // Newest first.
    assert_eq!(all[0].kind, snapshot::KIND_PRE_RESTORE);
    println!(
        "snapshots on disk: {}",
        all.iter()
            .map(|x| format!("{}({})", x.kind, x.id))
            .collect::<Vec<_>>()
            .join(", ")
    );

    // --- delete one ---------------------------------------------------------
    snapshot::delete(state, &s.id, &snap.id)?;
    let after = snapshot::list(state, &s.id)?;
    assert!(
        !after.iter().any(|x| x.id == snap.id),
        "deleted snapshot still listed"
    );
    assert!(
        !snapshot::site_snapshots_dir(&state.data_dir, &s.id)
            .join(&snap.id)
            .exists(),
        "deleted snapshot directory still on disk"
    );

    let _ = std::fs::remove_file(&canary);
    println!("SNAPSHOT SMOKE OK");
    Ok(())
}

/// Drop every snapshot the smoke run left behind.
fn clean(state: &AppState) -> Result<(), String> {
    let root = snapshot::snapshots_root(&state.data_dir);
    if root.exists() {
        std::fs::remove_dir_all(&root).map_err(|e| format!("failed to clean snapshots: {e}"))?;
        println!("cleaned {}", root.display());
    }
    Ok(())
}

#[tokio::main]
async fn main() {
    let cmd = std::env::args().nth(1).unwrap_or_else(|| "run".to_string());
    let status = docker::check().await;
    if !status.available {
        eprintln!("docker unavailable: {:?}", status.error);
        std::process::exit(2);
    }
    let state = make_state();
    let result = match cmd.as_str() {
        "run" => run(&state).await,
        "clean" => clean(&state),
        other => Err(format!("unknown command: {other}")),
    };
    if let Err(e) = result {
        eprintln!("SNAPSHOT SMOKE {cmd} FAILED: {e}");
        std::process::exit(1);
    }
}
