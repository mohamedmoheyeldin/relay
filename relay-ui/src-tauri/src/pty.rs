//! The in-app terminal pane for signing in to Claude Code (D-045).
//!
//! It runs `claude auth login` in a pseudo-terminal (ConPTY on Windows) and streams
//! the bytes to xterm.js. The owner signs in themselves; Relay never sees the token,
//! it only checks `claude auth status` afterwards.

use std::io::{Read, Write};

use anyhow::{Context, Result};
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use tauri::{AppHandle, Emitter};

pub struct Session {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn Child + Send + Sync>,
}

impl Session {
    pub fn write(&mut self, data: &[u8]) -> Result<()> {
        self.writer.write_all(data)?;
        self.writer.flush()?;
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

/// Start `claude <args>` in a new terminal.
pub fn start(app: &AppHandle, args: &[&str], cols: u16, rows: u16) -> Result<Session> {
    let claude = relay_core::claude::find().context("Claude Code is not installed yet")?;
    let pair = native_pty_system().openpty(PtySize {
        rows,
        cols,
        pixel_width: 0,
        pixel_height: 0,
    })?;
    let mut cmd = CommandBuilder::new(claude);
    cmd.args(args);
    cmd.cwd(relay_core::paths::work_dir());
    let child = pair.slave.spawn_command(cmd)?;
    drop(pair.slave);
    let mut reader = pair.master.try_clone_reader()?;
    let writer = pair.master.take_writer()?;
    let app = app.clone();
    std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let _ = app.emit("pty-data", buf[..n].to_vec());
                }
            }
        }
        let _ = app.emit("pty-exit", ());
    });
    Ok(Session {
        master: pair.master,
        writer,
        child,
    })
}
