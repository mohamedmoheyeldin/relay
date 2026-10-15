//! Running helper programs without flashing console windows.

use std::path::PathBuf;
use std::process::Stdio;

use anyhow::{Context, Result};
use tokio::io::{AsyncBufReadExt, BufReader};

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// A tokio command that never opens a console window on Windows.
pub fn command(program: impl AsRef<std::ffi::OsStr>) -> tokio::process::Command {
    #[allow(unused_mut)]
    let mut cmd = tokio::process::Command::new(program);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd.stdin(Stdio::null());
    // A timed-out or abandoned call must not leave the process running.
    cmd.kill_on_drop(true);
    cmd
}

/// Output of a finished command.
#[derive(Debug, Clone)]
pub struct Output {
    pub ok: bool,
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

pub async fn run(program: impl AsRef<std::ffi::OsStr>, args: &[&str]) -> Result<Output> {
    let program = program.as_ref().to_owned();
    let out = command(&program)
        .args(args)
        .output()
        .await
        .with_context(|| format!("could not start {}", program.to_string_lossy()))?;
    Ok(Output {
        ok: out.status.success(),
        code: out.status.code(),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    })
}

/// Run a command and hand each stdout and stderr line to `on_line` as it arrives.
pub async fn run_streaming(
    mut cmd: tokio::process::Command,
    on_line: impl Fn(String) + Send + Sync + 'static,
) -> Result<Output> {
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().context("could not start the command")?;
    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");
    let on_line = std::sync::Arc::new(on_line);
    let out_cb = on_line.clone();
    let out_task = tokio::spawn(async move {
        let mut all = String::new();
        let mut lines = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            all.push_str(&line);
            all.push('\n');
            out_cb(line);
        }
        all
    });
    let err_task = tokio::spawn(async move {
        let mut all = String::new();
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            all.push_str(&line);
            all.push('\n');
            on_line(line);
        }
        all
    });
    let status = child.wait().await?;
    Ok(Output {
        ok: status.success(),
        code: status.code(),
        stdout: out_task.await.unwrap_or_default(),
        stderr: err_task.await.unwrap_or_default(),
    })
}

/// Find a program on PATH (adds `.exe` on Windows).
pub fn which(name: &str) -> Option<PathBuf> {
    let exe = if cfg!(windows) && !name.ends_with(".exe") {
        format!("{name}.exe")
    } else {
        name.to_string()
    };
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|dir| dir.join(&exe))
            .find(|candidate| candidate.is_file())
    })
}
