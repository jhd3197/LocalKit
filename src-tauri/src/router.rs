//! M6 local domains: one shared Caddy router on host ports 80/443 that maps
//! `http(s)://<slug>.test` to each site's `localhost:<port>`.
//!
//! Design decisions:
//! - TLD is `.test` (RFC 2606, reserved for testing): no collision with
//!   LocalWP's `.local`, and it resolves consistently across browsers and OS
//!   resolvers. Unlike `.localhost`, nothing auto-resolves it, so LocalKit
//!   manages a marked block (`# BEGIN/END LOCALKIT`) in the OS hosts file —
//!   edits require a one-time elevated approval per change (UAC / sudo).
//! - The router proxies to `host.docker.internal:<site port>`, so existing
//!   site compose projects are untouched (no shared Docker network).
//! - Caddy serves BOTH plain http and https (`tls internal`, local CA); there
//!   is no http→https redirect so users who haven't trusted the CA yet get a
//!   clean http path. "Trusted" is tracked by recording the install command's
//!   success in the settings table — we don't probe OS trust stores.

use serde::Serialize;
use std::path::{Path, PathBuf};

use crate::{docker, site::Site, wordpress, AppState};

pub const TLD: &str = "test";
const KEY_ENABLED: &str = "domains_enabled";
const KEY_CA_TRUSTED: &str = "router_ca_trusted";
const KEY_LAST_ERROR: &str = "router_last_error";
/// Default router host ports (the clean-URL mode).
pub const DEFAULT_HTTP_PORT: u16 = 80;
pub const DEFAULT_HTTPS_PORT: u16 = 443;
/// Suggested fallback pair when another program owns 80/443.
pub const FALLBACK_HTTP_PORT: u16 = 8080;
pub const FALLBACK_HTTPS_PORT: u16 = 8443;
const KEY_HTTP_PORT: &str = "router_http_port";
const KEY_HTTPS_PORT: &str = "router_https_port";
/// Path of Caddy's local-CA root cert inside the container.
const CA_CERT_CONTAINER_PATH: &str = "/data/caddy/pki/authorities/local/root.crt";
/// Managed-block markers in the OS hosts file.
const HOSTS_BEGIN: &str = "# BEGIN LOCALKIT";
const HOSTS_END: &str = "# END LOCALKIT";

#[derive(Debug, Clone, Serialize)]
pub struct RouterStatus {
    pub enabled: bool,
    pub running: bool,
    pub ca_trusted: bool,
    pub error: Option<String>,
    /// Router ports another program is holding (empty = free, or the router
    /// itself is up and holding them legitimately).
    pub conflicts: Vec<PortConflict>,
    pub http_port: u16,
    pub https_port: u16,
}

/// A router port held by some other program, with a best-effort owner name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PortConflict {
    pub port: u16,
    pub process: Option<String>,
}

/// Host ports the Caddy router publishes on. Container ports are always
/// 80/443 — only the host side moves, so the Caddyfile is port-blind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct RouterPorts {
    pub http: u16,
    pub https: u16,
}

impl Default for RouterPorts {
    fn default() -> Self {
        Self { http: DEFAULT_HTTP_PORT, https: DEFAULT_HTTPS_PORT }
    }
}

impl RouterPorts {
    /// Clean-URL mode: browsers imply 80/443, so no `:port` suffix is needed.
    pub fn is_default(&self) -> bool {
        self.http == DEFAULT_HTTP_PORT && self.https == DEFAULT_HTTPS_PORT
    }

    fn validate(&self) -> Result<(), String> {
        for port in [self.http, self.https] {
            if port == 0 {
                return Err("Router ports must be between 1 and 65535.".into());
            }
        }
        if self.http == self.https {
            return Err("The HTTP and HTTPS router ports must be different.".into());
        }
        Ok(())
    }
}

pub fn router_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("router")
}

/// The URL a site is reachable at through the router.
///
/// On the default ports this is the clean `http(s)://<slug>.test`. On fallback
/// ports the browser needs the port spelled out, and we deliberately stay on
/// http: a non-standard https port would prompt for a second certificate
/// exception even after the CA is trusted.
pub fn site_url(slug: &str, ca_trusted: bool, ports: RouterPorts) -> String {
    if !ports.is_default() {
        return format!("http://{slug}.{TLD}:{}", ports.http);
    }
    let scheme = if ca_trusted { "https" } else { "http" };
    format!("{scheme}://{slug}.{TLD}")
}

/// The URL a site should be opened at (mirrors the frontend's `siteUrl`):
/// its `*.test` domain when local domains are enabled, else `localhost:<port>`.
/// The single source of truth for "where does this site live" — tray menu,
/// one-click login, WP install URL and the CLI all funnel through here.
pub fn site_public_url(state: &AppState, site: &Site) -> String {
    let (domains_on, ca_trusted) = enabled_and_trusted(state);
    if domains_on {
        site_url(&site.slug, ca_trusted, router_ports(state))
    } else {
        format!("http://localhost:{}", site.port)
    }
}

fn render_compose(ports: RouterPorts) -> String {
    // Container ports stay 80/443 — only the host mapping moves, so the
    // Caddyfile (and the hosts block) are unaffected by fallback mode.
    format!(
        r#"name: localkit-router

services:
  caddy:
    image: caddy:2
    restart: unless-stopped
    ports:
      - "{http}:80"
      - "{https}:443"
    # Route to sites via their published host ports — no shared network,
    # no changes to per-site compose projects.
    extra_hosts:
      - "host.docker.internal:host-gateway"
    volumes:
      - ./Caddyfile:/etc/caddy/Caddyfile:ro
      - caddy-data:/data

volumes:
  caddy-data:
"#,
        http = ports.http,
        https = ports.https,
    )
}

fn render_caddyfile(sites: &[Site]) -> String {
    let mut out = String::from("# Generated by LocalKit — do not edit by hand.\n\n");
    for site in sites {
        out.push_str(&format!(
            "http://{slug}.{TLD} {{\n\treverse_proxy host.docker.internal:{port}\n}}\n\n\
             https://{slug}.{TLD} {{\n\ttls internal\n\treverse_proxy host.docker.internal:{port}\n}}\n\n",
            slug = site.slug,
            port = site.port,
        ));
    }
    out
}

/// Write the router compose project + Caddyfile for the given sites.
fn write_files(data_dir: &Path, sites: &[Site], ports: RouterPorts) -> Result<PathBuf, String> {
    let dir = router_dir(data_dir);
    std::fs::create_dir_all(&dir).map_err(|e| format!("failed to create router directory: {e}"))?;
    std::fs::write(dir.join("docker-compose.yml"), render_compose(ports))
        .map_err(|e| format!("failed to write router docker-compose.yml: {e}"))?;
    std::fs::write(dir.join("Caddyfile"), render_caddyfile(sites))
        .map_err(|e| format!("failed to write Caddyfile: {e}"))?;
    Ok(dir)
}

/// Reload Caddy's config in the running container; fall back to a restart.
async fn reload(dir: &Path) -> Result<(), String> {
    if docker::compose_exec(dir, "caddy", &["caddy", "reload", "--config", "/etc/caddy/Caddyfile"])
        .await
        .is_ok()
    {
        return Ok(());
    }
    docker::compose_restart(dir).await
}

/// Add a "what's probably holding the port" hint to bind failures. Used only
/// as a backstop — the pre-flight probe (`probe_ports`) catches the common
/// case *before* we touch the hosts file or Docker.
fn port_conflict_hint(err: &str) -> String {
    let lower = err.to_lowercase();
    if lower.contains("port is already allocated")
        || lower.contains("address already in use")
        || lower.contains("bind")
        || lower.contains("permission denied") && lower.contains("80")
    {
        format!(
            "Could not start the local-domains router: its ports appear to be in use \
             by another program (LocalWP's router, IIS, Skype, or another web server). \
             Stop it, or switch LocalKit to fallback ports in Settings → Domains.\
             \n\nDetails: {err}"
        )
    } else {
        format!("Could not start the local-domains router: {err}")
    }
}

// ---------------------------------------------------------------------------
// Port pre-flight (plan 16)
//
// LocalWP's nginx router is the canonical conflict: it binds 80/443
// machine-wide and answers *every* unknown local host with its own "Site Not
// Found" page, so without this probe a LocalKit site at `http://x.test/`
// silently hits Local's router while LocalKit's Caddy is down. Probing with a
// plain `TcpListener::bind` costs nothing and runs before any hosts-file or
// Docker mutation, so it can never race our own containers.
// ---------------------------------------------------------------------------

/// Can we bind `port`? Checks both the wildcard and the loopback address: on
/// Windows a program bound only to `127.0.0.1:80` still wins loopback traffic
/// even though `0.0.0.0:80` binds fine, which is exactly the case that makes
/// a site silently answer from the other app's router.
///
/// NOT sufficient on its own — see `probe_port`.
fn bind_free(port: u16) -> bool {
    use std::net::{Ipv4Addr, TcpListener};
    TcpListener::bind((Ipv4Addr::UNSPECIFIED, port)).is_ok()
        && TcpListener::bind((Ipv4Addr::LOCALHOST, port)).is_ok()
}

/// Is `port` held by something else? Combines two independent signals,
/// because neither is reliable alone:
///
/// - the OS listener table (`Get-NetTCPConnection` / `lsof`) — authoritative,
///   and it names the owner;
/// - a probe bind — catches listeners the query misses or can't see.
///
/// Bind-probing alone is a false-negative trap on Windows: a socket bound
/// with SO_REUSEADDR (Docker's port publisher does exactly this) lets us bind
/// the *same* address again, so a genuinely busy port reports free. Verified
/// on a machine where a container published 8080: the wildcard bind succeeded
/// while `netstat` showed it LISTENING.
async fn probe_port(port: u16) -> Option<PortConflict> {
    let process = identify_port_owner(port).await;
    if process.is_some() || !bind_free(port) {
        Some(PortConflict { port, process })
    } else {
        None
    }
}

/// Best-effort process name holding `port` (`None` when we can't tell — the
/// message then falls back to the generic "another web server" hint).
async fn identify_port_owner(port: u16) -> Option<String> {
    #[cfg(target_os = "windows")]
    let output = {
        let ps = format!(
            "$c = Get-NetTCPConnection -LocalPort {port} -State Listen -ErrorAction SilentlyContinue \
             | Select-Object -First 1; \
             if ($c) {{ (Get-Process -Id $c.OwningProcess -ErrorAction SilentlyContinue).ProcessName }}"
        );
        docker::no_window(
            tokio::process::Command::new("powershell").args(["-NoProfile", "-Command", &ps]),
        )
        .output()
        .await
    };
    #[cfg(not(target_os = "windows"))]
    let output = docker::no_window(tokio::process::Command::new("lsof").args([
        "-nP",
        &format!("-iTCP:{port}"),
        "-sTCP:LISTEN",
        "-F",
        "c",
    ]))
    .output()
    .await;

    let out = output.ok()?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    let name = stdout
        .lines()
        // `lsof -F c` prefixes the command name with 'c'; PowerShell prints it bare.
        .filter_map(|l| Some(l.trim()).filter(|l| !l.is_empty()))
        .map(|l| l.strip_prefix('c').unwrap_or(l))
        .next()?
        .to_string();
    Some(name).filter(|n| !n.is_empty())
}

/// Every TCP port in LISTEN state on the host, from one OS query.
///
/// `probe_port` answers "who holds *this* port" and costs a subprocess per
/// call; port *allocation* needs the whole set at once (site.rs walks upward
/// from 8081), so it gets this instead — one spawn, no owner lookup.
///
/// Same authority as `probe_port` and for the same reason: a bare bind test
/// misses ports published by Docker (SO_REUSEADDR lets the wildcard address be
/// re-bound), which would hand out a port that then fails at `compose up`.
/// Best effort — an empty set on error just falls back to bind-only checks.
pub async fn listening_ports() -> std::collections::HashSet<u16> {
    #[cfg(target_os = "windows")]
    let output = docker::no_window(tokio::process::Command::new("powershell").args([
        "-NoProfile",
        "-Command",
        "Get-NetTCPConnection -State Listen -ErrorAction SilentlyContinue \
         | Select-Object -ExpandProperty LocalPort",
    ]))
    .output()
    .await;
    #[cfg(not(target_os = "windows"))]
    let output = docker::no_window(tokio::process::Command::new("lsof").args([
        "-nP",
        "-iTCP",
        "-sTCP:LISTEN",
        "-F",
        "n",
    ]))
    .output()
    .await;

    match output {
        Ok(out) => parse_listening_ports(&String::from_utf8_lossy(&out.stdout)),
        Err(_) => std::collections::HashSet::new(),
    }
}

/// Parse the listener query output. Windows prints one bare port per line;
/// `lsof -F n` prints `n<addr>:<port>` (addr may be `*`, IPv4, or a bracketed
/// IPv6 literal), interleaved with other `-F` field lines we ignore.
fn parse_listening_ports(stdout: &str) -> std::collections::HashSet<u16> {
    stdout
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() {
                return None;
            }
            let candidate = match line.strip_prefix('n') {
                // lsof: take everything after the last ':' (IPv6 has several).
                Some(addr) => addr.rsplit(':').next()?,
                None => line,
            };
            candidate.parse::<u16>().ok()
        })
        .collect()
}

/// Which of the router's ports are held by something else.
pub async fn probe_ports(http: u16, https: u16) -> Vec<PortConflict> {
    let mut conflicts = Vec::new();
    for port in [http, https] {
        if let Some(conflict) = probe_port(port).await {
            conflicts.push(conflict);
        }
    }
    conflicts
}

/// User-facing explanation of a port conflict. Pure (unit-tested); the
/// remediation half depends on whether the user is already on fallback ports.
fn conflict_message(conflicts: &[PortConflict], on_default_ports: bool) -> String {
    let held = conflicts
        .iter()
        .map(|c| match &c.process {
            Some(p) => format!("port {} is held by {p}", c.port),
            None => format!("port {} is in use", c.port),
        })
        .collect::<Vec<_>>()
        .join(", ");
    let fix = if on_default_ports {
        "Quit the other program (LocalWP's router, IIS, Skype, or another web server), \
         or switch LocalKit to fallback ports (8080/8443) in Settings → Domains."
    } else {
        "Quit whatever is holding those ports, or pick different router ports in \
         Settings → Domains."
    };
    format!("Local domains could not start: {held}. {fix}")
}

// ---------------------------------------------------------------------------
// Hosts file management (`.test` does not auto-resolve — browsers AND the OS
// resolver need `127.0.0.1 <slug>.test` entries). Edits are made inside a
// marked block and require elevation (UAC / administrator password).
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
fn hosts_file_path() -> PathBuf {
    let root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
    PathBuf::from(root).join(r"System32\drivers\etc\hosts")
}

#[cfg(not(target_os = "windows"))]
fn hosts_file_path() -> PathBuf {
    PathBuf::from("/etc/hosts")
}

/// Insert or replace the LocalKit managed block in hosts-file `content`.
/// An empty `slugs` list removes the block. Pure (and unit-tested) — all
/// elevation/IO lives in `sync_hosts`.
fn update_hosts_content(content: &str, slugs: &[String]) -> String {
    let eol = if content.contains("\r\n") { "\r\n" } else { "\n" };
    let mut lines: Vec<String> = content
        .split('\n')
        .map(|l| l.trim_end_matches('\r').to_string())
        .collect();
    while matches!(lines.last(), Some(l) if l.is_empty()) {
        lines.pop();
    }

    let begin = lines.iter().position(|l| l.trim() == HOSTS_BEGIN);
    let end = lines.iter().position(|l| l.trim() == HOSTS_END);
    let had_block = matches!((begin, end), (Some(b), Some(e)) if b <= e);
    if let (Some(b), Some(e)) = (begin, end) {
        if b <= e {
            lines.drain(b..=e);
            // Also drop a single blank separator line left right before it.
            if b > 0 && matches!(lines.get(b - 1), Some(l) if l.is_empty()) {
                lines.remove(b - 1);
            }
        }
    }

    if slugs.is_empty() {
        if !had_block {
            return content.to_string(); // nothing to do — byte-identical
        }
    } else {
        if !lines.is_empty() {
            lines.push(String::new());
        }
        lines.push(HOSTS_BEGIN.to_string());
        for slug in slugs {
            lines.push(format!("127.0.0.1  {slug}.{TLD}"));
        }
        lines.push(HOSTS_END.to_string());
    }

    let mut out = lines.join(eol);
    if !out.is_empty() {
        out.push_str(eol);
    }
    out
}

/// Map an elevated-helper failure to a user-displayable message.
fn elevation_error(detail: &str) -> String {
    let lower = detail.to_lowercase();
    if lower.contains("canceled") || lower.contains("cancelled") || lower.contains("(-128)") {
        "Administrator approval is needed to manage hosts entries, and the approval \
         prompt was declined. Local domains were not enabled."
            .into()
    } else {
        format!(
            "Could not update the hosts file ({}): {}",
            hosts_file_path().display(),
            detail.trim().chars().take(300).collect::<String>()
        )
    }
}

/// Write `content` over the OS hosts file via an elevated one-shot helper:
/// Windows: UAC (`Start-Process -Verb RunAs`), macOS: `osascript … with
/// administrator privileges`, Linux: `pkexec` (with a sudo fallback message).
async fn write_hosts_elevated(content: &str) -> Result<(), String> {
    let id = uuid::Uuid::new_v4().simple().to_string();
    let tmp = std::env::temp_dir().join(format!("localkit-hosts-{id}.txt"));
    std::fs::write(&tmp, content).map_err(|e| format!("failed to stage hosts file: {e}"))?;
    let hosts = hosts_file_path();

    #[cfg(target_os = "windows")]
    let result = {
        // A tiny .bat keeps PowerShell quoting sane; RunAs triggers UAC.
        let bat = std::env::temp_dir().join(format!("localkit-hosts-{id}.bat"));
        let script = format!(
            "@echo off\r\ncopy /y \"{}\" \"{}\" >nul\r\n",
            tmp.display(),
            hosts.display()
        );
        std::fs::write(&bat, script).map_err(|e| format!("failed to stage hosts helper: {e}"))?;
        let ps = format!(
            "$ErrorActionPreference='Stop'; try {{ \
             $p = Start-Process -FilePath cmd.exe -ArgumentList '/c','\"{}\"' \
             -Verb RunAs -Wait -PassThru -WindowStyle Hidden; exit $p.ExitCode \
             }} catch {{ Write-Error $_; exit 1 }}",
            bat.display()
        );
        let out = docker::no_window(
            tokio::process::Command::new("powershell").args(["-NoProfile", "-Command", &ps]),
        )
        .output()
        .await;
        let _ = std::fs::remove_file(&bat);
        out
    };
    #[cfg(target_os = "macos")]
    let result = {
        let script = format!(
            "do shell script \"cp '{}' '{}'\" with administrator privileges",
            tmp.display(),
            hosts.display()
        );
        tokio::process::Command::new("osascript")
            .args(["-e", &script])
            .output()
            .await
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let result = {
        match tokio::process::Command::new("pkexec")
            .arg("cp")
            .arg(&tmp)
            .arg(&hosts)
            .output()
            .await
        {
            Err(_) => {
                // No pkexec: leave the staged file and tell the user exactly
                // what to run.
                let _ = std::fs::remove_file(&tmp);
                let staged = std::env::temp_dir().join(format!("localkit-hosts-{id}.txt"));
                std::fs::write(&staged, content).ok();
                return Err(format!(
                    "Could not update {} automatically (pkexec not found). \
                     Run this command yourself, then toggle local domains again:\n\n  sudo cp '{}' {}",
                    hosts.display(),
                    staged.display(),
                    hosts.display()
                ));
            }
            ok => ok,
        }
    };

    let _ = std::fs::remove_file(&tmp);
    match result {
        Ok(out) if out.status.success() => Ok(()),
        Ok(out) => Err(elevation_error(&format!(
            "{}{}",
            String::from_utf8_lossy(&out.stderr),
            String::from_utf8_lossy(&out.stdout)
        ))),
        Err(e) => Err(elevation_error(&e.to_string())),
    }
}

/// Reconcile the managed hosts block with `slugs` (empty = remove block).
/// Reads the file unprivileged; only the write is elevated. Verifies the
/// write afterwards so a silently-failed elevation is still reported.
async fn sync_hosts(slugs: &[String]) -> Result<(), String> {
    let path = hosts_file_path();
    let current = std::fs::read_to_string(&path).unwrap_or_default();
    let desired = update_hosts_content(&current, slugs);
    if desired == current {
        return Ok(());
    }
    write_hosts_elevated(&desired).await?;
    let after = std::fs::read_to_string(&path).unwrap_or_default();
    if after == desired {
        Ok(())
    } else {
        Err(elevation_error("hosts file unchanged after elevated write"))
    }
}

fn site_slugs(state: &AppState) -> Vec<String> {
    list_sites(state)
        .unwrap_or_default()
        .iter()
        .map(|s| s.slug.clone())
        .collect()
}

fn get_flag(state: &AppState, key: &str) -> Result<bool, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    Ok(db.get_setting(key)?.as_deref() == Some("true"))
}

fn set_flag(state: &AppState, key: &str, on: bool) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.set_setting(key, if on { "true" } else { "false" })
}

fn set_last_error(state: &AppState, err: Option<&str>) {
    if let Ok(db) = state.db.lock() {
        let _ = db.set_setting(KEY_LAST_ERROR, err.unwrap_or(""));
    }
}

fn list_sites(state: &AppState) -> Result<Vec<Site>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    db.list_sites()
}

/// Configured router host ports (`app_settings` KV — no migration). Never
/// fails: unset or unparseable values fall back to 80/443.
pub fn router_ports(state: &AppState) -> RouterPorts {
    let Ok(db) = state.db.lock() else {
        return RouterPorts::default();
    };
    let read = |key: &str, fallback: u16| -> u16 {
        db.get_setting(key)
            .ok()
            .flatten()
            .and_then(|v| v.parse::<u16>().ok())
            .filter(|p| *p > 0)
            .unwrap_or(fallback)
    };
    RouterPorts {
        http: read(KEY_HTTP_PORT, DEFAULT_HTTP_PORT),
        https: read(KEY_HTTPS_PORT, DEFAULT_HTTPS_PORT),
    }
}

/// (domains_enabled, ca_trusted) — used by site creation to pick the
/// WordPress install URL. Never fails; defaults to (false, false).
fn enabled_and_trusted(state: &AppState) -> (bool, bool) {
    let Ok(db) = state.db.lock() else {
        return (false, false);
    };
    let enabled = db.get_setting(KEY_ENABLED).ok().flatten().as_deref() == Some("true");
    let trusted = db
        .get_setting(KEY_CA_TRUSTED)
        .ok()
        .flatten()
        .as_deref()
        == Some("true");
    (enabled, trusted)
}

pub async fn status(state: &AppState) -> Result<RouterStatus, String> {
    let (enabled, ca_trusted, last_error) = {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        (
            db.get_setting(KEY_ENABLED)?.as_deref() == Some("true"),
            db.get_setting(KEY_CA_TRUSTED)?.as_deref() == Some("true"),
            db.get_setting(KEY_LAST_ERROR)?.filter(|s| !s.is_empty()),
        )
    };
    let running = is_running(state).await;
    let ports = router_ports(state);
    // Diagnose a persistent conflict on every status read, so reopening the
    // app while LocalWP still holds 80/443 shows the same named cause instead
    // of a bare "router is not running".
    let conflicts = if enabled && !running {
        probe_ports(ports.http, ports.https).await
    } else {
        Vec::new()
    };
    Ok(RouterStatus {
        enabled,
        running,
        ca_trusted,
        error: last_error,
        conflicts,
        http_port: ports.http,
        https_port: ports.https,
    })
}

/// Is our own Caddy container up? (Distinguishes "port 80 is busy because the
/// router owns it" from a real foreign conflict.)
async fn is_running(state: &AppState) -> bool {
    let dir = router_dir(&state.data_dir);
    if !dir.join("docker-compose.yml").exists() {
        return false;
    }
    match docker::compose_ps(&dir).await {
        Ok(containers) => containers
            .iter()
            .any(|c| c.service == "caddy" && c.state == "running"),
        Err(_) => false,
    }
}

/// Best-effort rewrite of `home`/`siteurl` for every running site.
/// Returns messages for the sites that failed (never fails the caller).
async fn rewrite_site_urls(state: &AppState, to_domains: bool) -> Vec<String> {
    let ca_trusted = get_flag(state, KEY_CA_TRUSTED).unwrap_or(false);
    let ports = router_ports(state);
    let sites = list_sites(state).unwrap_or_default();
    let mut failures = Vec::new();
    for site in sites.iter().filter(|s| s.status == "running") {
        let url = if to_domains {
            site_url(&site.slug, ca_trusted, ports)
        } else {
            format!("http://localhost:{}", site.port)
        };
        if let Err(e) = wordpress::update_site_urls(&site.dir(), &url).await {
            failures.push(format!("{}: {e}", site.name));
        }
    }
    failures
}

pub async fn set_enabled(state: &AppState, enabled: bool) -> Result<RouterStatus, String> {
    if enabled {
        // Pre-flight BEFORE touching the hosts file: writing `127.0.0.1
        // <slug>.test` while another program owns port 80 would point every
        // site at that program's router (LocalWP answers unknown hosts with
        // its own 404), which looks like LocalKit is broken.
        let ports = router_ports(state);
        if !is_running(state).await {
            let conflicts = probe_ports(ports.http, ports.https).await;
            if !conflicts.is_empty() {
                let msg = conflict_message(&conflicts, ports.is_default());
                set_last_error(state, Some(&msg));
                let mut st = status(state).await?;
                st.error = Some(msg);
                st.conflicts = conflicts;
                return Ok(st);
            }
        }
        let sites = list_sites(state)?;
        let dir = write_files(&state.data_dir, &sites, ports)?;
        // Hosts entries first: if the user declines elevation, nothing else
        // changes and the flag stays off.
        if let Err(e) = sync_hosts(&site_slugs(state)).await {
            set_last_error(state, Some(&e));
            let mut st = status(state).await?;
            st.error = Some(e);
            return Ok(st);
        }
        if let Err(e) = docker::compose_up(&dir).await {
            let msg = port_conflict_hint(&e);
            // Roll back the hosts entries we just added (best-effort; this
            // may prompt for elevation again).
            let _ = sync_hosts(&[]).await;
            set_last_error(state, Some(&msg));
            let mut st = status(state).await?;
            st.error = Some(msg);
            return Ok(st);
        }
        set_flag(state, KEY_ENABLED, true)?;
        set_last_error(state, None);
        // Routes for all sites are already in the Caddyfile; make sure Caddy
        // picked the file up (compose up mounts it, but reload is harmless).
        let _ = reload(&dir).await;
        // Point running sites at their domain (best-effort).
        let failures = rewrite_site_urls(state, true).await;
        let mut st = status(state).await?;
        if !failures.is_empty() {
            st.error = Some(format!(
                "Router is running, but the WordPress URL rewrite failed for: {}",
                failures.join("; ")
            ));
        }
        Ok(st)
    } else {
        let dir = router_dir(&state.data_dir);
        if dir.join("docker-compose.yml").exists() {
            // Keep the caddy-data volume: the local CA survives re-enables.
            let _ = docker::compose_down(&dir, false).await;
        }
        set_flag(state, KEY_ENABLED, false)?;
        set_last_error(state, None);
        // Remove the managed hosts block. If elevation fails here we still
        // disable the router — stale entries just harmlessly loopback.
        let mut warning: Option<String> = None;
        if let Err(e) = sync_hosts(&[]).await {
            warning = Some(format!(
                "Local domains are off, but the hosts entries could not be removed: {e}"
            ));
        }
        // Revert running sites to localhost:<port> (best-effort).
        let failures = rewrite_site_urls(state, false).await;
        let mut st = status(state).await?;
        if !failures.is_empty() {
            warning = Some(match warning {
                Some(w) => format!(
                    "{w}\nAlso, the WordPress URL revert failed for: {}",
                    failures.join("; ")
                ),
                None => format!(
                    "Router stopped, but the WordPress URL revert failed for: {}",
                    failures.join("; ")
                ),
            });
        }
        if warning.is_some() {
            set_last_error(state, warning.as_deref());
            st.error = warning;
        }
        Ok(st)
    }
}

/// Regenerate the Caddyfile and reload Caddy after site start/stop (no hosts
/// changes — the slug set is unchanged). No-op when domains are disabled.
pub async fn refresh_routes(state: &AppState) {
    if !get_flag(state, KEY_ENABLED).unwrap_or(false) {
        return;
    }
    let Ok(sites) = list_sites(state) else { return };
    let ports = router_ports(state);
    let Ok(dir) = write_files(&state.data_dir, &sites, ports) else { return };
    let _ = reload(&dir).await;
}

/// Change the router's host ports (fallback mode). Validates, pre-flights the
/// *new* ports, regenerates compose, restarts the router on them, and rewrites
/// running sites' WordPress URLs — the same path the enable toggle uses, so
/// `home`/`siteurl` never drift from where the site is actually served.
pub async fn set_ports(state: &AppState, http: u16, https: u16) -> Result<RouterStatus, String> {
    let ports = RouterPorts { http, https };
    ports.validate()?;
    if ports == router_ports(state) {
        return status(state).await;
    }

    let enabled = get_flag(state, KEY_ENABLED).unwrap_or(false);
    let dir = router_dir(&state.data_dir);
    // Free the old ports before probing the new ones — otherwise a swap that
    // reuses one of them would see our own container as the conflict.
    if enabled && dir.join("docker-compose.yml").exists() {
        let _ = docker::compose_down(&dir, false).await;
    }
    if enabled {
        let conflicts = probe_ports(ports.http, ports.https).await;
        if !conflicts.is_empty() {
            // Leave the old ports in settings: the router is down either way,
            // but the user's previous working config is worth preserving.
            let msg = conflict_message(&conflicts, ports.is_default());
            set_last_error(state, Some(&msg));
            let mut st = status(state).await?;
            st.error = Some(msg);
            st.conflicts = conflicts;
            return Ok(st);
        }
    }

    {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        db.set_setting(KEY_HTTP_PORT, &ports.http.to_string())?;
        db.set_setting(KEY_HTTPS_PORT, &ports.https.to_string())?;
    }

    if !enabled {
        // Ports are recorded; the router starts on them at the next enable.
        return status(state).await;
    }

    let sites = list_sites(state)?;
    let dir = write_files(&state.data_dir, &sites, ports)?;
    if let Err(e) = docker::compose_up(&dir).await {
        let msg = port_conflict_hint(&e);
        set_last_error(state, Some(&msg));
        let mut st = status(state).await?;
        st.error = Some(msg);
        return Ok(st);
    }
    set_last_error(state, None);
    let _ = reload(&dir).await;
    let failures = rewrite_site_urls(state, true).await;
    let mut st = status(state).await?;
    if !failures.is_empty() {
        st.error = Some(format!(
            "Router restarted on ports {}/{}, but the WordPress URL rewrite failed for: {}",
            ports.http,
            ports.https,
            failures.join("; ")
        ));
    }
    Ok(st)
}

/// Reconcile the managed hosts block after site create/delete (the slug set
/// changed). Elevated; best-effort — failures are recorded as the router's
/// last error rather than failing the site operation.
pub async fn refresh_hosts(state: &AppState) {
    if !get_flag(state, KEY_ENABLED).unwrap_or(false) {
        return;
    }
    if let Err(e) = sync_hosts(&site_slugs(state)).await {
        set_last_error(state, Some(&e));
    }
}

/// Extract Caddy's local-CA root cert from the container and install it into
/// the current user's OS trust store. Windows/macOS need no admin; Linux is
/// best-effort via sudo.
pub async fn trust_ca(state: &AppState) -> Result<RouterStatus, String> {
    let dir = router_dir(&state.data_dir);
    let cert_path = dir.join("caddy-root.crt");
    docker::compose_cp(&dir, "caddy", CA_CERT_CONTAINER_PATH, &cert_path)
        .await
        .map_err(|e| {
            format!(
                "Could not read the router's CA certificate (is the router running? \
                 Visit a site over https:// once so Caddy creates its local CA): {e}"
            )
        })?;
    install_ca_cert(&cert_path).await?;
    set_flag(state, KEY_CA_TRUSTED, true)?;
    status(state).await
}

async fn install_ca_cert(cert: &Path) -> Result<(), String> {
    let cert_arg = cert.to_string_lossy().to_string();
    #[cfg(target_os = "windows")]
    let (program, args): (&str, Vec<String>) = (
        "certutil",
        vec!["-user".into(), "-addstore".into(), "Root".into(), cert_arg],
    );
    #[cfg(target_os = "macos")]
    let (program, args): (&str, Vec<String>) = {
        let keychain = dirs::home_dir()
            .unwrap_or_default()
            .join("Library/Keychains/login.keychain-db")
            .to_string_lossy()
            .to_string();
        (
            "security",
            vec!["add-trusted-cert".into(), "-k".into(), keychain, cert_arg],
        )
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let (program, args): (&str, Vec<String>) = (
        "sh",
        vec![
            "-c".into(),
            format!(
                "sudo cp '{}' /usr/local/share/ca-certificates/localkit-root.crt && sudo update-ca-certificates",
                cert_arg.replace('\'', "'\\''")
            ),
        ],
    );
    let output = docker::no_window(tokio::process::Command::new(program).args(&args))
        .output()
        .await
        .map_err(|e| format!("failed to run {program}: {e}"))?;
    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let detail = if stderr.trim().is_empty() {
            stdout.trim()
        } else {
            stderr.trim()
        };
        Err(format!(
            "Failed to trust the LocalKit CA ({program}): {}",
            detail.chars().take(300).collect::<String>()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slugs(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parses_the_windows_listener_table() {
        // `Select-Object -ExpandProperty LocalPort`: one bare port per line.
        let ports = parse_listening_ports("80\r\n443\r\n8081\r\n18081\r\n");
        assert_eq!(ports.len(), 4);
        assert!(ports.contains(&8081) && ports.contains(&18081));
    }

    #[test]
    fn parses_lsof_listener_output() {
        // `lsof -F n` interleaves `p<pid>` lines; IPv6 names carry extra colons.
        let ports = parse_listening_ports("p123\nn*:8081\nn127.0.0.1:18081\np456\nn[::1]:443\n");
        assert_eq!(ports.len(), 3);
        assert!(ports.contains(&8081));
        assert!(ports.contains(&18081));
        assert!(ports.contains(&443), "IPv6 literal should not eat the port");
    }

    #[test]
    fn listener_parsing_ignores_junk() {
        let ports = parse_listening_ports("\nLocalPort\n-------\n\nnot-a-port\n99999999\n8082\n");
        assert_eq!(ports, std::collections::HashSet::from([8082]));
    }

    const SAMPLE: &str = "# Copyright (c) Microsoft Corp.\r\n\
                          \r\n\
                          127.0.0.1  localhost\r\n\
                          ::1        localhost\r\n";

    #[test]
    fn insert_block_preserves_existing_content() {
        let out = update_hosts_content(SAMPLE, &slugs(&["pixel-bakery", "acme"]));
        assert!(out.starts_with("# Copyright (c) Microsoft Corp."));
        assert!(out.contains("127.0.0.1  localhost"));
        assert!(out.contains("# BEGIN LOCALKIT"));
        assert!(out.contains("127.0.0.1  pixel-bakery.test"));
        assert!(out.contains("127.0.0.1  acme.test"));
        assert!(out.contains("# END LOCALKIT"));
        assert!(out.contains("\r\n"), "CRLF line endings preserved");
    }

    #[test]
    fn insert_is_idempotent() {
        let once = update_hosts_content(SAMPLE, &slugs(&["pixel-bakery"]));
        let twice = update_hosts_content(&once, &slugs(&["pixel-bakery"]));
        assert_eq!(once, twice);
        assert_eq!(twice.matches(HOSTS_BEGIN).count(), 1);
    }

    #[test]
    fn block_is_replaced_not_duplicated() {
        let first = update_hosts_content(SAMPLE, &slugs(&["a", "b"]));
        let second = update_hosts_content(&first, &slugs(&["b", "c"]));
        assert!(!second.contains("a.test"));
        assert!(second.contains("b.test"));
        assert!(second.contains("c.test"));
        assert_eq!(second.matches(HOSTS_BEGIN).count(), 1);
    }

    #[test]
    fn empty_slug_list_removes_block() {
        let with = update_hosts_content(SAMPLE, &slugs(&["pixel-bakery"]));
        let without = update_hosts_content(&with, &[]);
        assert!(!without.contains("LOCALKIT"));
        assert!(!without.contains("pixel-bakery.test"));
        assert_eq!(without, SAMPLE, "removal restores the original file");
    }

    #[test]
    fn remove_without_block_is_byte_identical() {
        assert_eq!(update_hosts_content(SAMPLE, &[]), SAMPLE);
        assert_eq!(update_hosts_content("", &[]), "");
    }

    #[test]
    fn lf_only_file_stays_lf() {
        let lf = "127.0.0.1  localhost\n::1  localhost\n";
        let out = update_hosts_content(lf, &slugs(&["x"]));
        assert!(!out.contains("\r\n"));
        assert!(out.contains("127.0.0.1  x.test"));
    }

    // --- plan 16: port pre-flight -----------------------------------------

    #[test]
    fn bind_free_reports_true_for_an_unbound_port() {
        // Bind an ephemeral port, learn its number, release it, then probe.
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        assert!(bind_free(port));
    }

    #[test]
    fn bind_free_reports_false_while_a_port_is_held() {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        assert!(!bind_free(port), "a bound loopback port is not free");
        drop(listener);
    }

    #[tokio::test]
    async fn probe_catches_a_wildcard_listener_that_still_allows_rebinding() {
        // The Windows SO_REUSEADDR trap: a wildcard listener can be re-bound,
        // so `bind_free` alone reports the port free. The OS listener table
        // is what actually catches it, so `probe_port` must still flag it.
        let listener = std::net::TcpListener::bind(("0.0.0.0", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        assert!(
            probe_port(port).await.is_some(),
            "a port with a live wildcard listener must be reported as in use \
             (bind_free said free={})",
            bind_free(port)
        );
        drop(listener);
    }

    #[tokio::test]
    async fn probe_returns_empty_when_ports_are_free() {
        let a = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let b = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let (pa, pb) = (a.local_addr().unwrap().port(), b.local_addr().unwrap().port());
        drop(a);
        drop(b);
        assert_eq!(probe_ports(pa, pb).await, Vec::new());
    }

    #[tokio::test]
    async fn probe_reports_the_held_port_only() {
        let held = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let taken = held.local_addr().unwrap().port();
        let free_listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let free = free_listener.local_addr().unwrap().port();
        drop(free_listener);

        let conflicts = probe_ports(taken, free).await;
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].port, taken);
        drop(held);
    }

    // --- plan 16: configurable ports --------------------------------------

    #[test]
    fn site_url_is_clean_on_default_ports() {
        let d = RouterPorts::default();
        assert_eq!(site_url("acme", false, d), "http://acme.test");
        assert_eq!(site_url("acme", true, d), "https://acme.test");
    }

    #[test]
    fn site_url_carries_the_port_in_fallback_mode() {
        let fb = RouterPorts { http: FALLBACK_HTTP_PORT, https: FALLBACK_HTTPS_PORT };
        assert_eq!(site_url("acme", false, fb), "http://acme.test:8080");
        // Even with a trusted CA we stay on http: a non-standard https port
        // would prompt for a second certificate exception.
        assert_eq!(site_url("acme", true, fb), "http://acme.test:8080");
    }

    #[test]
    fn render_compose_maps_host_ports_to_container_80_443() {
        let yml = render_compose(RouterPorts { http: 8080, https: 8443 });
        assert!(yml.contains("\"8080:80\""), "{yml}");
        assert!(yml.contains("\"8443:443\""), "{yml}");
        // The default render is unchanged from the M6 template.
        let default_yml = render_compose(RouterPorts::default());
        assert!(default_yml.contains("\"80:80\""));
        assert!(default_yml.contains("\"443:443\""));
    }

    #[test]
    fn caddyfile_is_port_blind() {
        // Only the host mapping moves — the Caddyfile never mentions host ports.
        let sites: Vec<Site> = Vec::new();
        assert_eq!(render_caddyfile(&sites), render_caddyfile(&sites));
        assert!(!render_caddyfile(&sites).contains("8080"));
    }

    #[test]
    fn port_validation_rejects_zero_and_duplicates() {
        assert!(RouterPorts::default().validate().is_ok());
        assert!(RouterPorts { http: 0, https: 443 }.validate().is_err());
        assert!(RouterPorts { http: 8080, https: 8080 }.validate().is_err());
        assert!(RouterPorts { http: 8080, https: 8443 }.validate().is_ok());
    }

    #[test]
    fn is_default_only_for_80_443() {
        assert!(RouterPorts::default().is_default());
        assert!(!RouterPorts { http: 8080, https: 443 }.is_default());
        assert!(!RouterPorts { http: 80, https: 8443 }.is_default());
    }

    #[test]
    fn conflict_message_names_the_process_and_offers_fallback() {
        let msg = conflict_message(
            &[PortConflict { port: 80, process: Some("httpd.exe".into()) }],
            true,
        );
        assert!(msg.contains("port 80 is held by httpd.exe"), "{msg}");
        assert!(msg.contains("fallback ports (8080/8443)"), "{msg}");
    }

    #[test]
    fn conflict_message_falls_back_when_the_owner_is_unknown() {
        let msg = conflict_message(
            &[
                PortConflict { port: 80, process: None },
                PortConflict { port: 443, process: None },
            ],
            true,
        );
        assert!(msg.contains("port 80 is in use"), "{msg}");
        assert!(msg.contains("port 443 is in use"), "{msg}");
    }

    #[test]
    fn conflict_message_on_fallback_ports_does_not_suggest_fallback_again() {
        let msg = conflict_message(
            &[PortConflict { port: 8080, process: Some("node".into()) }],
            false,
        );
        assert!(msg.contains("port 8080 is held by node"), "{msg}");
        assert!(!msg.contains("8080/8443"), "must not loop the same advice: {msg}");
    }

    #[test]
    fn staged_content_round_trips_through_temp_file() {
        // Mirrors what sync_hosts stages for the elevated writer.
        let desired = update_hosts_content(SAMPLE, &slugs(&["one", "two"]));
        let tmp = std::env::temp_dir().join("localkit-hosts-test-roundtrip.txt");
        std::fs::write(&tmp, &desired).unwrap();
        let read_back = std::fs::read_to_string(&tmp).unwrap();
        let _ = std::fs::remove_file(&tmp);
        assert_eq!(read_back, desired);
        // Applying the same slugs again is a no-op (desired == current).
        assert_eq!(update_hosts_content(&read_back, &slugs(&["one", "two"])), read_back);
    }
}
