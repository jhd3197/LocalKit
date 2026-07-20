//! M4 end-to-end smoke: real local Docker site <-> mock serverkit-localkit ext.
//! Prereq: the `smoke` example's site exists (`cargo run --example smoke -- create`).
//! Usage: cargo run --example m4_smoke

use std::sync::Mutex;

use localkit_lib::{db::Db, docker, serverkit::ServerKitConnection, sync, AppState};

const MOCK_URL: &str = "http://127.0.0.1:9872";
const REMOTE_URL: &str = "https://blog.example.com";

fn make_state() -> AppState {
    let data_dir = std::env::temp_dir().join("localkit-smoke");
    let db = Db::open(&data_dir.join("localkit.db")).expect("open smoke db");
    AppState {
        db: Mutex::new(db),
        data_dir,
        terminals: localkit_lib::terminal::PtyManager::new(),
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

    println!("M4 SMOKE OK");
}
