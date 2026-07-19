//! Interactive per-site terminals.
//!
//! Each terminal is a real PTY (ConPTY on Windows, openpty elsewhere via the
//! `portable-pty` crate) running `docker compose exec wordpress bash` in the
//! site's directory, so the user lands in a root shell inside the site's
//! WordPress container. Output streams to the frontend over the
//! `terminal://data` event; exit is reported on `terminal://exit`. The
//! frontend (xterm.js) sends keystrokes back through `write`/`resize`.
//!
//! Modeled on Faro's `PtyManager` (SSH channels there, local PTYs here).

use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};
use serde::Serialize;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalDataEvent {
    terminal_id: String,
    data: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalExitEvent {
    terminal_id: String,
    code: Option<i32>,
}

struct PtySession {
    writer: Box<dyn Write + Send>,
    master: Box<dyn MasterPty + Send>,
    killer: Box<dyn ChildKiller + Send + Sync>,
}

#[derive(Default)]
pub struct PtyManager {
    sessions: Mutex<HashMap<String, PtySession>>,
}

impl PtyManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Open a shell inside the site's `wordpress` container. Returns the
    /// terminal id the frontend uses for write/resize/close.
    pub fn open(
        &self,
        app: &AppHandle,
        site_dir: &Path,
        cols: u32,
        rows: u32,
    ) -> Result<String, String> {
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: rows.max(2) as u16,
                cols: cols.max(2) as u16,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| format!("failed to open PTY: {e}"))?;

        let mut cmd = CommandBuilder::new("docker");
        cmd.args(["compose", "exec", "wordpress", "bash"]);
        cmd.cwd(site_dir);
        cmd.env("TERM", "xterm-256color");

        let mut child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| format!("failed to start shell: {e}"))?;
        // The slave side must be dropped once the child owns it, or the
        // reader never sees EOF when the child exits.
        drop(pair.slave);

        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| format!("failed to clone PTY reader: {e}"))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|e| format!("failed to take PTY writer: {e}"))?;
        let killer = child.clone_killer();

        let id = uuid::Uuid::new_v4().to_string();

        // Pump PTY output to the frontend.
        {
            let app = app.clone();
            let id = id.clone();
            std::thread::spawn(move || {
                let mut buf = [0u8; 8192];
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) => break,
                        Ok(n) => {
                            let _ = app.emit(
                                "terminal://data",
                                TerminalDataEvent {
                                    terminal_id: id.clone(),
                                    data: String::from_utf8_lossy(&buf[..n]).to_string(),
                                },
                            );
                        }
                        Err(_) => break,
                    }
                }
            });
        }

        // Report the shell's exit.
        {
            let app = app.clone();
            let id = id.clone();
            std::thread::spawn(move || {
                let code = child.wait().ok().map(|s| s.exit_code() as i32);
                let _ = app.emit("terminal://exit", TerminalExitEvent { terminal_id: id, code });
            });
        }

        self.sessions
            .lock()
            .map_err(|e| e.to_string())?
            .insert(id.clone(), PtySession { writer, master: pair.master, killer });
        Ok(id)
    }

    pub fn write(&self, id: &str, data: &str) -> Result<(), String> {
        let mut map = self.sessions.lock().map_err(|e| e.to_string())?;
        let session = map.get_mut(id).ok_or("terminal not found")?;
        session
            .writer
            .write_all(data.as_bytes())
            .and_then(|()| session.writer.flush())
            .map_err(|e| format!("failed to write to terminal: {e}"))
    }

    pub fn resize(&self, id: &str, cols: u32, rows: u32) -> Result<(), String> {
        let map = self.sessions.lock().map_err(|e| e.to_string())?;
        let session = map.get(id).ok_or("terminal not found")?;
        session
            .master
            .resize(PtySize {
                rows: rows.max(2) as u16,
                cols: cols.max(2) as u16,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| format!("failed to resize terminal: {e}"))
    }

    pub fn close(&self, id: &str) -> Result<(), String> {
        let session = self.sessions.lock().map_err(|e| e.to_string())?.remove(id);
        if let Some(mut session) = session {
            let _ = session.killer.kill();
            // Dropping the master/writer closes the PTY and ends the reader.
        }
        Ok(())
    }
}
