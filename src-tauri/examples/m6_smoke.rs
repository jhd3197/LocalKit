//! M6 local domains E2E: enables the shared Caddy router on the smoke site
//! (run `cargo run --example smoke -- create` first), verifies routing +
//! WordPress URL rewrites, then disables again. Does NOT touch the OS trust
//! store (trust_router_ca is exercised manually).
//!
//! NOTE: enabling writes a managed block to the OS hosts file, which triggers
//! an elevation prompt (UAC on Windows) — twice (enable + disable). Run
//! interactively and approve the prompts. The hosts block add/remove logic
//! itself is unit-tested in `router.rs` (`cargo test --lib router`).
//!
//! Usage: cargo run --example m6_smoke

use std::sync::Mutex;

use localkit_lib::{db::Db, docker, router, site, AppState};

const SMOKE_SLUG: &str = "smoke-test";

fn make_state() -> AppState {
    let data_dir = std::env::temp_dir().join("localkit-smoke");
    let db = Db::open(&data_dir.join("localkit.db")).expect("open smoke db");
    AppState {
        db: Mutex::new(db),
        data_dir,
    }
}

fn find_site(state: &AppState) -> site::Site {
    let db = state.db.lock().expect("lock db");
    db.list_sites()
        .expect("list sites")
        .into_iter()
        .find(|s| s.slug == SMOKE_SLUG)
        .expect("smoke site not found — run `cargo run --example smoke -- create` first")
}

/// Resolve the domain explicitly to 127.0.0.1 in case the hosts entries are
/// not in effect — we're testing the router, not OS DNS.
fn http_code(url: &str, host: &str) -> String {
    std::process::Command::new("curl")
        .args([
            "-s",
            "-o",
            "NUL",
            "-w",
            "%{http_code}",
            "--max-time",
            "20",
            "--resolve",
            &format!("{host}:80:127.0.0.1"),
            url,
        ])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|e| format!("curl failed: {e}"))
}

/// Ask Caddy for the site from INSIDE the router container (busybox wget).
/// We do this instead of curling host port 80 because another process
/// (LocalWP's router, IIS, nginx, …) may already hold 80 on the host — that
/// conflict scenario is covered by the friendly-error path in `set_enabled`.
async fn code_via_caddy(router_dir: &std::path::Path, host: &str) -> String {
    let host_header = format!("Host: {host}");
    docker::compose_exec(
        router_dir,
        "caddy",
        &[
            "wget",
            "-q",
            "--server-response",
            "-O",
            "/dev/null",
            "--header",
            &host_header,
            "http://127.0.0.1/",
        ],
    )
    .await
    .map(|_| "200".to_string()) // -q: success means a 2xx after redirects off
    .unwrap_or_else(|e| {
        // wget exits non-zero on 3xx/4xx; the status line is in stderr which
        // docker::compose_exec folds into the error string.
        e.lines()
            .find_map(|l| l.trim().strip_prefix("HTTP/1.1 "))
            .and_then(|s| s.split_whitespace().next())
            .unwrap_or("?")
            .to_string()
    })
}

async fn wp_home(dir: &std::path::Path) -> String {
    docker::compose_run(dir, "wpcli", &["wp", "option", "get", "home"])
        .await
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|e| format!("error: {e}"))
}

#[tokio::main]
async fn main() {
    let status = docker::check().await;
    if !status.available {
        eprintln!("docker unavailable: {:?}", status.error);
        std::process::exit(2);
    }
    let state = make_state();
    let site = find_site(&state);
    let host = format!("{SMOKE_SLUG}.test");

    // -- Enable -------------------------------------------------------------
    let st = router::set_enabled(&state, true)
        .await
        .expect("set_enabled(true) failed");
    println!("after enable: {st:?}");
    assert!(st.enabled, "domains_enabled flag should be on");
    assert!(st.running, "router should be running: {:?}", st.error);

    let router_dir = router::router_dir(&state.data_dir);
    let caddyfile = std::fs::read_to_string(router_dir.join("Caddyfile")).expect("read Caddyfile");
    assert!(
        caddyfile.contains(&format!("http://{host}")) && caddyfile.contains("tls internal"),
        "Caddyfile missing route for {host}:\n{caddyfile}"
    );
    println!("Caddyfile OK:\n{caddyfile}");

    let code = code_via_caddy(&router_dir, &host).await;
    println!("http://{host}/ via caddy -> {code}");
    assert!(
        ["200", "301", "302"].contains(&code.as_str()),
        "router proxy failed: {code}"
    );

    let home = wp_home(&site.dir()).await;
    println!("wp option home = {home}");
    assert_eq!(home, format!("http://{host}"), "siteurl not rewritten to domain");

    // -- Disable ------------------------------------------------------------
    let st = router::set_enabled(&state, false)
        .await
        .expect("set_enabled(false) failed");
    println!("after disable: {st:?}");
    assert!(!st.enabled, "domains_enabled flag should be off");

    let home = wp_home(&site.dir()).await;
    println!("wp option home = {home}");
    assert_eq!(
        home,
        format!("http://localhost:{}", site.port),
        "siteurl not reverted to localhost:<port>"
    );

    // Site must still be reachable directly with the router down.
    let code = http_code(&format!("http://localhost:{}/", site.port), &host);
    println!("http://localhost:{}/ -> {code}", site.port);
    assert!(["200", "301", "302"].contains(&code.as_str()));

    // Remove the router project entirely (keeps the test machine clean).
    let _ = docker::compose_down(&router_dir, true).await;
    let _ = std::fs::remove_dir_all(&router_dir);
    println!("M6 SMOKE OK");
}
