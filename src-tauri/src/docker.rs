//! Docker orchestration via the `docker compose` CLI.
//!
//! We intentionally shell out instead of using a Docker API client:
//! fewer dependencies and it matches whatever Docker Desktop the user has.

use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::process::Command;

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
    match Command::new("docker")
        .args(["info", "--format", "{{.ServerVersion}}"])
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
    Command::new("docker")
        .arg("compose")
        .args(args)
        .current_dir(dir)
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

pub async fn compose_down(dir: &Path, volumes: bool) -> Result<(), String> {
    let args: &[&str] = if volumes { &["down", "-v"] } else { &["down"] };
    compose(dir, args).await.map(|_| ())
}

/// Run a one-off command in a compose service, e.g. wp-cli:
/// `docker compose run --rm -T <service> <args...>`
pub async fn compose_run(dir: &Path, service: &str, args: &[&str]) -> Result<String, String> {
    let mut full: Vec<&str> = vec!["run", "--rm", "-T", service];
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
    use tokio::io::AsyncWriteExt;
    if !dir.exists() {
        return Err(format!("site directory not found: {}", dir.display()));
    }
    let mut full: Vec<&str> = vec!["run", "--rm", "-T", service];
    full.extend_from_slice(args);
    let mut child = Command::new("docker")
        .arg("compose")
        .args(&full)
        .current_dir(dir)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to run docker compose: {e}"))?;

    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(input).await;
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

pub async fn compose_ps(dir: &Path) -> Result<Vec<ContainerInfo>, String> {
    let stdout = compose(dir, &["ps", "--format", "json"]).await?;
    parse_ps(&stdout)
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
