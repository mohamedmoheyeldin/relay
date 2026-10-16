//! The in-app terminal pane for signing in to Cloud AI (D-045).
//!
//! It runs `cloud-cli auth login` in a pseudo-terminal (ConPTY on Windows) and streams
//! the bytes to the sign-in screen, which finds the sign-in link in them. The owner signs in themselves; Relay never sees the token,
//! it only checks `cloud-cli auth status` afterwards.
//!
//! portable-pty opens ConPTY with PSEUDOCONSOLE_INHERIT_CURSOR, so ConPTY first asks
//! the terminal where its cursor is (`ESC [ 6 n`) and passes no output until it gets
//! an answer. There is no terminal emulator here, so Relay answers it itself;
//! without that, `cloud-cli auth login` printed nothing and the sign-in never started.

use std::io::{Read, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use tauri::{AppHandle, Emitter};

/// Each terminal gets its own id, carried on its events, so output from a
/// terminal that was just replaced is never mistaken for the new one's.
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, serde::Serialize)]
struct Data {
    id: u64,
    data: Vec<u8>,
}

pub struct Session {
    pub id: u64,
    master: Box<dyn MasterPty + Send>,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    child: Box<dyn Child + Send + Sync>,
}

impl Session {
    pub fn write(&mut self, data: &[u8]) -> Result<()> {
        let mut writer = self.writer.lock().expect("pty writer lock");
        writer.write_all(data)?;
        writer.flush()?;
        Ok(())
    }

    pub fn resize(&self, cols: u16, rows: u16) -> Result<()> {
        self.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        Ok(())
    }

    pub fn kill(&mut self) {
        let _ = self.child.kill();
    }
}

const CURSOR_QUERY: &[u8] = b"\x1b[6n";
/// "The cursor is at row 1, column 1."
const CURSOR_REPORT: &[u8] = b"\x1b[1;1R";

/// Removes cursor-position queries from terminal output. Returns the rest and how
/// many queries there were.
fn take_cursor_queries(data: &[u8]) -> (Vec<u8>, usize) {
    let mut out = Vec::with_capacity(data.len());
    let mut queries = 0;
    let mut i = 0;
    while i < data.len() {
        if data[i..].starts_with(CURSOR_QUERY) {
            queries += 1;
            i += CURSOR_QUERY.len();
        } else {
            out.push(data[i]);
            i += 1;
        }
    }
    (out, queries)
}

/// Start `cloud <args>` in a new terminal.
pub fn start(app: &AppHandle, args: &[&str], cols: u16, rows: u16) -> Result<Session> {
    let cloud = relay_core::cloud::find().context("Cloud AI is not installed yet")?;
    let pair = native_pty_system().openpty(PtySize {
        rows,
        cols,
        pixel_width: 0,
        pixel_height: 0,
    })?;
    let mut cmd = CommandBuilder::new(cloud);
    cmd.args(args);
    cmd.cwd(relay_core::paths::work_dir());
    let child = pair.slave.spawn_command(cmd)?;
    drop(pair.slave);
    let mut reader = pair.master.try_clone_reader()?;
    let writer = Arc::new(Mutex::new(pair.master.take_writer()?));
    let answers = writer.clone();
    let app = app.clone();
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let (data, queries) = take_cursor_queries(&buf[..n]);
                    if queries > 0 {
                        if let Ok(mut w) = answers.lock() {
                            for _ in 0..queries {
                                let _ = w.write_all(CURSOR_REPORT);
                            }
                            let _ = w.flush();
                        }
                    }
                    if !data.is_empty() {
                        let _ = app.emit("pty-data", Data { id, data });
                    }
                }
            }
        }
        let _ = app.emit("pty-exit", id);
    });
    Ok(Session {
        id,
        master: pair.master,
        writer,
        child,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_queries_are_answered_not_shown() {
        let (rest, n) = take_cursor_queries(b"\x1b[6nhello\x1b[6n");
        assert_eq!(rest, b"hello");
        assert_eq!(n, 2);
        let (rest, n) = take_cursor_queries(b"https://example.com/oauth");
        assert_eq!(rest, b"https://example.com/oauth");
        assert_eq!(n, 0);
    }
}
