//! `lk` — the LocalKit CLI.
//!
//! Headless companion for the LocalKit desktop app: a thin wrapper over
//! `localkit_lib` that shares the GUI's data dir and SQLite database, so both
//! always see the same sites. Lives in its own workspace crate (not a [[bin]]
//! in the GUI package — that breaks the macOS universal bundler).
//!
//! Conventions (shared with the sister faro-cli / serverkit CLIs):
//! - stdout carries data only; chrome (✓ successes, → hints, ! warnings,
//!   progress) goes to stderr, so `lk list --json | jq` is always clean.
//! - `--json` is per-command, always pretty-printed, raw payload (no envelope).
//! - Errors print `error: <msg>` in red on stderr, exit code 1.
//! - Sites are addressed by exact id, or case-insensitive slug or name.
//! - Destructive commands prompt (default No); `--yes` skips the prompt and
//!   is required when not on a TTY.

use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use clap::{Parser, Subcommand, ValueEnum};
use localkit_lib::{db::Db, docker, router, site, snapshot, wordpress, AppState};

// ---------------------------------------------------------------------------
// clap surface
// ---------------------------------------------------------------------------

#[derive(Parser)]
#[command(
    name = "lk",
    version,
    about = "LocalKit CLI — manage local WordPress sites from the terminal"
)]
struct Cli {
    /// Override the LocalKit data directory
    #[arg(long, global = true, env = "LOCALKIT_DATA_DIR")]
    data_dir: Option<PathBuf>,

    /// Disable colored output (also disabled when NO_COLOR is set)
    #[arg(long, global = true)]
    no_color: bool,

    #[command(subcommand)]
    command: Cmd,
}

#[derive(Clone, Copy, ValueEnum)]
enum Shell {
    Bash,
    Powershell,
}

#[derive(Subcommand)]
enum Cmd {
    /// List all sites (slug, live status, URL, versions)
    List {
        /// Output machine-readable JSON
        #[arg(long)]
        json: bool,
    },

    /// Create a new site (pulls Docker images on first run).
    /// Prints the site URL on stdout; progress goes to stderr.
    Create {
        /// Site name, e.g. "My Blog"
        name: String,
        /// WordPress version (allowlist lives in the app)
        #[arg(long)]
        wp_version: Option<String>,
        /// PHP version (allowlist lives in the app)
        #[arg(long)]
        php_version: Option<String>,
        /// Output the created site as machine-readable JSON
        #[arg(long)]
        json: bool,
    },

    /// Start a site
    Start { site: String },

    /// Stop a site
    Stop { site: String },

    /// Restart a site
    Restart { site: String },

    /// Delete a site (removes containers, volumes, and files).
    /// A restorable snapshot is kept unless --delete-snapshots is passed.
    /// Prompts for confirmation unless --yes; --yes is required non-interactively.
    Delete {
        site: String,
        /// Skip the confirmation prompt
        #[arg(long)]
        yes: bool,
        /// Also delete this site's snapshots (they are kept by default)
        #[arg(long)]
        delete_snapshots: bool,
    },

    /// Manage point-in-time snapshots (database + wp-content) of a site
    #[command(subcommand)]
    Snapshot(SnapshotCmd),

    /// Show site details, including DB credentials
    Info {
        site: String,
        /// Output machine-readable JSON
        #[arg(long)]
        json: bool,
    },

    /// Show container logs for a site
    Logs {
        site: String,
        #[arg(long, default_value_t = 100)]
        tail: u32,
    },

    /// Run a wp-cli command inside a site, e.g. `lk wp mysite plugin list`.
    /// The `wp` prefix is added for you.
    Wp {
        site: String,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true, required = true)]
        args: Vec<String>,
    },

    /// Print eval-able shell exports for a site (URL + DB credentials).
    /// Exports go to stdout, so `eval $(lk env mysite)` just works.
    Env {
        site: String,
        #[arg(long, value_enum, default_value_t = Shell::Bash)]
        shell: Shell,
        /// Output machine-readable JSON
        #[arg(long)]
        json: bool,
    },

    /// Print a one-time URL that logs straight into the site's wp-admin.
    /// The URL goes to stdout (scriptable); it expires after ~2 minutes and
    /// works exactly once.
    Login {
        site: String,
        /// User to log in as (id, login, or email); defaults to the site admin
        #[arg(long)]
        user: Option<String>,
        /// Also open the URL in the default browser
        #[arg(long)]
        open: bool,
    },

    /// Diagnose the local environment (Docker, compose, data dir).
    /// Exits non-zero while any check fails, so it can gate scripts.
    Doctor,
}

#[derive(Subcommand)]
enum SnapshotCmd {
    /// List a site's snapshots, newest first
    List {
        site: String,
        /// Output machine-readable JSON
        #[arg(long)]
        json: bool,
    },

    /// Take a snapshot now. Prints the new snapshot id on stdout.
    Create {
        site: String,
        /// Optional note stored in the snapshot's manifest
        #[arg(long)]
        note: Option<String>,
        /// Output the created snapshot as machine-readable JSON
        #[arg(long)]
        json: bool,
    },

    /// Restore a site to a snapshot (destructive — snapshots first, then
    /// replaces the database and wp-content). Prompts unless --yes.
    Restore {
        site: String,
        /// Snapshot id from `lk snapshot list`
        snapshot: String,
        /// Skip the confirmation prompt
        #[arg(long)]
        yes: bool,
    },

    /// Delete one snapshot. Prompts unless --yes.
    Delete {
        site: String,
        /// Snapshot id from `lk snapshot list`
        snapshot: String,
        /// Skip the confirmation prompt
        #[arg(long)]
        yes: bool,
    },
}

// ---------------------------------------------------------------------------
// main / dispatch
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    NO_COLOR_FLAG.store(cli.no_color, std::sync::atomic::Ordering::Relaxed);
    if let Err(e) = run(&cli).await {
        eprintln!("{} {e}", red("error:"));
        std::process::exit(1);
    }
}

async fn run(cli: &Cli) -> Result<(), String> {
    // `doctor` works without opening the DB.
    if let Cmd::Doctor = cli.command {
        return cmd_doctor(cli.data_dir.clone()).await;
    }

    let state = make_state(cli)?;

    match &cli.command {
        Cmd::List { json } => cmd_list(&state, *json).await,
        Cmd::Create {
            name,
            wp_version,
            php_version,
            json,
        } => cmd_create(&state, name, wp_version, php_version, *json).await,
        Cmd::Start { site: q } => {
            let s = resolve(&state, q)?;
            let s = site::start(&state, &s.id).await?;
            eprintln!("{} {} started", ok("✓"), bold(&s.name));
            println!("{}", site_url(&s));
            Ok(())
        }
        Cmd::Stop { site: q } => {
            let s = resolve(&state, q)?;
            let s = site::stop(&state, &s.id).await?;
            eprintln!("{} {} stopped", ok("✓"), bold(&s.name));
            Ok(())
        }
        Cmd::Restart { site: q } => {
            let s = resolve(&state, q)?;
            // stop + start (not `compose restart`) so the DB status stays correct.
            site::stop(&state, &s.id).await?;
            let s = site::start(&state, &s.id).await?;
            eprintln!("{} {} restarted", ok("✓"), bold(&s.name));
            println!("{}", site_url(&s));
            Ok(())
        }
        Cmd::Delete {
            site: q,
            yes,
            delete_snapshots,
        } => cmd_delete(&state, q, *yes, *delete_snapshots).await,
        Cmd::Snapshot(sub) => cmd_snapshot(&state, sub).await,
        Cmd::Info { site: q, json } => cmd_info(&state, q, *json),
        Cmd::Logs { site: q, tail } => {
            let s = resolve(&state, q)?;
            let logs = site::logs(&state, &s.id, *tail).await?;
            print!("{logs}");
            Ok(())
        }
        Cmd::Wp { site: q, args } => {
            let s = resolve(&state, q)?;
            let mut full: Vec<&str> = vec!["wp"];
            full.extend(args.iter().map(String::as_str));
            let out = docker::compose_run(&s.dir(), "wpcli", &full).await?;
            print!("{out}");
            Ok(())
        }
        Cmd::Env { site: q, shell, json } => cmd_env(&state, q, *shell, *json),
        Cmd::Login { site: q, user, open } => cmd_login(&state, q, user.as_deref(), *open).await,
        Cmd::Doctor => unreachable!("handled above"),
    }
}

// ---------------------------------------------------------------------------
// Subcommands
// ---------------------------------------------------------------------------

async fn cmd_list(state: &AppState, json: bool) -> Result<(), String> {
    let sites = site::list(state).await?;
    if json {
        return print_json(&sites);
    }
    if sites.is_empty() {
        eprintln!("{} no sites yet. create one with `lk create <name>`.", info("→"));
        return Ok(());
    }
    let rows: Vec<[String; 4]> = sites
        .iter()
        .map(|s| {
            [
                s.site.slug.clone(),
                s.live_status.clone(),
                site_url(&s.site),
                format!("WP {} / PHP {}", s.site.wp_version, s.site.php_version),
            ]
        })
        .collect();
    let headers = ["SLUG", "STATUS", "URL", "VERSION"];
    let mut w = [0usize; 4];
    for (i, h) in headers.iter().enumerate() {
        w[i] = h.len();
    }
    for r in &rows {
        for (i, c) in r.iter().enumerate() {
            w[i] = w[i].max(c.len());
        }
    }
    for (i, h) in headers.iter().enumerate() {
        print!("{:<w$}  ", dim(h), w = w[i]);
    }
    println!();
    for r in &rows {
        for (i, c) in r.iter().enumerate() {
            // Pad first, then colorize, so ANSI codes don't break alignment.
            let padded = format!("{:<w$}", c, w = w[i]);
            let cell = match (i, c.as_str()) {
                (1, "running") => ok(&padded),
                (1, _) => dim(&padded),
                _ => padded,
            };
            print!("{cell}  ");
        }
        println!();
    }
    Ok(())
}

async fn cmd_create(
    state: &AppState,
    name: &str,
    wp_version: &Option<String>,
    php_version: &Option<String>,
    json: bool,
) -> Result<(), String> {
    let wp = wp_version
        .clone()
        .unwrap_or_else(|| site::WP_VERSIONS[0].into());
    let php = php_version
        .clone()
        .unwrap_or_else(|| site::PHP_VERSIONS[0].into());
    let site = site::create(None, state, name.to_string(), wp, php).await?;
    if json {
        print_json(&site)?;
    } else {
        // stdout carries the URL (scriptable); chrome stays on stderr.
        println!("{}", site_url(&site));
    }
    eprintln!("{} {} is running", ok("✓"), bold(&site.name));
    eprintln!(
        "{} admin credentials: {} / {}",
        info("→"),
        site.admin_user,
        site.admin_pass
    );
    Ok(())
}

/// Does `query` name a deleted site whose snapshots are still on disk?
/// Only an exact site id can match — there is no sites row left to map a
/// slug through.
fn orphan_snapshots_exist(state: &AppState, query: &str) -> bool {
    snapshot::site_snapshots_dir(&state.data_dir, query).is_dir()
}

/// Destructive-command gate: prompt with a No default unless `--yes`, and
/// require `--yes` when there is no TTY to prompt on.
fn confirm(yes: bool, question: &str, non_tty_hint: &str) -> Result<(), String> {
    if yes {
        return Ok(());
    }
    if !std::io::stdout().is_terminal() {
        return Err(non_tty_hint.to_string());
    }
    eprint!("{} {question} [y/N] ", warn("!"));
    let mut line = String::new();
    use std::io::BufRead;
    // EOF/no-tty falls through to the No path.
    let read = std::io::stdin().lock().read_line(&mut line);
    if read.is_err() || !matches!(line.trim().to_lowercase().as_str(), "y" | "yes") {
        return Err("aborted".into());
    }
    Ok(())
}

async fn cmd_delete(
    state: &AppState,
    query: &str,
    yes: bool,
    delete_snapshots: bool,
) -> Result<(), String> {
    let s = resolve(state, query)?;
    let tail = if delete_snapshots {
        "this removes its containers, volumes, files AND snapshots."
    } else {
        "this removes its containers, volumes, and files (a snapshot is kept)."
    };
    confirm(
        yes,
        &format!("delete `{}`? {tail}", s.slug),
        &format!(
            "`lk delete` removes `{}` permanently. pass --yes to confirm.",
            s.slug
        ),
    )?;
    site::delete(None, state, &s.id, delete_snapshots).await?;
    eprintln!("{} {} deleted", ok("✓"), bold(&s.name));
    if !delete_snapshots {
        eprintln!(
            "{} snapshots kept — `lk snapshot list {}` still lists them",
            info("→"),
            s.id
        );
    }
    Ok(())
}

async fn cmd_snapshot(state: &AppState, cmd: &SnapshotCmd) -> Result<(), String> {
    match cmd {
        SnapshotCmd::List { site: q, json } => {
            // Listing tolerates a site that no longer exists: deleting a site
            // keeps its snapshots, and their manifests carry the name/slug, so
            // `lk snapshot list <site id>` stays useful afterwards. (Restore
            // and delete still require a live site — there is nothing to
            // restore *into*.)
            let (id, label) = match resolve(state, q) {
                Ok(s) => (s.id, s.slug),
                Err(_) if orphan_snapshots_exist(state, q) => (q.to_string(), q.to_string()),
                Err(e) => return Err(e),
            };
            let snaps = snapshot::list(state, &id)?;
            if *json {
                return print_json(&snaps);
            }
            if snaps.is_empty() {
                eprintln!(
                    "{} no snapshots for `{label}` yet. take one with `lk snapshot create {label}`.",
                    info("→"),
                );
                return Ok(());
            }
            let rows: Vec<[String; 5]> = snaps
                .iter()
                .map(|x| {
                    [
                        x.id.clone(),
                        short_time(&x.created_at),
                        x.kind.clone(),
                        format!("{} + {}", human_bytes(x.db_bytes), human_bytes(x.code_bytes)),
                        x.note.clone(),
                    ]
                })
                .collect();
            print_table(&["ID", "CREATED", "KIND", "DB + CODE", "NOTE"], &rows);
            Ok(())
        }

        SnapshotCmd::Create { site: q, note, json } => {
            let s = resolve(state, q)?;
            let snap = snapshot::create(
                None,
                state,
                &s.id,
                snapshot::KIND_MANUAL,
                note.clone(),
            )
            .await?;
            if *json {
                print_json(&snap)?;
            } else {
                // stdout carries the id (scriptable); chrome stays on stderr.
                println!("{}", snap.id);
            }
            eprintln!(
                "{} snapshot of {} taken ({} database, {} wp-content)",
                ok("✓"),
                bold(&s.name),
                human_bytes(snap.db_bytes),
                human_bytes(snap.code_bytes)
            );
            Ok(())
        }

        SnapshotCmd::Restore {
            site: q,
            snapshot: id,
            yes,
        } => {
            let s = resolve(state, q)?;
            confirm(
                *yes,
                &format!(
                    "restore `{}` to snapshot {id}? this replaces its database and wp-content \
                     (a pre-restore snapshot is taken first).",
                    s.slug
                ),
                &format!(
                    "`lk snapshot restore` overwrites `{}`. pass --yes to confirm.",
                    s.slug
                ),
            )?;
            let message = snapshot::restore(None, state, &s.id, id).await?;
            eprintln!("{} {message}", ok("✓"));
            Ok(())
        }

        SnapshotCmd::Delete {
            site: q,
            snapshot: id,
            yes,
        } => {
            let s = resolve(state, q)?;
            confirm(
                *yes,
                &format!("delete snapshot {id} of `{}`? this cannot be undone.", s.slug),
                &format!("`lk snapshot delete` removes snapshot {id} permanently. pass --yes to confirm."),
            )?;
            snapshot::delete(state, &s.id, id)?;
            eprintln!("{} snapshot {id} deleted", ok("✓"));
            Ok(())
        }
    }
}

fn cmd_info(state: &AppState, query: &str, json: bool) -> Result<(), String> {
    let s = resolve(state, query)?;
    let d = site::detail(state, &s.id)?;
    if json {
        return print_json(&d);
    }
    let rows = [
        ("Name", d.site.name.clone()),
        ("Slug", d.site.slug.clone()),
        ("Status", d.live_status.clone()),
        ("URL", site_url(&d.site)),
        ("Path", d.site.path.clone()),
        ("WordPress", d.site.wp_version.clone()),
        ("PHP", d.site.php_version.clone()),
        ("Admin user", d.site.admin_user.clone()),
        ("Admin pass", d.site.admin_pass.clone()),
        ("DB host", format!("{}:{}", d.db_host, d.db_port)),
        ("DB name", d.db_name.clone()),
        ("DB user", d.db_user.clone()),
        ("DB password", d.db_password.clone()),
    ];
    for (k, v) in rows {
        println!("{:<12} {}", dim(&format!("{k}:")), v);
    }
    Ok(())
}

fn cmd_env(state: &AppState, query: &str, shell: Shell, json: bool) -> Result<(), String> {
    let s = resolve(state, query)?;
    let d = site::detail(state, &s.id)?;
    let pairs: Vec<(String, String)> = vec![
        ("LOCALKIT_SITE_URL".into(), site_url(&d.site)),
        ("DB_HOST".into(), d.db_host.clone()),
        ("DB_PORT".into(), d.db_port.to_string()),
        ("DB_NAME".into(), d.db_name.clone()),
        ("DB_USER".into(), d.db_user.clone()),
        ("DB_PASSWORD".into(), d.db_password.clone()),
    ];
    if json {
        let map: serde_json::Map<String, serde_json::Value> = pairs
            .into_iter()
            .map(|(k, v)| (k, serde_json::Value::String(v)))
            .collect();
        return print_json(&serde_json::Value::Object(map));
    }
    // Exports go to stdout (for eval), the hint to stderr.
    print!("{}", render_exports(shell, &pairs));
    match shell {
        Shell::Bash => eprintln!("{} run: eval $(lk env {})", info("→"), s.slug),
        Shell::Powershell => eprintln!(
            "{} run: lk env {} --shell powershell | Invoke-Expression",
            info("→"),
            s.slug
        ),
    }
    Ok(())
}

async fn cmd_login(state: &AppState, query: &str, user: Option<&str>, open: bool) -> Result<(), String> {
    let s = resolve(state, query)?;
    let base = router::site_public_url(state, &s);
    // Thin wrapper: all logic lives in localkit_lib::wordpress.
    let url = wordpress::login_url(&s.dir(), &s, user, &base).await?;
    println!("{url}");
    if open {
        open::that(&url).map_err(|e| format!("failed to open the browser: {e}"))?;
        eprintln!("{} opened one-time login URL in your browser", ok("✓"));
    }
    Ok(())
}

async fn cmd_doctor(data_dir_override: Option<PathBuf>) -> Result<(), String> {    let mut ok = true;

    let status = docker::check().await;
    match (&status.available, &status.version) {
        (true, Some(v)) => check_line(true, &format!("docker daemon reachable (server v{v})")),
        _ => {
            check_line(false, "docker daemon reachable");
            eprintln!(
                "  {}",
                status.error.unwrap_or_else(|| "unknown error".into())
            );
            ok = false;
        }
    }

    match tokio::process::Command::new("docker")
        .args(["compose", "version", "--short"])
        .output()
        .await
    {
        Ok(out) if out.status.success() => check_line(
            true,
            &format!(
                "docker compose plugin ({})",
                String::from_utf8_lossy(&out.stdout).trim()
            ),
        ),
        _ => {
            check_line(false, "docker compose plugin");
            ok = false;
        }
    }

    let data_dir = data_dir_override.unwrap_or_else(default_data_dir);
    let probe = data_dir.join(".lk-doctor");
    let writable = std::fs::create_dir_all(&data_dir)
        .and_then(|_| std::fs::write(&probe, b""))
        .and_then(|_| std::fs::remove_file(&probe))
        .is_ok();
    check_line(
        writable,
        &format!("data dir writable ({})", data_dir.display()),
    );
    ok &= writable;

    ok &= doctor_router(&data_dir).await;

    if !ok {
        return Err("one or more checks failed".into());
    }
    Ok(())
}

/// Local-domains section of `doctor` (plan 16): active router mode + who owns
/// the router ports, so "my .test sites show someone else's 404" has a
/// copy-paste answer. Best-effort — a missing DB just means "not configured".
async fn doctor_router(data_dir: &Path) -> bool {
    let Ok(db) = Db::open(&data_dir.join("localkit.db")) else {
        check_line(true, "local domains not configured yet (no database)");
        return true;
    };
    let state = AppState {
        db: Mutex::new(db),
        data_dir: data_dir.to_path_buf(),
        terminals: localkit_lib::terminal::PtyManager::new(),
    };

    let ports = router::router_ports(&state);
    let mode = if ports.is_default() { "default" } else { "fallback" };
    let Ok(status) = router::status(&state).await else {
        check_line(false, "local domains status unavailable");
        return false;
    };

    if !status.enabled {
        check_line(true, "local domains disabled — sites use localhost:<port>");
        return true;
    }

    check_line(
        status.running,
        &format!(
            "local domains enabled — router on ports {}/{} ({mode}), {}",
            ports.http,
            ports.https,
            if status.running { "running" } else { "NOT running" }
        ),
    );

    if status.running {
        // Our own Caddy owns the ports; say so rather than probing and
        // reporting LocalKit as its own conflict.
        eprintln!("  ports {}/{} held by LocalKit's router", ports.http, ports.https);
        return true;
    }

    for c in router::probe_ports(ports.http, ports.https).await {
        match c.process {
            Some(p) => eprintln!("  port {} held by {p}", c.port),
            None => eprintln!("  port {} in use by an unidentified process", c.port),
        }
    }
    eprintln!(
        "  {} quit the other program, or set fallback ports in Settings → Local domains",
        info("→")
    );
    false
}

// ---------------------------------------------------------------------------
// State / data dir
// ---------------------------------------------------------------------------

fn make_state(cli: &Cli) -> Result<AppState, String> {
    let data_dir = cli.data_dir.clone().unwrap_or_else(default_data_dir);
    std::fs::create_dir_all(&data_dir).map_err(|e| format!("failed to create data dir: {e}"))?;
    let db = Db::open(&data_dir.join("localkit.db"))?;
    Ok(AppState {
        db: Mutex::new(db),
        data_dir,
        terminals: localkit_lib::terminal::PtyManager::new(),
    })
}

/// Same default as the GUI: `<platform data dir>/LocalKit`.
fn default_data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("LocalKit")
}

// ---------------------------------------------------------------------------
// Site resolution
// ---------------------------------------------------------------------------

fn resolve(state: &AppState, query: &str) -> Result<site::Site, String> {
    let sites = {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        db.list_sites()?
    };
    pick(sites, query)
}

/// Exact id wins, then case-insensitive slug, then case-insensitive name.
/// Ambiguity tells you to pass the id; no match lists what's available.
fn pick(sites: Vec<site::Site>, query: &str) -> Result<site::Site, String> {
    if let Some(s) = sites.iter().find(|s| s.id == query) {
        return Ok(s.clone());
    }
    let q = query.to_lowercase();
    let slug_hits: Vec<&site::Site> = sites.iter().filter(|s| s.slug.to_lowercase() == q).collect();
    if slug_hits.len() == 1 {
        return Ok(slug_hits[0].clone());
    }
    let name_hits: Vec<&site::Site> = sites.iter().filter(|s| s.name.to_lowercase() == q).collect();
    let hits = if slug_hits.len() > 1 { slug_hits } else { name_hits };
    match hits.len() {
        0 => {
            let available = sites
                .iter()
                .map(|s| s.slug.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            if available.is_empty() {
                Err(format!(
                    "no site named `{query}` — there are no sites yet. create one with `lk create <name>`."
                ))
            } else {
                Err(format!(
                    "no site named `{query}`. available: {available}"
                ))
            }
        }
        1 => Ok(hits[0].clone()),
        _ => Err(format!(
            "`{query}` matches more than one site ({}). pass the exact id from `lk list --json`.",
            hits.iter().map(|s| s.slug.as_str()).collect::<Vec<_>>().join(", ")
        )),
    }
}

// ---------------------------------------------------------------------------
// Output helpers (pure / testable)
// ---------------------------------------------------------------------------

fn site_url(s: &site::Site) -> String {
    format!("http://localhost:{}", s.port)
}

/// RFC3339 down to seconds for table display — the stored timestamps carry
/// sub-second precision and an offset, which is noise in a column.
/// `--json` keeps the full value.
fn short_time(rfc3339: &str) -> String {
    match rfc3339.split_once('T') {
        Some((date, rest)) => {
            let time: String = rest.chars().take(8).collect();
            format!("{date} {time}")
        }
        None => rfc3339.to_string(),
    }
}

/// Byte counts for humans — snapshot archives run from KB to GB.
fn human_bytes(n: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut value = n as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{n} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// Left-aligned column table with dimmed headers (stdout — it is data).
fn print_table<const N: usize>(headers: &[&str; N], rows: &[[String; N]]) {
    let mut w = [0usize; N];
    for (i, h) in headers.iter().enumerate() {
        w[i] = h.len();
    }
    for r in rows {
        for (i, c) in r.iter().enumerate() {
            w[i] = w[i].max(c.len());
        }
    }
    for (i, h) in headers.iter().enumerate() {
        // Pad first, then colorize, so ANSI codes don't break alignment.
        print!("{}  ", dim(&format!("{:<width$}", h, width = w[i])));
    }
    println!();
    for r in rows {
        for (i, c) in r.iter().enumerate() {
            print!("{:<width$}  ", c, width = w[i]);
        }
        println!();
    }
}

/// Render eval-able export lines for a shell.
fn render_exports(shell: Shell, pairs: &[(String, String)]) -> String {
    let mut out = String::new();
    for (k, v) in pairs {
        match shell {
            Shell::Bash => out.push_str(&format!("export {k}=\"{v}\"\n")),
            Shell::Powershell => out.push_str(&format!("$env:{k} = \"{v}\"\n")),
        }
    }
    out
}

fn print_json<T: serde::Serialize>(v: &T) -> Result<(), String> {
    println!(
        "{}",
        serde_json::to_string_pretty(v).map_err(|e| e.to_string())?
    );
    Ok(())
}

fn check_line(ok: bool, msg: &str) {
    if ok {
        println!("{} {msg}", self::ok("✓"));
    } else {
        println!("{} {msg}", err_mark("✗"));
    }
}

// ---------------------------------------------------------------------------
// Styling (hand-rolled ANSI; disabled by --no-color, NO_COLOR, or non-TTY)
// ---------------------------------------------------------------------------

static NO_COLOR_FLAG: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn colors_enabled() -> bool {
    !NO_COLOR_FLAG.load(std::sync::atomic::Ordering::Relaxed)
        && std::env::var_os("NO_COLOR").is_none()
        && std::io::stdout().is_terminal()
}

fn paint(code: &str, s: &str) -> String {
    if colors_enabled() {
        format!("\x1b[{code}m{s}\x1b[0m")
    } else {
        s.to_string()
    }
}

fn ok(s: &str) -> String {
    paint("32", s)
}
fn err_mark(s: &str) -> String {
    paint("31", s)
}
fn red(s: &str) -> String {
    paint("31", s)
}
fn warn(s: &str) -> String {
    paint("33", s)
}
fn info(s: &str) -> String {
    paint("36", s)
}
fn dim(s: &str) -> String {
    paint("2", s)
}
fn bold(s: &str) -> String {
    paint("1", s)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn site(id: &str, slug: &str, name: &str) -> site::Site {
        site::Site {
            id: id.into(),
            name: name.into(),
            slug: slug.into(),
            path: format!("/tmp/{slug}"),
            port: 8081,
            wp_version: "6.7".into(),
            php_version: "8.3".into(),
            status: "running".into(),
            admin_user: "admin".into(),
            admin_pass: "secret".into(),
            created_at: "2026-01-01T00:00:00Z".into(),
        }
    }

    fn sample_sites() -> Vec<site::Site> {
        vec![
            site("id-aaa", "blog", "My Blog"),
            site("id-bbb", "shop", "Shop"),
        ]
    }

    #[test]
    fn pick_exact_id_wins() {
        let s = pick(sample_sites(), "id-bbb").unwrap();
        assert_eq!(s.slug, "shop");
    }

    #[test]
    fn pick_slug_case_insensitive() {
        let s = pick(sample_sites(), "BLOG").unwrap();
        assert_eq!(s.id, "id-aaa");
    }

    #[test]
    fn pick_name_case_insensitive() {
        let s = pick(sample_sites(), "my blog").unwrap();
        assert_eq!(s.id, "id-aaa");
    }

    #[test]
    fn pick_no_match_lists_available() {
        let err = pick(sample_sites(), "nope").unwrap_err();
        assert!(err.contains("no site named `nope`"));
        assert!(err.contains("blog") && err.contains("shop"));
    }

    #[test]
    fn pick_no_sites_suggests_create() {
        let err = pick(vec![], "nope").unwrap_err();
        assert!(err.contains("lk create"));
    }

    #[test]
    fn pick_ambiguous_names_ask_for_id() {
        let sites = vec![site("id-1", "blog", "Dup"), site("id-2", "blog-2", "dup")];
        let err = pick(sites, "dup").unwrap_err();
        assert!(err.contains("more than one site"));
    }

    #[test]
    fn exports_bash() {
        let out = render_exports(
            Shell::Bash,
            &[("DB_HOST".into(), "127.0.0.1".into())],
        );
        assert_eq!(out, "export DB_HOST=\"127.0.0.1\"\n");
    }

    #[test]
    fn short_time_drops_subseconds_and_offset() {
        assert_eq!(
            short_time("2026-07-20T18:23:53.160418100+00:00"),
            "2026-07-20 18:23:53"
        );
    }

    #[test]
    fn short_time_passes_through_anything_unexpected() {
        assert_eq!(short_time("not a timestamp"), "not a timestamp");
    }

    #[test]
    fn human_bytes_stays_exact_under_a_kilobyte() {
        assert_eq!(human_bytes(0), "0 B");
        assert_eq!(human_bytes(1023), "1023 B");
    }

    #[test]
    fn human_bytes_scales_up() {
        assert_eq!(human_bytes(1024), "1.0 KB");
        assert_eq!(human_bytes(1024 * 1024 * 3 / 2), "1.5 MB");
        assert_eq!(human_bytes(5 * 1024 * 1024 * 1024), "5.0 GB");
    }

    #[test]
    fn exports_powershell() {
        let out = render_exports(
            Shell::Powershell,
            &[("DB_PORT".into(), "18081".into())],
        );
        assert_eq!(out, "$env:DB_PORT = \"18081\"\n");
    }
}
