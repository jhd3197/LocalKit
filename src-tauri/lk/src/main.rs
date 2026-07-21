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

use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
use clap_complete::Shell as CompletionShell;
use localkit_lib::serverkit::{self, ServerKitConnection};
use localkit_lib::sync::{self, SyncRecord};
use localkit_lib::{blueprint, db::Db, docker, router, site, snapshot, wordpress, AppState};

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

    /// List the WordPress sites on a ServerKit server (read-only).
    Sites {
        /// ServerKit connection to query (exact id, or case-insensitive name)
        #[arg(long)]
        remote: String,
        /// Output machine-readable JSON
        #[arg(long)]
        json: bool,
    },

    /// Manage ServerKit connections (add, list, test, remove)
    #[command(subcommand)]
    Connection(ConnectionCmd),

    /// Push a local site's code and/or database to its ServerKit remote.
    /// `--connection`/`--remote-site` are only needed when the site has no
    /// linked remote (imported sites carry one). Exit 2 = the server rejected it.
    Push {
        /// Local site (exact id, or case-insensitive slug or name)
        site: String,
        /// Push wp-content
        #[arg(long)]
        code: bool,
        /// Push the database (site must be running)
        #[arg(long)]
        db: bool,
        /// ServerKit connection (defaults to the site's linked remote)
        #[arg(long)]
        connection: Option<String>,
        /// Remote site to target (numeric id or name; defaults to the link)
        #[arg(long)]
        remote_site: Option<String>,
        /// Print the resulting sync record(s) as machine-readable JSON
        #[arg(long)]
        json: bool,
    },

    /// Pull a database from a local site's ServerKit remote into it (destructive
    /// — a pre-pull snapshot is taken first). Exit 2 = the server rejected it.
    /// To bring a remote site down as a NEW local site, use `lk import`.
    Pull {
        /// Local site (exact id, or case-insensitive slug or name)
        site: String,
        /// Pull the database (the only pull; the site must be running)
        #[arg(long)]
        db: bool,
        /// ServerKit connection (defaults to the site's linked remote)
        #[arg(long)]
        connection: Option<String>,
        /// Remote site to target (numeric id or name; defaults to the link)
        #[arg(long)]
        remote_site: Option<String>,
        /// Print the resulting sync record as machine-readable JSON
        #[arg(long)]
        json: bool,
    },

    /// Create a new site (pulls Docker images on first run).
    /// Prints the site URL on stdout; progress goes to stderr.
    Create {
        /// Site name, e.g. "My Blog" (defaults to the blueprint name with --blueprint)
        name: Option<String>,
        /// WordPress version (allowlist lives in the app; ignored with --blueprint)
        #[arg(long)]
        wp_version: Option<String>,
        /// PHP version (allowlist lives in the app; ignored with --blueprint)
        #[arg(long)]
        php_version: Option<String>,
        /// Create from a saved blueprint (its id or name) instead of a blank install
        #[arg(long)]
        blueprint: Option<String>,
        /// Output the created site as machine-readable JSON
        #[arg(long)]
        json: bool,
    },

    /// Clone an existing local site into a NEW one (copies its database and
    /// wp-content, with fresh ports and DB credentials). Prints the new site's
    /// URL on stdout; progress goes to stderr.
    Clone {
        /// Source site (exact id, or case-insensitive slug or name)
        site: String,
        /// Name for the new cloned site
        new_name: String,
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

    /// Finish a half-created site (a create killed mid-install)
    Resume { site: String },

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

    /// Manage reusable site blueprints (save one, list, delete, share)
    #[command(subcommand)]
    Blueprint(BlueprintCmd),

    /// Clone a site from a ServerKit server down as a NEW local site.
    /// Downloads its wp-content and database, rewrites URLs to the local one,
    /// and leaves the site running. Prints the new site's URL on stdout.
    Import {
        /// ServerKit connection (exact id, or case-insensitive label)
        connection: String,
        /// Remote site (numeric id from the server, or its case-insensitive name)
        site: String,
        /// Name for the new local site (defaults to the remote site's name)
        #[arg(long)]
        name: Option<String>,
        /// Output the created site as machine-readable JSON
        #[arg(long)]
        json: bool,
    },

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

    /// Diagnose the local environment (Docker, compose, data dir) plus every
    /// stored ServerKit connection. Exits non-zero while any local check fails,
    /// so it can gate scripts; a connection being down is reported but does not
    /// flip the exit code (a remote outage is not a local misconfiguration).
    Doctor,

    /// Print a shell completion script for `lk` to stdout.
    /// e.g. `lk completions bash > /etc/bash_completion.d/lk`.
    Completions {
        /// Target shell
        #[arg(value_enum)]
        shell: CompletionShell,
    },
}

/// ServerKit connection management (Track D, plan 21). Connections live in the
/// same SQLite table the GUI uses, so `lk connection add` and the app's
/// Settings → ServerKit panel share one list.
#[derive(Subcommand)]
enum ConnectionCmd {
    /// Add a connection. Validates it (health + API key + extension probe) the
    /// same way the app does and refuses to store a key that doesn't work.
    /// The key is read from a hidden prompt, `--key`, or LOCALKIT_API_KEY.
    Add {
        /// Connection name (label), e.g. "prod"
        name: String,
        /// ServerKit base URL, e.g. https://panel.example.com
        url: String,
        /// API key (skips the hidden prompt; required when not on a TTY)
        #[arg(long, env = "LOCALKIT_API_KEY", hide_env_values = true)]
        key: Option<String>,
        /// Output the stored connection as machine-readable JSON
        #[arg(long)]
        json: bool,
    },

    /// List stored connections (local only — no network). Use `test` to probe.
    List {
        /// Output machine-readable JSON (never includes the API key)
        #[arg(long)]
        json: bool,
    },

    /// Re-run the connection test: health, API key, and extension features.
    Test {
        /// Connection (exact id, or case-insensitive name)
        connection: String,
        /// Output the test result as machine-readable JSON
        #[arg(long)]
        json: bool,
    },

    /// Remove a connection. Prompts unless --yes; --yes required on non-TTY.
    Remove {
        /// Connection (exact id, or case-insensitive name)
        connection: String,
        /// Skip the confirmation prompt
        #[arg(long)]
        yes: bool,
    },
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

#[derive(Subcommand)]
enum BlueprintCmd {
    /// List saved blueprints
    List {
        /// Output machine-readable JSON
        #[arg(long)]
        json: bool,
    },

    /// Save an existing site as a reusable blueprint.
    /// Prints the new blueprint's id on stdout; progress goes to stderr.
    Save {
        /// Source site (exact id, or case-insensitive slug or name)
        site: String,
        /// Blueprint name
        name: String,
        /// Optional description stored in the blueprint
        #[arg(long)]
        description: Option<String>,
        /// Output the created blueprint as machine-readable JSON
        #[arg(long)]
        json: bool,
    },

    /// Delete a blueprint (id or name). Prompts unless --yes.
    Delete {
        /// Blueprint (exact id, or case-insensitive name)
        blueprint: String,
        /// Skip the confirmation prompt
        #[arg(long)]
        yes: bool,
    },

    /// Export a blueprint to a single portable `.lkbp` file for sharing
    Export {
        /// Blueprint (exact id, or case-insensitive name)
        blueprint: String,
        /// Output file (defaults to <id>.lkbp in the current directory)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Import a blueprint from a `.lkbp` file
    Import {
        /// Path to the `.lkbp` file
        file: PathBuf,
        /// Output the imported blueprint as machine-readable JSON
        #[arg(long)]
        json: bool,
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
        eprintln!("{} {}", red("error:"), e.message);
        std::process::exit(e.code);
    }
}

/// A CLI failure plus the process exit code it carries. Almost everything is
/// code 1; a sync operation the *server* rejects surfaces as code 2 so scripts
/// can tell "the server said no" apart from "something local broke".
struct CliError {
    message: String,
    code: i32,
}

impl CliError {
    fn new(message: impl Into<String>) -> Self {
        Self { message: message.into(), code: 1 }
    }
    /// Exit code 2 — the remote rejected the operation (see `sync_err`).
    fn rejected(message: impl Into<String>) -> Self {
        Self { message: message.into(), code: 2 }
    }
}

impl From<String> for CliError {
    fn from(message: String) -> Self {
        CliError::new(message)
    }
}

async fn run(cli: &Cli) -> Result<(), CliError> {
    // These two never touch the DB.
    match &cli.command {
        Cmd::Doctor => return cmd_doctor(cli.data_dir.clone()).await.map_err(CliError::from),
        Cmd::Completions { shell } => return cmd_completions(*shell).map_err(CliError::from),
        _ => {}
    }

    let state = make_state(cli)?;

    // Push/pull own their exit code (2 on a server rejection), so they `return`
    // a `CliError` directly; every other command's `String` error collapses to
    // a plain code-1 `CliError` at the end.
    let out: Result<(), String> = match &cli.command {
        Cmd::Push {
            site,
            code,
            db,
            connection,
            remote_site,
            json,
        } => {
            return cmd_push(
                &state,
                site,
                *code,
                *db,
                connection.as_deref(),
                remote_site.as_deref(),
                *json,
            )
            .await
        }
        Cmd::Pull {
            site,
            db,
            connection,
            remote_site,
            json,
        } => {
            return cmd_pull(
                &state,
                site,
                *db,
                connection.as_deref(),
                remote_site.as_deref(),
                *json,
            )
            .await
        }
        Cmd::List { json } => cmd_list(&state, *json).await,
        Cmd::Sites { remote, json } => cmd_remote_sites(&state, remote, *json).await,
        Cmd::Connection(sub) => cmd_connection(&state, sub).await,
        Cmd::Create {
            name,
            wp_version,
            php_version,
            blueprint,
            json,
        } => cmd_create(&state, name.as_deref(), wp_version, php_version, blueprint.as_deref(), *json).await,
        Cmd::Clone {
            site: q,
            new_name,
            json,
        } => cmd_clone(&state, q, new_name, *json).await,
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
        Cmd::Resume { site: q } => {
            let s = resolve(&state, q)?;
            let s = site::resume(None, &state, &s.id).await?;
            eprintln!("{} {} setup finished", ok("✓"), bold(&s.name));
            println!("{}", site_url(&s));
            Ok(())
        }
        Cmd::Delete {
            site: q,
            yes,
            delete_snapshots,
        } => cmd_delete(&state, q, *yes, *delete_snapshots).await,
        Cmd::Snapshot(sub) => cmd_snapshot(&state, sub).await,
        Cmd::Blueprint(sub) => cmd_blueprint(&state, sub).await,
        Cmd::Import {
            connection,
            site: remote,
            name,
            json,
        } => cmd_import(&state, connection, remote, name.clone(), *json).await,
        Cmd::Info { site: q, json } => cmd_info(&state, q, *json),
        Cmd::Logs { site: q, tail } => {
            let s = resolve(&state, q)?;
            let logs = site::logs(&state, &s.id, *tail).await?;
            print!("{logs}");
            Ok(())
        }
        Cmd::Wp { site: q, args } => {
            let s = resolve(&state, q)?;
            s.require(s.capabilities.wp_tools, "`lk wp`")?;
            let mut full: Vec<&str> = vec!["wp"];
            full.extend(args.iter().map(String::as_str));
            let out = docker::compose_run(&s.dir(), "wpcli", &full).await?;
            print!("{out}");
            Ok(())
        }
        Cmd::Env { site: q, shell, json } => cmd_env(&state, q, *shell, *json),
        Cmd::Login { site: q, user, open } => cmd_login(&state, q, user.as_deref(), *open).await,
        Cmd::Doctor | Cmd::Completions { .. } => unreachable!("handled before make_state"),
    };
    out.map_err(CliError::from)
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
                // A half-created site (plan 23) reads as `incomplete` — run
                // `lk resume <site>` to finish it.
                if s.incomplete { "incomplete".to_string() } else { s.live_status.clone() },
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
                // Degraded (up but unhealthy) and incomplete (a killed create)
                // both warrant attention — amber, not dim (plan 23).
                (1, "degraded") | (1, "incomplete") => warn(&padded),
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
    name: Option<&str>,
    wp_version: &Option<String>,
    php_version: &Option<String>,
    blueprint: Option<&str>,
    json: bool,
) -> Result<(), String> {
    // From a blueprint: versions come from the recipe, the name defaults to it.
    if let Some(query) = blueprint {
        let bp = blueprint::find(state, query)?;
        let site = blueprint::create_site(None, state, &bp.id, name.map(str::to_string)).await?;
        if json {
            print_json(&site)?;
        } else {
            println!("{}", site_url(&site));
        }
        eprintln!(
            "{} {} created from blueprint {} and running",
            ok("✓"),
            bold(&site.name),
            bold(&bp.manifest.name)
        );
        eprintln!(
            "{} log in with `lk login {}` — the blueprint's database keeps its accounts",
            info("→"),
            site.slug
        );
        return Ok(());
    }

    let name = name.ok_or("a site name is required (or pass --blueprint <name>)")?;
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

/// `lk clone` — thin wrapper over `site::clone_site`; all orchestration lives
/// in the library. Progress reaches the terminal on its own: with no Tauri app
/// handle `site::emit` prints each stage to stderr.
async fn cmd_clone(
    state: &AppState,
    query: &str,
    new_name: &str,
    json: bool,
) -> Result<(), String> {
    let source = resolve(state, query)?;
    let clone = site::clone_site(None, state, &source.id, new_name.to_string()).await?;
    if json {
        print_json(&clone)?;
    } else {
        // stdout carries the URL (scriptable); chrome stays on stderr.
        println!("{}", site_url(&clone));
    }
    eprintln!(
        "{} {} cloned from {} and running",
        ok("✓"),
        bold(&clone.name),
        bold(&source.name)
    );
    eprintln!(
        "{} admin login carries over from the source: {} / {}",
        info("→"),
        clone.admin_user,
        clone.admin_pass
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

async fn cmd_blueprint(state: &AppState, cmd: &BlueprintCmd) -> Result<(), String> {
    match cmd {
        BlueprintCmd::List { json } => {
            let bps = blueprint::list(state)?;
            if *json {
                return print_json(&bps);
            }
            if bps.is_empty() {
                eprintln!(
                    "{} no blueprints yet. save one with `lk blueprint save <site> <name>`.",
                    info("→")
                );
                return Ok(());
            }
            let rows: Vec<[String; 5]> = bps
                .iter()
                .map(|b| {
                    let theme = if b.manifest.theme.is_empty() {
                        "—"
                    } else {
                        b.manifest.theme.as_str()
                    };
                    [
                        b.id.clone(),
                        b.manifest.name.clone(),
                        short_time(&b.manifest.created_at),
                        format!("{} + {}", human_bytes(b.db_bytes), human_bytes(b.code_bytes)),
                        format!("{} plugins · {theme}", b.manifest.plugins.len()),
                    ]
                })
                .collect();
            print_table(&["ID", "NAME", "CREATED", "DB + CODE", "STACK"], &rows);
            Ok(())
        }

        BlueprintCmd::Save {
            site: q,
            name,
            description,
            json,
        } => {
            let s = resolve(state, q)?;
            let bp = blueprint::save(None, state, &s.id, name.clone(), description.clone()).await?;
            if *json {
                print_json(&bp)?;
            } else {
                // stdout carries the id (scriptable); chrome stays on stderr.
                println!("{}", bp.id);
            }
            eprintln!(
                "{} saved {} as the blueprint {} ({} plugins, {} theme)",
                ok("✓"),
                bold(&s.name),
                bold(&bp.manifest.name),
                bp.manifest.plugins.len(),
                if bp.manifest.theme.is_empty() { "no" } else { bp.manifest.theme.as_str() }
            );
            Ok(())
        }

        BlueprintCmd::Delete { blueprint: q, yes } => {
            let bp = blueprint::find(state, q)?;
            confirm(
                *yes,
                &format!("delete blueprint `{}`? this cannot be undone.", bp.manifest.name),
                &format!("`lk blueprint delete` removes `{}` permanently. pass --yes to confirm.", bp.id),
            )?;
            blueprint::delete(state, &bp.id)?;
            eprintln!("{} blueprint {} deleted", ok("✓"), bold(&bp.manifest.name));
            Ok(())
        }

        BlueprintCmd::Export { blueprint: q, output } => {
            let bp = blueprint::find(state, q)?;
            let dest = output
                .clone()
                .unwrap_or_else(|| PathBuf::from(format!("{}.lkbp", bp.id)));
            blueprint::export(state, &bp.id, &dest)?;
            // stdout carries the path (scriptable); chrome stays on stderr.
            println!("{}", dest.display());
            eprintln!(
                "{} exported blueprint {} to {}",
                ok("✓"),
                bold(&bp.manifest.name),
                dest.display()
            );
            Ok(())
        }

        BlueprintCmd::Import { file, json } => {
            let bp = blueprint::import(state, file)?;
            if *json {
                print_json(&bp)?;
            } else {
                println!("{}", bp.id);
            }
            eprintln!(
                "{} imported blueprint {} ({} plugins)",
                ok("✓"),
                bold(&bp.manifest.name),
                bp.manifest.plugins.len()
            );
            eprintln!(
                "{} create a site from it with `lk create --blueprint {}`",
                info("→"),
                bp.id
            );
            Ok(())
        }
    }
}

/// `lk import` — thin wrapper over `sync::import_site`; all orchestration
/// lives in the library. Progress reaches the terminal on its own: with no
/// Tauri app handle `site::emit` prints each stage to stderr.
async fn cmd_import(
    state: &AppState,
    connection: &str,
    remote: &str,
    name: Option<String>,
    json: bool,
) -> Result<(), String> {
    let conn = resolve_connection(state, connection)?;
    let remote_id = resolve_remote_site(&conn, remote).await?;

    let site = localkit_lib::sync::import_site(None, state, &conn.id, remote_id, name).await?;
    if json {
        print_json(&site)?;
    } else {
        // stdout carries the URL (scriptable); chrome stays on stderr.
        println!("{}", site_url(&site));
    }
    eprintln!(
        "{} {} imported from {} and running",
        ok("✓"),
        bold(&site.name),
        conn.label
    );
    eprintln!(
        "{} log in with `lk login {}` — the imported database keeps the remote's accounts",
        info("→"),
        site.slug
    );
    Ok(())
}

/// Exact connection id wins, then case-insensitive label — the same shape as
/// site resolution, so the two feel identical from the terminal.
fn resolve_connection(state: &AppState, query: &str) -> Result<ServerKitConnection, String> {
    let conns = load_connections(state)?;
    if conns.is_empty() {
        return Err(NO_CONNECTIONS.into());
    }
    pick_connection(&conns, query)
}

const NO_CONNECTIONS: &str =
    "no ServerKit connections yet — add one with `lk connection add <name> <url>`.";

fn load_connections(state: &AppState) -> Result<Vec<ServerKitConnection>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.list_connections()
}

/// Pure connection resolver: exact id, then unique case-insensitive label.
/// Kept separate from `resolve_connection` so it can be unit-tested without a DB
/// (it mirrors `pick` for sites).
fn pick_connection(conns: &[ServerKitConnection], query: &str) -> Result<ServerKitConnection, String> {
    if let Some(c) = conns.iter().find(|c| c.id == query) {
        return Ok(c.clone());
    }
    let q = query.to_lowercase();
    let hits: Vec<_> = conns.iter().filter(|c| c.label.to_lowercase() == q).collect();
    match hits.len() {
        1 => Ok(hits[0].clone()),
        0 => Err(format!(
            "no ServerKit connection named `{query}`. available: {}",
            conns.iter().map(|c| c.label.as_str()).collect::<Vec<_>>().join(", ")
        )),
        _ => Err(format!(
            "`{query}` matches more than one connection. pass the exact id."
        )),
    }
}

/// A remote site is addressed by its numeric server id, or by name — in which
/// case the server is listed to look it up.
async fn resolve_remote_site(conn: &ServerKitConnection, query: &str) -> Result<i64, String> {
    if let Ok(id) = query.parse::<i64>() {
        return Ok(id);
    }
    let sites = serverkit::list_wp_sites(&conn.url, &conn.api_key).await?;
    let q = query.to_lowercase();
    let hits: Vec<_> = sites.iter().filter(|s| s.name.to_lowercase() == q).collect();
    match hits.len() {
        1 => Ok(hits[0].id),
        0 => Err(format!(
            "no site named `{query}` on {}. available: {}",
            conn.label,
            sites
                .iter()
                .map(|s| format!("{} (#{})", s.name, s.id))
                .collect::<Vec<_>>()
                .join(", ")
        )),
        _ => Err(format!(
            "`{query}` matches more than one remote site. pass the numeric id."
        )),
    }
}

// ---------------------------------------------------------------------------
// ServerKit — connections, remote listing, push/pull (plan 21)
// ---------------------------------------------------------------------------

/// Redacted view of a connection for `--json` output — deliberately omits the
/// API key, which the full `ServerKitConnection` struct carries in plaintext.
#[derive(serde::Serialize)]
struct ConnectionView<'a> {
    id: &'a str,
    name: &'a str,
    url: &'a str,
    created_at: &'a str,
}

impl<'a> From<&'a ServerKitConnection> for ConnectionView<'a> {
    fn from(c: &'a ServerKitConnection) -> Self {
        Self { id: &c.id, name: &c.label, url: &c.url, created_at: &c.created_at }
    }
}

async fn cmd_connection(state: &AppState, cmd: &ConnectionCmd) -> Result<(), String> {
    match cmd {
        ConnectionCmd::Add { name, url, key, json } => cmd_connection_add(state, name, url, key.as_deref(), *json).await,
        ConnectionCmd::List { json } => cmd_connection_list(state, *json),
        ConnectionCmd::Test { connection, json } => cmd_connection_test(state, connection, *json).await,
        ConnectionCmd::Remove { connection, yes } => cmd_connection_remove(state, connection, *yes),
    }
}

/// `lk connection add` — validate before storing (health + key + extension),
/// mirroring the app's Settings → ServerKit flow, and refuse to persist a key
/// that doesn't work rather than storing a dud that fails at push time.
async fn cmd_connection_add(
    state: &AppState,
    name: &str,
    url: &str,
    key: Option<&str>,
    json: bool,
) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("a connection name is required".into());
    }
    let url = serverkit::normalize_base_url(url)?;
    let api_key = read_api_key(key)?;

    eprintln!("{} testing {url}...", info("→"));
    let ext = serverkit::test_connection(&url, &api_key).await?;

    let conn = ServerKitConnection {
        id: uuid::Uuid::new_v4().to_string(),
        label: name.to_string(),
        url,
        api_key,
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        db.insert_connection(&conn)?;
    }

    if json {
        print_json(&ConnectionView::from(&conn))?;
    } else {
        // stdout carries the new id (scriptable); chrome stays on stderr.
        println!("{}", conn.id);
    }
    eprintln!("{} connection {} saved ({})", ok("✓"), bold(&conn.label), conn.url);
    if ext.localkit_extension {
        eprintln!(
            "{} serverkit-localkit extension detected — features: {}",
            info("→"),
            if ext.features.is_empty() { "(none advertised)".to_string() } else { ext.features.join(", ") }
        );
    } else {
        eprintln!(
            "{} the serverkit-localkit extension is not installed — push/pull/import will not work until it is.",
            warn("!")
        );
    }
    Ok(())
}

/// Read an API key from `--key`/env, or a hidden TTY prompt. Refuses to hang on
/// a non-TTY with no key supplied.
fn read_api_key(flag: Option<&str>) -> Result<String, String> {
    if let Some(k) = flag {
        let k = k.trim();
        if k.is_empty() {
            return Err("the API key is empty".into());
        }
        return Ok(k.to_string());
    }
    if !std::io::stdin().is_terminal() {
        return Err(
            "no API key and no TTY to prompt on — pass --key <key> or set LOCALKIT_API_KEY.".into(),
        );
    }
    let key = rpassword::prompt_password("ServerKit API key: ")
        .map_err(|e| format!("failed to read the API key: {e}"))?;
    let key = key.trim().to_string();
    if key.is_empty() {
        return Err("no API key entered".into());
    }
    Ok(key)
}

fn cmd_connection_list(state: &AppState, json: bool) -> Result<(), String> {
    let conns = load_connections(state)?;
    if json {
        let views: Vec<ConnectionView> = conns.iter().map(ConnectionView::from).collect();
        return print_json(&views);
    }
    if conns.is_empty() {
        eprintln!(
            "{} no ServerKit connections yet. add one with `lk connection add <name> <url>`.",
            info("→")
        );
        return Ok(());
    }
    let rows: Vec<[String; 3]> = conns
        .iter()
        .map(|c| [c.label.clone(), c.url.clone(), short_time(&c.created_at)])
        .collect();
    print_table(&["NAME", "URL", "ADDED"], &rows);
    eprintln!("{} probe a server's extension with `lk connection test <name>`", info("→"));
    Ok(())
}

async fn cmd_connection_test(state: &AppState, query: &str, json: bool) -> Result<(), String> {
    let conn = resolve_connection(state, query)?;
    eprintln!("{} testing {}...", info("→"), conn.url);
    let ext = serverkit::test_connection(&conn.url, &conn.api_key).await?;
    if json {
        return print_json(&ext);
    }
    eprintln!("{} {} reachable, API key valid", ok("✓"), bold(&conn.label));
    if ext.localkit_extension {
        println!(
            "serverkit-localkit extension: installed (features: {})",
            if ext.features.is_empty() { "none advertised".to_string() } else { ext.features.join(", ") }
        );
    } else {
        println!("serverkit-localkit extension: NOT installed");
    }
    Ok(())
}

fn cmd_connection_remove(state: &AppState, query: &str, yes: bool) -> Result<(), String> {
    let conn = resolve_connection(state, query)?;
    confirm(
        yes,
        &format!("remove connection `{}` ({})?", conn.label, conn.url),
        &format!("`lk connection remove` deletes `{}`. pass --yes to confirm.", conn.label),
    )?;
    {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        db.delete_connection(&conn.id)?;
    }
    eprintln!("{} connection {} removed", ok("✓"), bold(&conn.label));
    Ok(())
}

/// `lk sites --remote <connection>` — read-only remote site listing.
async fn cmd_remote_sites(state: &AppState, remote: &str, json: bool) -> Result<(), String> {
    let conn = resolve_connection(state, remote)?;
    let sites = serverkit::list_wp_sites(&conn.url, &conn.api_key).await?;
    if json {
        return print_json(&sites);
    }
    if sites.is_empty() {
        eprintln!("{} no WordPress sites on {}.", info("→"), conn.label);
        return Ok(());
    }
    let rows: Vec<[String; 5]> = sites
        .iter()
        .map(|s| {
            [
                s.id.to_string(),
                s.name.clone(),
                s.status.clone(),
                s.url.clone().unwrap_or_else(|| "—".into()),
                if s.multisite {
                    "multisite".into()
                } else {
                    format!(
                        "WP {} / PHP {}",
                        s.wp_version.as_deref().unwrap_or("?"),
                        s.php_version.as_deref().unwrap_or("?")
                    )
                },
            ]
        })
        .collect();
    print_table(&["ID", "NAME", "STATUS", "URL", "STACK"], &rows);
    Ok(())
}

/// Decide which connection a push/pull targets: an explicit `--connection`
/// wins; otherwise the site's linked remote (plan 18 columns); otherwise the
/// sole connection if there is exactly one.
fn resolve_sync_connection(
    conns: Vec<ServerKitConnection>,
    site: &site::Site,
    flag: Option<&str>,
) -> Result<ServerKitConnection, String> {
    if let Some(q) = flag {
        if conns.is_empty() {
            return Err(NO_CONNECTIONS.into());
        }
        return pick_connection(&conns, q);
    }
    // A site imported from a remote carries its origin connection.
    if let Some(cid) = &site.connection_id {
        if let Some(c) = conns.iter().find(|c| &c.id == cid) {
            return Ok(c.clone());
        }
        // The linked connection was removed — fall through to the auto rules.
    }
    match conns.len() {
        0 => Err(NO_CONNECTIONS.into()),
        1 => Ok(conns.into_iter().next().unwrap()),
        _ => Err(format!(
            "`{}` has no linked remote and there is more than one connection — pass --connection <name>. available: {}",
            site.slug,
            conns.iter().map(|c| c.label.as_str()).collect::<Vec<_>>().join(", ")
        )),
    }
}

/// Decide which remote site id a push/pull targets: `--remote-site` wins;
/// otherwise the site's linked remote id, but only when the resolved connection
/// is the one it was linked to (a remote id is meaningless on another server).
async fn resolve_sync_remote_id(
    conn: &ServerKitConnection,
    site: &site::Site,
    flag: Option<&str>,
) -> Result<i64, String> {
    if let Some(q) = flag {
        return resolve_remote_site(conn, q).await;
    }
    if site.connection_id.as_deref() == Some(conn.id.as_str()) {
        if let Some(id) = site.remote_site_id {
            return Ok(id);
        }
    }
    Err(format!(
        "`{}` has no linked remote site on {} — pass --remote-site <id|name> (see `lk sites --remote {}`).",
        site.slug, conn.label, conn.label
    ))
}

/// The remote site's public URL, best-effort, so pull can search-replace remote
/// -> local. A listing failure just means the rewrite is skipped, not that the
/// pull fails.
async fn remote_site_url(conn: &ServerKitConnection, remote_id: i64) -> Option<String> {
    serverkit::list_wp_sites(&conn.url, &conn.api_key)
        .await
        .ok()?
        .into_iter()
        .find(|s| s.id == remote_id)
        .and_then(|s| s.url)
}

/// Classify a sync failure into an exit code: 2 when the failure clearly
/// originated on the server (rejected key, missing/old extension, size limit,
/// an HTTP status), 1 for local failures (site not found, snapshot, Docker).
///
/// A heuristic over the library's error strings — the sync API returns a bare
/// `String`. Worst case a server error is reported as 1 rather than 2; it never
/// mislabels a local failure as a remote rejection in a way that matters.
fn remote_rejected(msg: &str) -> bool {
    const MARKERS: [&str; 6] = [
        "API key was rejected",
        "extension is not installed",
        "too old to import",
        "too large for the server",
        "failed with HTTP",
        "ServerKit limit",
    ];
    MARKERS.iter().any(|m| msg.contains(m))
}

fn sync_err(e: String) -> CliError {
    if remote_rejected(&e) {
        CliError::rejected(e)
    } else {
        CliError::new(e)
    }
}

/// The freshly written sync-history row for an operation, so `--json` can print
/// the resulting `SyncRecord` (the library's sync fns return `()`).
fn latest_record(state: &AppState, site_id: &str, direction: &str, kind: &str) -> Result<SyncRecord, String> {
    sync::history(state, site_id)?
        .into_iter()
        .find(|r| r.direction == direction && r.kind == kind)
        .ok_or_else(|| "the sync succeeded but no history record was found".into())
}

async fn cmd_push(
    state: &AppState,
    query: &str,
    code: bool,
    db: bool,
    connection: Option<&str>,
    remote_site: Option<&str>,
    json: bool,
) -> Result<(), CliError> {
    if !code && !db {
        return Err(CliError::new("nothing to push — pass --code and/or --db"));
    }
    let site = resolve(state, query)?;
    let conns = load_connections(state)?;
    let conn = resolve_sync_connection(conns, &site, connection)?;
    let remote_id = resolve_sync_remote_id(&conn, &site, remote_site).await?;

    let mut records: Vec<SyncRecord> = Vec::new();
    if code {
        sync::push_code(None, state, &conn.id, &site.id, remote_id).await.map_err(sync_err)?;
        records.push(latest_record(state, &site.id, "push", "code")?);
    }
    if db {
        sync::push_db(None, state, &conn.id, &site.id, remote_id).await.map_err(sync_err)?;
        records.push(latest_record(state, &site.id, "push", "db")?);
    }

    if json {
        // One record → the object; both → the array, so the shape is predictable.
        match records.as_slice() {
            [only] => print_json(only)?,
            many => print_json(&many)?,
        }
    }
    eprintln!(
        "{} pushed {} to remote site #{remote_id} on {}",
        ok("✓"),
        pushed_kinds(code, db),
        conn.label
    );
    Ok(())
}

fn pushed_kinds(code: bool, db: bool) -> &'static str {
    match (code, db) {
        (true, true) => "code + database",
        (true, false) => "code",
        _ => "database",
    }
}

async fn cmd_pull(
    state: &AppState,
    query: &str,
    db: bool,
    connection: Option<&str>,
    remote_site: Option<&str>,
    json: bool,
) -> Result<(), CliError> {
    if !db {
        return Err(CliError::new(
            "pass --db — pulling a remote site's code creates a NEW local site, which is `lk import`.",
        ));
    }
    let site = resolve(state, query)?;
    let conns = load_connections(state)?;
    let conn = resolve_sync_connection(conns, &site, connection)?;
    let remote_id = resolve_sync_remote_id(&conn, &site, remote_site).await?;
    let remote_url = remote_site_url(&conn, remote_id).await;

    sync::pull_db(None, state, &conn.id, &site.id, remote_id, remote_url)
        .await
        .map_err(sync_err)?;
    let record = latest_record(state, &site.id, "pull", "db")?;
    if json {
        print_json(&record)?;
    }
    eprintln!(
        "{} pulled the database from remote site #{remote_id} on {} into {}",
        ok("✓"),
        conn.label,
        bold(&site.name)
    );
    eprintln!("{} a pre-pull snapshot was taken — `lk snapshot list {}` to restore", info("→"), site.slug);
    Ok(())
}

/// `lk completions <shell>` — static completion script via clap_complete.
fn cmd_completions(shell: CompletionShell) -> Result<(), String> {
    let mut cmd = Cli::command();
    clap_complete::generate(shell, &mut cmd, "lk", &mut std::io::stdout());
    Ok(())
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
    s.require(s.capabilities.one_click_login, "`lk login`")?;
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

    // Connection reachability is diagnostic only — a remote being down is not a
    // local misconfiguration, so it prints pass/fail but never flips the exit
    // code that scripts gate their local setup on.
    doctor_connections(&data_dir).await;

    // Same rule for the update check: an available update (or a GitHub outage)
    // is informational, never a reason for `doctor` to exit non-zero.
    doctor_update().await;

    if !ok {
        return Err("one or more checks failed".into());
    }
    Ok(())
}

/// Update section of `doctor` (plan 25): report whether a newer LocalKit
/// release exists. Never downloads and never flips the exit code — a GitHub
/// outage is not a local misconfiguration.
async fn doctor_update() {
    match localkit_lib::update::check().await {
        Ok(u) if u.update_available => {
            check_line(true, &format!("update available: v{} (you have v{})", u.latest, u.current));
            eprintln!("  {} download it from {}", info("→"), u.url);
        }
        Ok(u) => check_line(true, &format!("up to date (v{})", u.current)),
        Err(e) => {
            check_line(true, "update check skipped");
            eprintln!("  {e}");
        }
    }
}

/// ServerKit section of `doctor` (plan 21): for each stored connection, run the
/// same health + key + `/pair` probe the app does, so "is it me or the server"
/// has a one-command answer. Best-effort and non-fatal — a missing DB or a
/// down server does not fail `doctor`.
async fn doctor_connections(data_dir: &Path) {
    let Ok(db) = Db::open(&data_dir.join("localkit.db")) else {
        return;
    };
    let conns = match db.list_connections() {
        Ok(c) => c,
        Err(_) => return,
    };
    // Drop the DB handle before the awaits below — nothing else needs it, and
    // holding it across network calls buys nothing.
    drop(db);

    if conns.is_empty() {
        check_line(true, "no ServerKit connections configured");
        return;
    }
    for conn in &conns {
        match serverkit::test_connection(&conn.url, &conn.api_key).await {
            Ok(ext) => {
                let extension = if ext.localkit_extension {
                    if ext.features.is_empty() {
                        "extension present".to_string()
                    } else {
                        format!("extension: {}", ext.features.join(", "))
                    }
                } else {
                    "extension NOT installed".to_string()
                };
                check_line(true, &format!("connection {} → {} ({extension})", conn.label, conn.url));
            }
            Err(e) => {
                check_line(false, &format!("connection {} → {}", conn.label, conn.url));
                eprintln!("  {e}");
            }
        }
    }
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
        transfers: Default::default(),
        in_flight: Default::default(),
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
        transfers: Default::default(),
        in_flight: Default::default(),
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
        let mut s = site::Site {
            id: id.into(),
            name: name.into(),
            slug: slug.into(),
            path: format!("/tmp/{slug}"),
            port: 8081,
            wp_version: "6.7".into(),
            php_version: "8.3".into(),
            status: "running".into(),
            status_updated_at: "2026-01-01T00:00:00Z".into(),
            admin_user: "admin".into(),
            admin_pass: "secret".into(),
            created_at: "2026-01-01T00:00:00Z".into(),
            connection_id: None,
            remote_site_id: None,
            kind: site::KIND_WORDPRESS.into(),
            config: site::SiteConfig::default(),
            capabilities: site::Capabilities::default(),
        };
        s.refresh_capabilities();
        s
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

    // -- ServerKit CLI (plan 21) -------------------------------------------

    fn conn(id: &str, label: &str) -> ServerKitConnection {
        ServerKitConnection {
            id: id.into(),
            label: label.into(),
            url: "https://x.example.com".into(),
            api_key: "k".into(),
            created_at: "2026-01-01T00:00:00Z".into(),
        }
    }

    fn linked_site(conn_id: &str, remote_id: i64) -> site::Site {
        let mut s = site("id-x", "linked", "Linked");
        s.connection_id = Some(conn_id.into());
        s.remote_site_id = Some(remote_id);
        s
    }

    #[test]
    fn connection_pick_exact_id_then_label() {
        let conns = vec![conn("c1", "prod"), conn("c2", "staging")];
        assert_eq!(pick_connection(&conns, "c2").unwrap().label, "staging");
        assert_eq!(pick_connection(&conns, "PROD").unwrap().id, "c1");
    }

    #[test]
    fn connection_pick_no_match_lists_available() {
        let conns = vec![conn("c1", "prod")];
        let err = pick_connection(&conns, "nope").unwrap_err();
        assert!(err.contains("prod"), "unexpected: {err}");
    }

    #[test]
    fn connection_pick_ambiguous_label_asks_for_id() {
        let conns = vec![conn("c1", "dup"), conn("c2", "DUP")];
        let err = pick_connection(&conns, "dup").unwrap_err();
        assert!(err.contains("more than one"), "unexpected: {err}");
    }

    #[test]
    fn sync_connection_flag_wins_over_link() {
        let conns = vec![conn("c1", "prod"), conn("c2", "staging")];
        let chosen = resolve_sync_connection(conns, &linked_site("c1", 5), Some("staging")).unwrap();
        assert_eq!(chosen.id, "c2");
    }

    #[test]
    fn sync_connection_defaults_to_link() {
        let conns = vec![conn("c1", "prod"), conn("c2", "staging")];
        let chosen = resolve_sync_connection(conns, &linked_site("c2", 5), None).unwrap();
        assert_eq!(chosen.id, "c2");
    }

    #[test]
    fn sync_connection_single_is_auto_selected() {
        let conns = vec![conn("c1", "prod")];
        let site = site("id-x", "unlinked", "Unlinked");
        assert_eq!(resolve_sync_connection(conns, &site, None).unwrap().id, "c1");
    }

    #[test]
    fn sync_connection_ambiguous_without_link_needs_flag() {
        let conns = vec![conn("c1", "prod"), conn("c2", "staging")];
        let site = site("id-x", "unlinked", "Unlinked");
        let err = resolve_sync_connection(conns, &site, None).unwrap_err();
        assert!(err.contains("--connection"), "unexpected: {err}");
    }

    #[test]
    fn sync_connection_stale_link_falls_back_to_single() {
        // Linked to a connection that no longer exists → the auto rules apply.
        let conns = vec![conn("c1", "prod")];
        let chosen = resolve_sync_connection(conns, &linked_site("gone", 5), None).unwrap();
        assert_eq!(chosen.id, "c1");
    }

    #[tokio::test]
    async fn sync_remote_id_defaults_to_link() {
        let c = conn("c1", "prod");
        assert_eq!(resolve_sync_remote_id(&c, &linked_site("c1", 42), None).await.unwrap(), 42);
    }

    #[tokio::test]
    async fn sync_remote_id_unlinked_needs_flag() {
        let c = conn("c1", "prod");
        let site = site("id-x", "unlinked", "Unlinked");
        let err = resolve_sync_remote_id(&c, &site, None).await.unwrap_err();
        assert!(err.contains("--remote-site"), "unexpected: {err}");
    }

    #[tokio::test]
    async fn sync_remote_id_link_ignored_for_other_connection() {
        // The numeric remote id is meaningless on a different server.
        let other = conn("c2", "staging");
        let err = resolve_sync_remote_id(&other, &linked_site("c1", 42), None).await.unwrap_err();
        assert!(err.contains("--remote-site"), "unexpected: {err}");
    }

    #[test]
    fn remote_rejected_flags_server_errors() {
        assert!(remote_rejected("The API key was rejected (or lacks admin rights). Check the key."));
        assert!(remote_rejected("Push failed with HTTP 500."));
        assert!(remote_rejected(
            "The serverkit-localkit extension is not installed on this ServerKit server (404)."
        ));
        assert!(remote_rejected("The upload is too large for the server (ServerKit limit is 100MB)."));
    }

    #[test]
    fn remote_rejected_ignores_local_errors() {
        assert!(!remote_rejected("no site named `blog`"));
        assert!(!remote_rejected("pre-sync snapshot failed, nothing was synced: disk full"));
        assert!(!remote_rejected("Docker is not running"));
    }

    #[test]
    fn pushed_kinds_labels() {
        assert_eq!(pushed_kinds(true, true), "code + database");
        assert_eq!(pushed_kinds(true, false), "code");
        assert_eq!(pushed_kinds(false, true), "database");
    }

    #[test]
    fn connection_view_omits_the_api_key() {
        let json = serde_json::to_string(&ConnectionView::from(&conn("c1", "prod"))).unwrap();
        assert!(!json.contains("api_key"), "the api key leaked into --json output: {json}");
        assert!(!json.contains("\"k\""), "the api key value leaked: {json}");
        assert!(json.contains("\"prod\""));
    }

    #[test]
    fn completions_generate_for_every_shell() {
        for shell in [
            CompletionShell::Bash,
            CompletionShell::Zsh,
            CompletionShell::Fish,
            CompletionShell::PowerShell,
        ] {
            let mut cmd = Cli::command();
            let mut buf = Vec::new();
            clap_complete::generate(shell, &mut cmd, "lk", &mut buf);
            let out = String::from_utf8(buf).expect("completion script is valid UTF-8");
            assert!(!out.is_empty(), "{shell:?} produced no completion script");
            assert!(out.contains("connection"), "{shell:?} completion missing `connection`");
            assert!(out.contains("completions"), "{shell:?} completion missing `completions`");
        }
    }
}
