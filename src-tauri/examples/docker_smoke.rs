//! End-to-end smoke test for the generic Docker app kind (plan 22 phase 2).
//! Runs outside the Tauri runtime (no AppHandle; events print to stderr).
//!
//! Usage: cargo run --example docker_smoke [-- run|clean]
//!
//! Imports a trivial two-service compose fixture (an nginx web server + a
//! mariadb) as a `docker` kind site, then asserts the whole generic lifecycle:
//! the app answers HTTP on its published port, the chosen app service is
//! exec-able (what the terminal shells into), a code-only snapshot is taken
//! (no database dump), and stop/start/delete all work.

use std::path::PathBuf;
use std::sync::Mutex;

use localkit_lib::{db::Db, docker, dockerapp, site, snapshot, AppState};

const NAME: &str = "Docker Smoke";
const SLUG: &str = "docker-smoke";

fn data_dir() -> PathBuf {
    std::env::temp_dir().join("localkit-docker-smoke")
}

fn source_dir() -> PathBuf {
    std::env::temp_dir().join("localkit-docker-smoke-src")
}

fn make_state() -> AppState {
    let dir = data_dir();
    std::fs::create_dir_all(&dir).expect("create data dir");
    let db = Db::open(&dir.join("localkit.db")).expect("open db");
    AppState {
        db: Mutex::new(db),
        data_dir: dir,
        terminals: localkit_lib::terminal::PtyManager::new(),
        transfers: Default::default(),
    }
}

/// A free host port for the fixture to publish on.
fn free_port() -> u16 {
    let l = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = l.local_addr().unwrap().port();
    drop(l);
    port
}

fn http_code(url: &str) -> String {
    std::process::Command::new("curl")
        .args(["-s", "-o", "NUL", "-w", "%{http_code}", "--max-time", "20", url])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|e| format!("curl failed: {e}"))
}

/// Write the fixture compose project into a fresh source directory.
fn write_fixture(port: u16) -> PathBuf {
    let src = source_dir();
    let _ = std::fs::remove_dir_all(&src);
    std::fs::create_dir_all(&src).unwrap();
    // A .git dir the copy must exclude, to prove the ignore list works.
    std::fs::create_dir_all(src.join(".git")).unwrap();
    std::fs::write(src.join(".git/HEAD"), b"ref: refs/heads/main").unwrap();
    std::fs::write(
        src.join("docker-compose.yml"),
        format!(
            "services:\n  \
             web:\n    image: nginx:latest\n    ports:\n      - \"{port}:80\"\n  \
             db:\n    image: mariadb:11\n    environment:\n      MARIADB_ROOT_PASSWORD: example\n"
        ),
    )
    .unwrap();
    src
}

async fn find_site(state: &AppState) -> Result<site::Site, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.list_sites()?
        .into_iter()
        .find(|s| s.slug == SLUG || s.slug.starts_with(&format!("{SLUG}-")))
        .ok_or_else(|| "docker smoke site not found".to_string())
}

async fn run(state: &AppState) -> Result<(), String> {
    clean(state).await;
    let port = free_port();
    let src = write_fixture(port);

    // 1. Inspect — the dialog's read-only pass.
    let inspection = dockerapp::inspect(&src).await?;
    println!(
        "INSPECT compose={} services={:?} app={:?}:{:?} db_engine={:?} copy={} B",
        inspection.compose_file,
        inspection.services.iter().map(|s| s.name.clone()).collect::<Vec<_>>(),
        inspection.suggested_service,
        inspection.suggested_port,
        inspection.db_engine,
        inspection.copy_bytes,
    );
    assert_eq!(inspection.suggested_service.as_deref(), Some("web"));
    assert_eq!(inspection.suggested_port, Some(port));
    assert_eq!(inspection.db_engine.as_deref(), Some("mariadb"));
    assert!(inspection.copy_bytes > 0, "copy size should be non-zero");

    // 2. Import — copy + record + up.
    let s = dockerapp::import_project(
        None,
        state,
        NAME.to_string(),
        src.clone(),
        "web".to_string(),
        port,
        false,
    )
    .await?;
    println!(
        "IMPORTED id={} slug={} kind={} app_port={:?}",
        s.id, s.slug, s.kind, s.config.app_port
    );
    assert_eq!(s.kind, site::KIND_DOCKER);
    assert!(s.capabilities.code_sync && s.capabilities.terminal && s.capabilities.domains);
    assert!(
        !s.capabilities.wp_tools && !s.capabilities.one_click_login && !s.capabilities.db_sync,
        "a docker app must not claim WordPress/db capabilities"
    );
    assert_eq!(s.config.service, "web");
    assert_eq!(s.config.app_port, Some(port));

    // 3. The copy is owned, and the ignore list dropped .git; the .env carries
    //    a deterministic compose project name.
    let dir = s.dir();
    assert!(dir.join("docker-compose.yml").is_file(), "compose file copied");
    assert!(!dir.join(".git").exists(), ".git must be excluded from the copy");
    let env = std::fs::read_to_string(dir.join(".env")).unwrap_or_default();
    assert!(
        env.contains(&format!("COMPOSE_PROJECT_NAME=localkit-{SLUG}")),
        ".env should set COMPOSE_PROJECT_NAME, got: {env:?}"
    );

    // 4. The app answers HTTP on its published port.
    let url = format!("http://localhost:{port}");
    let code = http_code(&format!("{url}/"));
    println!("HTTP {url}/ -> {code}");
    assert!(
        ["200", "301", "302", "304"].contains(&code.as_str()),
        "app did not answer on its port: {code}"
    );

    // 5. The chosen app service is exec-able — this is exactly what the terminal
    //    shells into (`docker compose exec web bash`).
    let echoed = docker::compose_exec(&dir, "web", &["echo", "localkit-ok"]).await?;
    assert!(echoed.contains("localkit-ok"), "exec into `web` failed: {echoed:?}");
    println!("terminal target `web` is exec-able");

    // 6. A code-only snapshot: no database dump (db_sync is off for docker).
    let snap = snapshot::create(None, state, &s.id, snapshot::KIND_MANUAL, Some("smoke".into())).await?;
    println!("SNAPSHOT id={} db_bytes={} code_bytes={}", snap.id, snap.db_bytes, snap.code_bytes);
    assert_eq!(snap.db_bytes, 0, "a docker snapshot must be code-only");
    assert!(snap.code_bytes > 0, "the code archive should be non-empty");
    assert!(
        snapshot::list(state, &s.id)?.iter().any(|x| x.id == snap.id),
        "the snapshot should be listed"
    );

    // 7. Lifecycle: stop then start.
    let stopped = site::stop(state, &s.id).await?;
    assert_eq!(stopped.status, "stopped");
    let started = site::start(state, &s.id).await?;
    assert_eq!(started.status, "running");
    println!("stop/start OK");

    // 8. Delete removes everything.
    site::delete(None, state, &s.id, true).await?;
    assert!(!dir.exists(), "site dir should be gone after delete");
    assert!(find_site(state).await.is_err(), "db row should be gone after delete");
    println!("DOCKER SMOKE OK on {url}");

    let _ = std::fs::remove_dir_all(&src);
    Ok(())
}

async fn clean(state: &AppState) {
    let sites = {
        let db = state.db.lock().expect("lock db");
        db.list_sites().unwrap_or_default()
    };
    for s in sites {
        if s.slug == SLUG || s.slug.starts_with(&format!("{SLUG}-")) {
            let _ = site::delete(None, state, &s.id, true).await;
            println!("cleaned {}", s.slug);
        }
    }
    let orphan = state.data_dir.join("sites").join(SLUG);
    if orphan.exists() {
        let _ = docker::compose_down(&orphan, true).await;
        let _ = std::fs::remove_dir_all(&orphan);
    }
    let _ = std::fs::remove_dir_all(source_dir());
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
        "clean" => {
            clean(&state).await;
            Ok(())
        }
        other => Err(format!("unknown command: {other}")),
    };
    if let Err(e) = result {
        eprintln!("DOCKER SMOKE {cmd} FAILED: {e}");
        std::process::exit(1);
    }
}
