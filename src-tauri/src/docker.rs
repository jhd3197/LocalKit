//! Docker orchestration via the `docker compose` CLI.
//!
//! We intentionally shell out instead of using a Docker API client:
//! fewer dependencies and it matches whatever Docker Desktop the user has.

use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::process::Command;

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
