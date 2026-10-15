//! In-app approvals: relay-screen asks the owner in Relay's own window instead of a
//! separate popup when the app is running.
//!
//! The app listens on 127.0.0.1 with a random token and writes both to
//! `<data dir>\approval.json` (only the owner's account can read it). relay-screen posts
//! the same request it would show in its native prompt; the chat shows a card, the
//! owner answers there, and the answer goes back. If the app isn't running, relay-screen
//! falls back to its native prompt. While the app runs, input injected by software
//! (SendInput, which is how Relay and Claude click and type) is dropped when it targets
//! Relay's window, so automation can never answer its own approvals, as with the
//! native prompt.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;

pub const FILE: &str = "approval.json";
const MAX_BODY: usize = 16 * 1024 * 1024;

#[derive(Default)]
pub struct Approvals {
    next: AtomicU64,
    pending: Mutex<HashMap<u64, oneshot::Sender<String>>>,
}

/// What the chat shows: relay-screen's prompt request plus an id.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Request {
    #[serde(default)]
    pub id: u64,
    pub title: String,
    #[serde(default)]
    pub detail: String,
    #[serde(default)]
    pub step: Option<u64>,
    #[serde(default)]
    pub total: Option<u64>,
    /// PNG preview, base64.
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default = "default_timeout")]
    pub timeout: f64,
    #[serde(default = "default_approve")]
    pub approve_label: String,
    #[serde(default)]
    pub allow_batch: bool,
    #[serde(default)]
    pub batch_label: String,
}

fn default_timeout() -> f64 {
    120.0
}

fn default_approve() -> String {
    "Approve".into()
}

const DECISIONS: [&str; 3] = ["approve", "approve_batch", "deny"];

/// The owner's answer from the chat card.
pub fn answer(app: &AppHandle, id: u64, decision: &str) -> bool {
    if !DECISIONS.contains(&decision) {
        return false;
    }
    let sender = app
        .state::<Approvals>()
        .pending
        .lock()
        .expect("approvals lock")
        .remove(&id);
    sender.is_some_and(|s| s.send(decision.to_string()).is_ok())
}

/// Start listening and publish the address for relay-screen.
pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        if let Err(err) = serve(app).await {
            eprintln!("relay: in-app approvals are off: {err:#}");
        }
    });
    #[cfg(windows)]
    input_guard::start();
}

async fn serve(app: AppHandle) -> anyhow::Result<()> {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
    let port = listener.local_addr()?.port();
    let token = uuid::Uuid::new_v4().simple().to_string();
    let dir = relay_core::paths::data_dir();
    std::fs::create_dir_all(&dir)?;
    let info = serde_json::json!({ "port": port, "token": token, "pid": std::process::id() });
    std::fs::write(dir.join(FILE), info.to_string())?;
    loop {
        let (stream, _) = listener.accept().await?;
        let app = app.clone();
        let token = token.clone();
        tauri::async_runtime::spawn(async move {
            let _ = handle(app, stream, &token).await;
        });
    }
}

/// Remove the address file so relay-screen goes back to its own prompt.
pub fn stop() {
    let _ = std::fs::remove_file(relay_core::paths::data_dir().join(FILE));
}

async fn handle(app: AppHandle, stream: TcpStream, token: &str) -> anyhow::Result<()> {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).await?;
    let path_ok = line.starts_with("POST /ask ");
    let mut length = 0usize;
    let mut authorized = false;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).await? == 0 {
            break;
        }
        let header = header.trim_end();
        if header.is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':') {
            match name.trim().to_ascii_lowercase().as_str() {
                "content-length" => length = value.trim().parse().unwrap_or(0),
                "authorization" => authorized = value.trim() == format!("Bearer {token}"),
                _ => {}
            }
        }
    }
    if !path_ok || !authorized || length == 0 || length > MAX_BODY {
        return respond(
            reader.get_mut(),
            403,
            &serde_json::json!({ "error": "forbidden" }),
        )
        .await;
    }
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body).await?;
    let mut request: Request = serde_json::from_slice(&body)?;
    let decision = ask(&app, &mut request).await;
    respond(
        reader.get_mut(),
        200,
        &serde_json::json!({ "decision": decision }),
    )
    .await
}

/// Show the card, bring Relay forward, and wait for the owner (or the timeout).
/// Only steps that can't be undone get here in the default "risky" mode.
async fn ask(app: &AppHandle, request: &mut Request) -> String {
    let state = app.state::<Approvals>();
    request.id = state.next.fetch_add(1, Ordering::SeqCst) + 1;
    let (tx, rx) = oneshot::channel();
    state
        .pending
        .lock()
        .expect("approvals lock")
        .insert(request.id, tx);
    // Bring Relay forward for the card, then hand the screen back to whatever window
    // Relay was working in, so the next step lands where Claude expects it.
    let previous = focus::foreground();
    crate::show_main(app);
    let _ = app.emit("approval", request.clone());
    let wait = Duration::from_secs_f64(request.timeout.clamp(5.0, 3600.0));
    let decision = match tokio::time::timeout(wait, rx).await {
        Ok(Ok(decision)) => decision,
        _ => "timeout".to_string(),
    };
    state
        .pending
        .lock()
        .expect("approvals lock")
        .remove(&request.id);
    let _ = app.emit("approval-done", request.id);
    focus::restore(previous);
    crate::stage::after_approval(app);
    decision
}

/// The window that had the keyboard before Relay showed a card.
mod focus {
    #[cfg(windows)]
    pub fn foreground() -> isize {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            GetForegroundWindow, GetWindowThreadProcessId,
        };
        let hwnd = unsafe { GetForegroundWindow() };
        let mut pid = 0u32;
        unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
        if hwnd.is_null() || pid == std::process::id() {
            0
        } else {
            hwnd as isize
        }
    }

    #[cfg(windows)]
    pub fn restore(hwnd: isize) {
        use windows_sys::Win32::UI::WindowsAndMessaging::{IsWindow, SetForegroundWindow};
        let hwnd = hwnd as windows_sys::Win32::Foundation::HWND;
        if !hwnd.is_null() && unsafe { IsWindow(hwnd) } != 0 {
            unsafe { SetForegroundWindow(hwnd) };
        }
    }

    #[cfg(not(windows))]
    pub fn foreground() -> isize {
        0
    }

    #[cfg(not(windows))]
    pub fn restore(_: isize) {}
}

async fn respond(stream: &mut TcpStream, status: u16, body: &Value) -> anyhow::Result<()> {
    let body = body.to_string();
    let reason = if status == 200 { "OK" } else { "Forbidden" };
    let head = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes()).await?;
    stream.write_all(body.as_bytes()).await?;
    stream.flush().await?;
    Ok(())
}

/// Drops software-injected mouse and keyboard input aimed at Relay's own windows,
/// the same rule relay-screen's native prompt uses.
#[cfg(windows)]
mod input_guard {
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, GetForegroundWindow, GetMessageW, GetWindowThreadProcessId,
        SetWindowsHookExW, WindowFromPoint, KBDLLHOOKSTRUCT, LLKHF_INJECTED, LLMHF_INJECTED, MSG,
        MSLLHOOKSTRUCT, WH_KEYBOARD_LL, WH_MOUSE_LL,
    };

    fn ours(hwnd: HWND) -> bool {
        if hwnd.is_null() {
            return false;
        }
        let mut pid = 0u32;
        unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
        pid == std::process::id()
    }

    unsafe extern "system" fn on_mouse(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code >= 0 {
            let info = &*(lparam as *const MSLLHOOKSTRUCT);
            if info.flags & LLMHF_INJECTED != 0
                && ours(WindowFromPoint(POINT {
                    x: info.pt.x,
                    y: info.pt.y,
                }))
            {
                return 1;
            }
        }
        CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam)
    }

    unsafe extern "system" fn on_key(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code >= 0 {
            let info = &*(lparam as *const KBDLLHOOKSTRUCT);
            if info.flags & LLKHF_INJECTED != 0 && ours(GetForegroundWindow()) {
                return 1;
            }
        }
        CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam)
    }

    pub fn start() {
        std::thread::spawn(|| unsafe {
            let mouse = SetWindowsHookExW(WH_MOUSE_LL, Some(on_mouse), std::ptr::null_mut(), 0);
            let key = SetWindowsHookExW(WH_KEYBOARD_LL, Some(on_key), std::ptr::null_mut(), 0);
            if mouse.is_null() || key.is_null() {
                eprintln!("relay: could not install the input guard");
            }
            // Low-level hooks run on this thread's message loop.
            let mut msg: MSG = std::mem::zeroed();
            while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {}
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_relay_screens_prompt_request() {
        let r: Request = serde_json::from_str(
            r#"{"title":"Relay wants to act on your PC","detail":"Click at (10, 20)","step":1,"total":3,"image":null,"timeout":120,"approve_label":"Approve","allow_batch":true,"batch_label":"Approve rest of batch","restore_hwnd":0}"#,
        )
        .unwrap();
        assert_eq!(r.detail, "Click at (10, 20)");
        assert!(r.allow_batch);
        assert_eq!(r.total, Some(3));
        let bare: Request = serde_json::from_str(r#"{"title":"Re-arm Relay?"}"#).unwrap();
        assert_eq!(bare.approve_label, "Approve");
        assert_eq!(bare.timeout, 120.0);
    }
}
