//! Generic child-process runner: spawn, feed stdin, stream stdout lines, enforce timeouts,
//! cancel by killing the whole process tree.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

use crate::{AiError, AiEvent, Finish, RunHandle, TimeoutKind, Timeouts};

/// How long to wait for a process to exit on its own after its final message.
const EXIT_GRACE: Duration = Duration::from_secs(5);
/// Keep at most this much stderr for error messages.
const STDERR_LIMIT: usize = 16 * 1024;
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// What to run.
#[derive(Debug, Clone)]
pub struct ProcessSpec {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub work_dir: PathBuf,
    /// Written to stdin, then stdin is closed.
    pub stdin: String,
    /// Scratch directory deleted once the process is gone.
    pub cleanup_dir: Option<PathBuf>,
}

/// Interprets a tool's stdout and decides how a run ended.
pub trait StreamProtocol: Send + 'static {
    /// Called for every stdout line.
    fn on_line(&mut self, line: &str) -> Vec<AiEvent>;
    /// Called if the process ended without the protocol reporting [`AiEvent::Finished`].
    fn on_exit(&mut self, exit_code: Option<i32>, stderr: &str) -> Finish;
}

fn spawn_error(program: &std::path::Path, e: std::io::Error) -> AiError {
    if e.kind() == std::io::ErrorKind::NotFound {
        AiError::NotInstalled {
            program: program.display().to_string(),
        }
    } else {
        AiError::Spawn(e.to_string())
    }
}

/// Start the process and return a handle whose stream always ends with [`AiEvent::Finished`].
pub fn spawn(
    spec: ProcessSpec,
    protocol: impl StreamProtocol,
    timeouts: Timeouts,
) -> Result<RunHandle, AiError> {
    let mut command = Command::new(&spec.program);
    command
        .args(&spec.args)
        .envs(&spec.env)
        .current_dir(&spec.work_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    #[cfg(unix)]
    command.process_group(0);
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(e) => {
            if let Some(dir) = &spec.cleanup_dir {
                let _ = std::fs::remove_dir_all(dir);
            }
            return Err(spawn_error(&spec.program, e));
        }
    };
    let cleanup_dir = spec.cleanup_dir.clone();

    let stdin = child.stdin.take();
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| AiError::Io("no stdout pipe".into()))?;
    let stderr = child.stderr.take();

    let (tx, rx) = mpsc::channel(256);
    let cancel = CancellationToken::new();
    let handle = RunHandle::new(rx, cancel.clone());

    tokio::spawn(async move {
        if let Some(mut stdin) = stdin {
            let data = spec.stdin;
            tokio::spawn(async move {
                if let Err(e) = stdin.write_all(data.as_bytes()).await {
                    debug!("writing prompt to stdin failed: {e}");
                }
                let _ = stdin.shutdown().await;
            });
        }
        let stderr_task = tokio::spawn(async move {
            let mut buf = String::new();
            if let Some(mut err) = stderr {
                let mut raw = Vec::new();
                let _ = err.read_to_end(&mut raw).await;
                buf = String::from_utf8_lossy(&raw).into_owned();
                if buf.len() > STDERR_LIMIT {
                    let cut = buf.len() - STDERR_LIMIT;
                    let boundary = (cut..buf.len())
                        .find(|i| buf.is_char_boundary(*i))
                        .unwrap_or(buf.len());
                    buf = buf[boundary..].to_string();
                }
            }
            buf
        });
        drive(child, stdout, protocol, timeouts, cancel, tx, stderr_task).await;
        if let Some(dir) = cleanup_dir {
            let _ = tokio::fs::remove_dir_all(dir).await;
        }
    });
    Ok(handle)
}

async fn drive(
    mut child: Child,
    stdout: tokio::process::ChildStdout,
    mut protocol: impl StreamProtocol,
    timeouts: Timeouts,
    cancel: CancellationToken,
    tx: mpsc::Sender<AiEvent>,
    stderr_task: tokio::task::JoinHandle<String>,
) {
    let mut lines = BufReader::new(stdout).lines();
    let total = tokio::time::sleep(timeouts.total);
    tokio::pin!(total);
    let mut finish: Option<Finish> = None;
    let mut receiver_gone = false;

    'read: loop {
        tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                finish = Some(Finish::Cancelled);
                break 'read;
            }
            _ = &mut total => {
                finish = Some(Finish::TimedOut(TimeoutKind::Total));
                break 'read;
            }
            _ = tokio::time::sleep(timeouts.idle) => {
                finish = Some(Finish::TimedOut(TimeoutKind::Idle));
                break 'read;
            }
            line = lines.next_line() => match line {
                Ok(Some(text)) => {
                    for event in protocol.on_line(&text) {
                        let done = matches!(event, AiEvent::Finished(_));
                        if done {
                            if let AiEvent::Finished(f) = &event {
                                finish = Some(f.clone());
                            }
                            continue;
                        }
                        if tx.send(event).await.is_err() {
                            receiver_gone = true;
                            break 'read;
                        }
                    }
                    if finish.is_some() {
                        break 'read;
                    }
                }
                Ok(None) => break 'read,
                Err(e) => {
                    finish = Some(Finish::Failed(AiError::Io(e.to_string())));
                    break 'read;
                }
            }
        }
    }

    let must_kill =
        receiver_gone || matches!(finish, Some(Finish::Cancelled) | Some(Finish::TimedOut(_)));
    if must_kill {
        kill_tree(&mut child).await;
    } else if tokio::time::timeout(EXIT_GRACE, child.wait())
        .await
        .is_err()
    {
        warn!("process did not exit after its final message; killing it");
        kill_tree(&mut child).await;
    }
    let status = child.wait().await.ok();
    let stderr = stderr_task.await.unwrap_or_default();

    let finish = match finish {
        Some(f) => f,
        None => protocol.on_exit(status.and_then(|s| s.code()), &stderr),
    };
    if !receiver_gone {
        let _ = tx.send(AiEvent::Finished(finish)).await;
    }
}

/// Kill the child and everything it started.
pub async fn kill_tree(child: &mut Child) {
    if let Some(pid) = child.id() {
        #[cfg(windows)]
        {
            let mut cmd = Command::new("taskkill");
            cmd.args(["/PID", &pid.to_string(), "/T", "/F"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            cmd.creation_flags(CREATE_NO_WINDOW);
            let _ = cmd.status().await;
        }
        #[cfg(not(windows))]
        {
            let mut cmd = Command::new("kill");
            cmd.args(["-KILL", &format!("-{pid}")])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            let _ = cmd.status().await;
        }
    }
    let _ = child.kill().await;
}
