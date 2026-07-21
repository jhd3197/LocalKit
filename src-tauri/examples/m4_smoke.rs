//! M4 end-to-end smoke: real local Docker site <-> mock serverkit-localkit ext.
//! Prereq: the `smoke` example's site exists (`cargo run --example smoke -- create`).
//! Usage: cargo run --example m4_smoke
//!
//! Covers push code / push DB / pull DB (M4) and, since plan 18, importing a
//! remote site as a brand-new local site — which provisions real containers
//! and tears them down again at the end.

use std::sync::Mutex;

use localkit_lib::{db::Db, docker, serverkit::ServerKitConnection, site, sync, AppState};

const MOCK_URL: &str = "http://127.0.0.1:9872";
const REMOTE_URL: &str = "https://blog.example.com";
/// Canary file the mock extension puts in the wp-content it serves.
const CANARY: &str = "wp-content/themes/remote-theme/style.css";

fn make_state() -> AppState {
    let data_dir = std::env::temp_dir().join("localkit-smoke");
    let db = Db::open(&data_dir.join("localkit.db")).expect("open smoke db");
    AppState {
        db: Mutex::new(db),
        data_dir,
        terminals: localkit_lib::terminal::PtyManager::new(),
        transfers: Default::default(),
        in_flight: Default::default(),
    }
}

#[tokio::main]
async fn main() {
    let state = make_state();
    let site = {
        let db = state.db.lock().unwrap();
        db.list_sites()
            .unwrap()
            .into_iter()
            .find(|s| s.slug == "smoke-test")
            .expect("smoke-test site missing — run `cargo run --example smoke -- create` first")
    };
    println!("local site: {} on port {}", site.slug, site.port);

    // Register a connection pointing at the mock extension.
    let conn = ServerKitConnection {
        id: "mock-conn".to_string(),
        label: "Mock".to_string(),
        url: MOCK_URL.to_string(),
        api_key: "good-key".to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    {
        let db = state.db.lock().unwrap();
        let _ = db.delete_connection("mock-conn");
        db.insert_connection(&conn).unwrap();
    }

    // 1) Push code (real tar.gz of the site's bind-mounted wp-content).
    sync::push_code(None, &state, "mock-conn", &site.id, 1)
        .await
        .expect("push_code failed");
    println!("PUSH CODE OK");

    // 2) Push DB (real `wp db export` of the local site).
    sync::push_db(None, &state, "mock-conn", &site.id, 1)
        .await
        .expect("push_db failed");
    println!("PUSH DB OK");

    // 3) Pull DB (mock returns the same dump with URLs rewritten to remote;
    //    import must bring it back and search-replace to the local URL).
    sync::pull_db(None, &state, "mock-conn", &site.id, 1, Some(REMOTE_URL.to_string()))
        .await
        .expect("pull_db failed");
    println!("PULL DB OK");

    // 4) Verify: siteurl is back to the local URL (search-replace worked).
    let out = docker::compose_run(
        &site.dir(),
        "wpcli",
        &["wp", "option", "get", "siteurl"],
    )
    .await
    .expect("wp option get failed");
    let siteurl = out.trim().to_string();
    let expected = format!("http://localhost:{}", site.port);
    println!("siteurl after pull: {siteurl}");
    assert_eq!(siteurl, expected, "search-replace did not restore local URL");

    // 5) Verify sync history recorded 3 successes.
    let history = sync::history(&state, &site.id).expect("history");
    for h in &history {
        println!("history: {} {} -> {} ({})", h.direction, h.kind, h.status, h.message);
    }
    assert!(history.iter().filter(|h| h.status == "success").count() >= 3);

    // 6) Import remote site #1 as a brand-new local site (plan 18).
    import_smoke(&state).await;

    // 7) Sync v2: chunked upload, interrupted and resumed (plan 19).
    chunked_smoke(&state, &site).await;

    println!("M4 SMOKE OK");
}

// ---------------------------------------------------------------------------
// Plan 19 — chunked transfers
// ---------------------------------------------------------------------------

/// Mock-only control/stats endpoints (see `mock_localkit_ext.cjs`).
async fn control(cfg: serde_json::Value) {
    reqwest::Client::new()
        .post(format!("{MOCK_URL}/api/v1/localkit/__control"))
        .header("X-API-Key", "good-key")
        .json(&cfg)
        .send()
        .await
        .expect("mock __control failed");
}

async fn stats() -> serde_json::Value {
    reqwest::Client::new()
        .get(format!("{MOCK_URL}/api/v1/localkit/__stats"))
        .header("X-API-Key", "good-key")
        .send()
        .await
        .expect("mock __stats failed")
        .json()
        .await
        .expect("mock __stats returned no JSON")
}

fn count(s: &serde_json::Value, key: &str) -> u64 {
    s.get(key).and_then(|v| v.as_u64()).unwrap_or_default()
}

/// A push big enough to need several 8 MiB chunks.
///
/// The smoke site's real `wp-content` is a few hundred KB, which is one chunk —
/// and a one-chunk transfer cannot demonstrate resume. The filler is
/// incompressible (an LCG, not zeroes) so gzip cannot collapse it back into a
/// single chunk and quietly make the test prove nothing.
fn write_filler(site: &site::Site, bytes: usize) -> std::path::PathBuf {
    let path = site.dir().join("wp-content").join("localkit-chunk-filler.bin");
    let mut buf = vec![0u8; bytes];
    let mut x: u32 = 0x1234_5678;
    for b in buf.iter_mut() {
        x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        *b = (x >> 24) as u8;
    }
    std::fs::write(&path, &buf).expect("failed to write the chunk filler");
    path
}

/// Kill the client mid-upload, re-run, and prove only the missing chunks were
/// re-sent — the whole point of plan 19.
async fn chunked_smoke(state: &AppState, site: &site::Site) {
    // Deliberately over the server's 100 MB body limit, so the same fixture
    // proves both halves of the plan: resume re-sends only what was lost, and
    // a payload v1 physically cannot deliver goes up fine in 8 MiB chunks.
    let filler = write_filler(site, 110 * 1024 * 1024);
    let result = run_chunked_assertions(state, site, &filler).await;
    let _ = std::fs::remove_file(&filler);
    result.expect("chunked sync verification failed");
    println!("CHUNKED SYNC OK");
}

async fn run_chunked_assertions(
    state: &AppState,
    site: &site::Site,
    filler: &std::path::Path,
) -> Result<(), String> {
    const STOP_AFTER: u64 = 2;

    // --- an interrupted upload -------------------------------------------
    control(serde_json::json!({
        "resetStats": true, "forgetTransfers": true, "failChunksAfter": STOP_AFTER
    }))
    .await;

    let err = sync::push_code(None, state, "mock-conn", &site.id, 1)
        .await
        .expect_err("the injected chunk failure did not fail the push");
    println!("push interrupted as designed: {err}");

    let s = stats().await;
    let landed = count(&s, "chunkPuts");
    if landed != STOP_AFTER {
        return Err(format!("expected {STOP_AFTER} chunks before the failure, got {landed}"));
    }
    if count(&s, "finishes") != 0 {
        return Err("an interrupted upload reached finish — nothing should have been applied".into());
    }
    println!("INTERRUPT LEFT {landed} CHUNKS ON THE SERVER OK");

    // --- the retry resumes -------------------------------------------------
    control(serde_json::json!({ "resetStats": true, "failChunksAfter": null })).await;
    sync::push_code(None, state, "mock-conn", &site.id, 1)
        .await
        .map_err(|e| format!("the resumed push failed: {e}"))?;

    let s = stats().await;
    let resent = count(&s, "chunkPuts");
    let total = count(&s, "lastTotalChunks");
    if count(&s, "resumedInits") != 1 {
        return Err("the server did not recognise the retry as a resume".into());
    }
    if total < 3 {
        return Err(format!(
            "the payload was only {total} chunk(s) — too small to prove anything about resume"
        ));
    }
    // The assertion the plan asks for: only what was lost went back up.
    if resent != total - STOP_AFTER {
        return Err(format!(
            "resume re-sent {resent} chunks; expected {} of {total} (the {STOP_AFTER} already confirmed should have been skipped)",
            total - STOP_AFTER
        ));
    }
    // A successful `finish` IS the whole-file hash check: the server refuses
    // to process a payload whose assembled sha256 does not match `init`.
    if count(&s, "finishes") != 1 {
        return Err("the resumed upload never completed a verified finish".into());
    }
    println!("RESUME RE-SENT ONLY {resent} OF {total} CHUNKS, HASH VERIFIED OK");

    // The archive that just went up is bigger than the server would accept in
    // one request — which is the whole point of the plan. Prove the wall is
    // real by making the same client talk v1 to the same payload.
    let sent_bytes = count(&s, "chunkBytes") + STOP_AFTER * 8 * 1024 * 1024;
    if sent_bytes <= 100 * 1024 * 1024 {
        return Err(format!(
            "the fixture is only {sent_bytes} bytes — too small to prove the 100 MB limit is gone"
        ));
    }
    control(serde_json::json!({ "resetStats": true, "forgetTransfers": true, "syncV2": false })).await;
    let err = sync::push_code(None, state, "mock-conn", &site.id, 1)
        .await
        .expect_err("v1 accepted a payload over the server's 100 MB limit");
    if !err.contains("too large") {
        return Err(format!("expected a size refusal from v1, got: {err}"));
    }
    println!("SAME PAYLOAD OVER V1: REFUSED ({err}) — LIMIT LIFTED BY V2 OK");

    // --- one client, both servers -----------------------------------------
    // With sync-v2 withdrawn from /pair, a payload that *does* fit must still
    // go up the v1 monolithic path rather than failing.
    std::fs::remove_file(filler).map_err(|e| format!("failed to remove the filler: {e}"))?;
    control(serde_json::json!({ "resetStats": true, "forgetTransfers": true, "syncV2": false })).await;
    sync::push_code(None, state, "mock-conn", &site.id, 1)
        .await
        .map_err(|e| format!("the v1 fallback push failed: {e}"))?;

    let s = stats().await;
    if count(&s, "v1Pushes") != 1 || count(&s, "chunkPuts") != 0 {
        return Err(format!(
            "expected exactly one v1 multipart push and no chunks, got v1={} chunks={}",
            count(&s, "v1Pushes"),
            count(&s, "chunkPuts")
        ));
    }
    println!("V1 FALLBACK ON AN OLD SERVER OK");

    control(serde_json::json!({ "syncV2": true, "resetStats": true })).await;
    Ok(())
}

/// Plan 18: clone remote site #1 down as a new local site, assert the remote
/// wp-content and database actually landed, then delete it again.
///
/// The imported site is always removed at the end (including on assertion
/// failure paths that run after creation) so repeat runs start clean — a
/// leftover would collide on the slug and mask a real regression.
async fn import_smoke(state: &AppState) {
    // A stale import from a previous run would trip the "already imported"
    // guard, so clear it first.
    let stale: Vec<String> = {
        let db = state.db.lock().unwrap();
        db.sites_from_remote("mock-conn", 1)
            .unwrap()
            .into_iter()
            .map(|s| s.id)
            .collect()
    };
    for id in stale {
        println!("removing stale imported site {id}");
        site::delete(None, state, &id, true).await.expect("cleanup stale import");
    }

    // Multisite must be refused *before* anything is provisioned.
    let before = { state.db.lock().unwrap().list_sites().unwrap().len() };
    let err = sync::import_site(None, state, "mock-conn", 3, None)
        .await
        .expect_err("importing a multisite must fail");
    assert!(err.contains("multisite"), "unexpected error: {err}");
    let after = { state.db.lock().unwrap().list_sites().unwrap().len() };
    assert_eq!(before, after, "a refused import left a site row behind");
    println!("IMPORT REFUSES MULTISITE OK");

    let imported = sync::import_site(None, state, "mock-conn", 1, Some("Imported Blog".into()))
        .await
        .expect("import_site failed");
    println!("imported: {} on port {}", imported.slug, imported.port);

    let result = verify_import(state, &imported).await;

    // Always tear the imported site down, then report.
    site::delete(None, state, &imported.id, true)
        .await
        .expect("failed to delete the imported site");
    println!("imported site deleted");
    result.expect("import verification failed");
    println!("IMPORT OK");
}

async fn verify_import(state: &AppState, imported: &site::Site) -> Result<(), String> {
    // Origin recorded, so a future pull knows which remote to default to.
    if imported.connection_id.as_deref() != Some("mock-conn") || imported.remote_site_id != Some(1) {
        return Err(format!(
            "origin not recorded: {:?} / {:?}",
            imported.connection_id, imported.remote_site_id
        ));
    }

    // The remote wp-content actually landed on disk.
    let canary = imported.dir().join(CANARY);
    let body = std::fs::read_to_string(&canary)
        .map_err(|e| format!("remote wp-content missing at {}: {e}", canary.display()))?;
    if !body.contains("pulled from the remote site") {
        return Err(format!("canary file has unexpected content: {body}"));
    }
    println!("remote wp-content extracted OK");

    // The one-click login plugin survived the archive landing on top of it.
    if !imported.dir().join("wp-content/mu-plugins/localkit-login.php").exists() {
        return Err("the login MU plugin did not survive the import".into());
    }

    // The imported database is live and rewritten to the local URL.
    let siteurl = docker::compose_run(&imported.dir(), "wpcli", &["wp", "option", "get", "siteurl"])
        .await
        .map_err(|e| format!("wp option get siteurl failed: {e}"))?;
    let siteurl = siteurl.trim();
    let expected = format!("http://localhost:{}", imported.port);
    if siteurl != expected {
        return Err(format!("siteurl is {siteurl}, expected {expected}"));
    }
    println!("imported siteurl: {siteurl}");

    // The import is recorded in the new site's sync history.
    let history = sync::history(state, &imported.id)?;
    if !history.iter().any(|h| h.kind == "import" && h.status == "success") {
        return Err("no successful import row in sync history".into());
    }
    Ok(())
}
