//! Docker orchestration via the `docker compose` CLI.
//!
//! We intentionally shell out instead of using a Docker API client:
//! fewer dependencies and it matches whatever Docker Desktop the user has.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use tokio::process::Command;

/// How long a `check()` result is cached (plan 23) — long enough that the
/// sidebar can poll Docker health cheaply, short enough to notice a daemon
/// going down within a tick.
const CHECK_TTL: Duration = Duration::from_secs(30);

/// Hide the console window Windows would otherwise allocate for a
/// console-subsystem child of our GUI process. No-op on other OSes.
/// Every subprocess spawn in the app must go through this.
pub(crate) fn no_window(cmd: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

#[derive(Debug, Clone, Serialize)]
pub struct DockerStatus {
    pub available: bool,
    pub version: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContainerInfo {
    pub service: String,
    pub state: String,
    pub status: String,
}

/// Check that the Docker CLI exists and the daemon is reachable.
pub async fn check() -> DockerStatus {
    match no_window(Command::new("docker").args(["info", "--format", "{{.ServerVersion}}"]))
        .output()
        .await
    {
        Ok(out) if out.status.success() => DockerStatus {
            available: true,
            version: Some(String::from_utf8_lossy(&out.stdout).trim().to_string()),
            error: None,
        },
        Ok(out) => DockerStatus {
            available: false,
            version: None,
            error: Some(friendly_error(&String::from_utf8_lossy(&out.stderr))),
        },
        Err(_) => DockerStatus {
            available: false,
            version: None,
            error: Some(
                "Docker CLI was not found. Install Docker Desktop and make sure `docker` is on your PATH."
                    .into(),
            ),
        },
    }
}

fn check_cache() -> &'static Mutex<Option<(Instant, DockerStatus)>> {
    static CACHE: OnceLock<Mutex<Option<(Instant, DockerStatus)>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

/// `check()` behind a 30 s cache (plan 23). The sidebar polls this to show a
/// global "Docker unavailable" pill without spawning a `docker info` subprocess
/// every few seconds. `force` bypasses the cache — the Settings "refresh"
/// button wants an immediate re-check. The lock is never held across the await.
pub async fn check_cached(force: bool) -> DockerStatus {
    if !force {
        if let Ok(guard) = check_cache().lock() {
            if let Some((at, status)) = guard.as_ref() {
                if at.elapsed() < CHECK_TTL {
                    return status.clone();
                }
            }
        }
    }
    let status = check().await;
    if let Ok(mut guard) = check_cache().lock() {
        *guard = Some((Instant::now(), status.clone()));
    }
    status
}

/// Turn raw CLI stderr into something the UI can show directly.
fn friendly_error(stderr: &str) -> String {
    let lower = stderr.to_lowercase();
    if lower.contains("cannot connect")
        || lower.contains("error during connect")
        || lower.contains("is the docker daemon running")
        || lower.contains("dockerdesktop")
        || lower.contains("//./pipe")
    {
        "Docker is installed but does not appear to be running. Start Docker Desktop and try again."
            .into()
    } else {
        let msg = stderr.trim();
        if msg.is_empty() {
            "Docker command failed.".into()
        } else {
            msg.chars().take(500).collect()
        }
    }
}

async fn compose_output(dir: &Path, args: &[&str]) -> Result<std::process::Output, String> {
    if !dir.exists() {
        return Err(format!("site directory not found: {}", dir.display()));
    }
    no_window(Command::new("docker").arg("compose").args(args).current_dir(dir))
        .output()
        .await
        .map_err(|e| format!("failed to run docker compose: {e}"))
}

async fn compose(dir: &Path, args: &[&str]) -> Result<String, String> {
    let output = compose_output(dir, args).await?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(friendly_error(&String::from_utf8_lossy(&output.stderr)))
    }
}

pub async fn compose_up(dir: &Path) -> Result<(), String> {
    compose(dir, &["up", "-d"]).await.map(|_| ())
}

pub async fn compose_restart(dir: &Path) -> Result<(), String> {
    compose(dir, &["restart"]).await.map(|_| ())
}

/// Pull the images for the given services. Named explicitly so profile-gated
/// services (wpcli) are included — a plain `compose pull` skips them, and the
/// first `compose run wpcli` would then pull blind for minutes.
pub async fn compose_pull(dir: &Path, services: &[&str]) -> Result<(), String> {
    let mut args: Vec<&str> = vec!["pull"];
    args.extend_from_slice(services);
    compose(dir, &args).await.map(|_| ())
}

pub async fn compose_down(dir: &Path, volumes: bool) -> Result<(), String> {
    let args: &[&str] = if volumes { &["down", "-v"] } else { &["down"] };
    compose(dir, args).await.map(|_| ())
}

/// Pull every image referenced by the compose project (no service list — used
/// for a bring-your-own-compose docker app, plan 22, where LocalKit does not
/// know the services ahead of time). Best-effort: `up` pulls anything missing
/// anyway, so this only exists to give the copy a labeled "pulling" stage.
pub async fn compose_pull_all(dir: &Path) -> Result<(), String> {
    compose(dir, &["pull"]).await.map(|_| ())
}

/// The normalized compose project as JSON (`docker compose config --format
/// json`), so LocalKit can enumerate a bring-your-own project's services,
/// images and published ports without shipping a YAML parser (plan 22). Docker
/// itself does the parsing, so every compose quirk (extends, anchors, env
/// interpolation) is already resolved.
pub async fn compose_config(dir: &Path) -> Result<String, String> {
    compose(dir, &["config", "--format", "json"]).await
}

/// Run a one-off command in a compose service, e.g. wp-cli:
/// `docker compose run --rm -T <service> <args...>`
pub async fn compose_run(dir: &Path, service: &str, args: &[&str]) -> Result<String, String> {
    let mut full: Vec<&str> = vec!["run", "--rm", "-T", service];
    full.extend_from_slice(args);
    compose(dir, &full).await
}

/// Like `compose_run`, but runs the one-off container as **root**
/// (`--user root`). Needed for commands that mutate root-owned files inside a
/// named volume: the wordpress image writes `wp-config.php` as root into the
/// `wp-data` volume, so the cli image's default `www-data` user cannot edit it —
/// `wp config set` fails with "wp-config.php is not writable" (plan 24). The
/// caller must also pass wp-cli's `--allow-root` (it refuses root otherwise).
pub async fn compose_run_root(dir: &Path, service: &str, args: &[&str]) -> Result<String, String> {
    let mut full: Vec<&str> = vec!["run", "--rm", "-T", "--user", "root", service];
    full.extend_from_slice(args);
    compose(dir, &full).await
}

/// Like `compose_run`, but pipes `input` to the command's stdin
/// (used for `wp db import -`).
pub async fn compose_run_stdin(
    dir: &Path,
    service: &str,
    args: &[&str],
    input: &[u8],
) -> Result<String, String> {
    compose_run_reader(dir, service, args, &mut &input[..]).await
}

/// Like `compose_run_stdin`, but pumps from a reader instead of a slice.
///
/// This is what keeps a pulled database off the heap (plan 19): the caller
/// hands over a `GzDecoder` on the downloaded file, and the dump streams
/// decompress -> pipe -> `wp db import` a megabyte at a time. Materializing a
/// multi-GB dump as a `Vec<u8>` just to write it to a pipe was the other half
/// of sync v1's memory problem.
///
/// The read side is blocking on purpose: it is a 1 MiB read off local disk
/// between two awaits, which is how the rest of this codebase treats file IO.
/// `Send` on the reader is not optional — it is held across an await, and every
/// future in this crate has to stay `Send` to reach a Tauri command.
pub async fn compose_run_reader(
    dir: &Path,
    service: &str,
    args: &[&str],
    input: &mut (dyn std::io::Read + Send),
) -> Result<String, String> {
    use tokio::io::AsyncWriteExt;
    if !dir.exists() {
        return Err(format!("site directory not found: {}", dir.display()));
    }
    let mut full: Vec<&str> = vec!["run", "--rm", "-T", service];
    full.extend_from_slice(args);
    let mut child = no_window(
        Command::new("docker")
            .arg("compose")
            .args(&full)
            .current_dir(dir)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped()),
    )
    .spawn()
    .map_err(|e| format!("failed to run docker compose: {e}"))?;

    if let Some(mut stdin) = child.stdin.take() {
        let mut buf = vec![0u8; 1 << 20];
        loop {
            match input.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    // A closed pipe means the command died; stop pumping and
                    // let wait_with_output report why.
                    if stdin.write_all(&buf[..n]).await.is_err() {
                        break;
                    }
                }
                Err(e) => return Err(format!("failed to read the input stream: {e}")),
            }
        }
        let _ = stdin.shutdown().await;
    }
    let output = child
        .wait_with_output()
        .await
        .map_err(|e| format!("failed to run docker compose: {e}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(friendly_error(&String::from_utf8_lossy(&output.stderr)))
    }
}

pub async fn compose_logs(dir: &Path, tail: u32) -> Result<String, String> {
    let tail_arg = format!("--tail={tail}");
    compose(dir, &["logs", "--no-color", &tail_arg]).await
}

/// Run a command inside a running compose service container:
/// `docker compose exec -T <service> <args...>`
pub async fn compose_exec(dir: &Path, service: &str, args: &[&str]) -> Result<String, String> {
    let mut full: Vec<&str> = vec!["exec", "-T", service];
    full.extend_from_slice(args);
    compose(dir, &full).await
}

/// Copy a file out of a service container:
/// `docker compose cp <service>:<src> <dest>`
pub async fn compose_cp(dir: &Path, service: &str, src: &str, dest: &Path) -> Result<(), String> {
    let from = format!("{service}:{src}");
    let dest_arg = dest.to_string_lossy().to_string();
    compose(dir, &["cp", &from, &dest_arg]).await.map(|_| ())
}

/// Copy a host file INTO a service container:
/// `docker compose cp <src> <service>:<dest>`. The copy runs through the Docker
/// daemon (root), so it can overwrite a root-owned file inside a volume that the
/// cli user cannot — which is why the config editor writes `wp-config.php` this
/// way rather than piping into the container (plan 24).
pub async fn compose_cp_into(dir: &Path, src: &Path, service: &str, dest: &str) -> Result<(), String> {
    let src_arg = src.to_string_lossy().to_string();
    let to = format!("{service}:{dest}");
    compose(dir, &["cp", &src_arg, &to]).await.map(|_| ())
}

pub async fn compose_ps(dir: &Path) -> Result<Vec<ContainerInfo>, String> {
    let stdout = compose(dir, &["ps", "--format", "json"]).await?;
    parse_ps(&stdout)
}

/// Ground-truth container states for every LocalKit compose project, from a
/// single `docker ps` pass (plan 23). One subprocess for all sites beats N
/// per-site `compose ps` calls every reconcile tick. Keyed by compose project
/// name (`com.docker.compose.project`, which is `localkit-<slug>` for every
/// LocalKit site — WordPress via the compose `name:`, docker apps via
/// `COMPOSE_PROJECT_NAME`). A project with no containers is simply absent from
/// the map. `--all` so exited containers are visible (a stopped project reads
/// as present-but-down, not gone).
pub async fn project_container_states() -> Result<HashMap<String, Vec<ContainerInfo>>, String> {
    let output = no_window(Command::new("docker").args(["ps", "--all", "--format", "json"]))
        .output()
        .await
        .map_err(|e| format!("failed to run docker ps: {e}"))?;
    if !output.status.success() {
        return Err(friendly_error(&String::from_utf8_lossy(&output.stderr)));
    }
    Ok(parse_ps_projects(&String::from_utf8_lossy(&output.stdout)))
}

/// Read one value out of a `docker ps` `Labels` string
/// (`k1=v1,k2=v2,...`). Returns the first match, `None` if the key is absent.
fn label_value(labels: &str, key: &str) -> Option<String> {
    labels.split(',').find_map(|kv| {
        let (k, v) = kv.split_once('=')?;
        (k.trim() == key).then(|| v.trim().to_string())
    })
}

#[derive(Deserialize)]
struct PsProjectEntry {
    #[serde(rename = "Labels")]
    labels: Option<String>,
    #[serde(rename = "Service")]
    service: Option<String>,
    #[serde(rename = "State")]
    state: Option<String>,
    #[serde(rename = "Status")]
    status: Option<String>,
}

/// Group `docker ps --format json` rows by their compose project. Accepts both
/// a JSON array and NDJSON (older CLIs), and skips any row without the compose
/// project/service labels (a non-LocalKit container). The service comes from
/// the compose label; some CLIs also expose a bare `Service` field, used as a
/// fallback.
fn parse_ps_projects(stdout: &str) -> HashMap<String, Vec<ContainerInfo>> {
    let trimmed = stdout.trim();
    let mut map: HashMap<String, Vec<ContainerInfo>> = HashMap::new();
    if trimmed.is_empty() {
        return map;
    }
    let entries: Vec<PsProjectEntry> = if trimmed.starts_with('[') {
        serde_json::from_str(trimmed).unwrap_or_default()
    } else {
        trimmed
            .lines()
            .filter_map(|l| serde_json::from_str::<PsProjectEntry>(l.trim()).ok())
            .collect()
    };
    for entry in entries {
        let labels = entry.labels.unwrap_or_default();
        let Some(project) = label_value(&labels, "com.docker.compose.project") else {
            continue;
        };
        let Some(service) = label_value(&labels, "com.docker.compose.service")
            .or(entry.service)
        else {
            continue;
        };
        map.entry(project).or_default().push(ContainerInfo {
            service,
            state: entry.state.unwrap_or_default(),
            status: entry.status.unwrap_or_default(),
        });
    }
    map
}

#[derive(Deserialize)]
struct PsEntry {
    #[serde(rename = "Service")]
    service: Option<String>,
    #[serde(rename = "State")]
    state: Option<String>,
    #[serde(rename = "Status")]
    status: Option<String>,
}

/// `docker compose ps --format json` emits a JSON array in Compose v5 and
/// NDJSON (one object per line) in Compose v2 — accept both.
fn parse_ps(stdout: &str) -> Result<Vec<ContainerInfo>, String> {
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return Ok(vec![]);
    }
    let to_info = |e: PsEntry| -> Option<ContainerInfo> {
        Some(ContainerInfo {
            service: e.service?,
            state: e.state.unwrap_or_default(),
            status: e.status.unwrap_or_default(),
        })
    };
    if trimmed.starts_with('[') {
        let entries: Vec<PsEntry> = serde_json::from_str(trimmed)
            .map_err(|e| format!("failed to parse compose ps output: {e}"))?;
        Ok(entries.into_iter().filter_map(to_info).collect())
    } else {
        let mut out = Vec::new();
        for line in trimmed.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Ok(entry) = serde_json::from_str::<PsEntry>(line) {
                if let Some(info) = to_info(entry) {
                    out.push(info);
                }
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn label_value_reads_one_compose_label() {
        let labels = "com.docker.compose.project=localkit-blog,com.docker.compose.service=wordpress,foo=bar";
        assert_eq!(label_value(labels, "com.docker.compose.project").as_deref(), Some("localkit-blog"));
        assert_eq!(label_value(labels, "com.docker.compose.service").as_deref(), Some("wordpress"));
        assert_eq!(label_value(labels, "missing"), None);
    }

    #[test]
    fn parse_ps_projects_groups_ndjson_by_project_and_skips_strays() {
        // Two LocalKit projects plus a non-compose container (no labels) that
        // must be ignored.
        let stdout = concat!(
            r#"{"Labels":"com.docker.compose.project=localkit-blog,com.docker.compose.service=wordpress","State":"running","Status":"Up 3 minutes"}"#, "\n",
            r#"{"Labels":"com.docker.compose.project=localkit-blog,com.docker.compose.service=db","State":"running","Status":"Up 3 minutes (healthy)"}"#, "\n",
            r#"{"Labels":"com.docker.compose.project=localkit-api,com.docker.compose.service=app","State":"exited","Status":"Exited (0) 1 minute ago"}"#, "\n",
            r#"{"Labels":"maintainer=someone","State":"running","Status":"Up"}"#, "\n",
        );
        let map = parse_ps_projects(stdout);
        assert_eq!(map.len(), 2);
        let blog = &map["localkit-blog"];
        assert_eq!(blog.len(), 2);
        assert!(blog.iter().any(|c| c.service == "wordpress" && c.state == "running"));
        let api = &map["localkit-api"];
        assert_eq!(api.len(), 1);
        assert_eq!(api[0].service, "app");
        assert_eq!(api[0].state, "exited");
    }

    #[test]
    fn parse_ps_projects_accepts_a_json_array_too() {
        let stdout = r#"[{"Labels":"com.docker.compose.project=localkit-x,com.docker.compose.service=web","State":"restarting","Status":"Restarting (1) 2 seconds ago"}]"#;
        let map = parse_ps_projects(stdout);
        assert_eq!(map["localkit-x"][0].state, "restarting");
    }

    #[test]
    fn parse_ps_projects_handles_empty_output() {
        assert!(parse_ps_projects("").is_empty());
        assert!(parse_ps_projects("   \n  ").is_empty());
    }
}
