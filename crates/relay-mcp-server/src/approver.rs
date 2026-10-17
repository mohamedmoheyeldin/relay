//! Asks the owner with a native dialog. The stand-alone server has no window of its own, so
//! approvals appear as a system dialog on top of everything. No answer in time counts as no.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use relay_mcp::{ApprovalRequest, Approver, Decision};

/// How long a dialog waits before it counts as a refusal.
pub const ANSWER_TIMEOUT: Duration = Duration::from_secs(120);

/// Shows the native question; returns true for "allow". Implemented per platform.
#[async_trait]
pub trait Prompter: Send + Sync {
    async fn ask(&self, title: &str, text: &str, timeout: Duration) -> bool;
}

/// Always says no (headless machines, or `--approvals deny`).
pub struct DenyPrompter;

#[async_trait]
impl Prompter for DenyPrompter {
    async fn ask(&self, _title: &str, _text: &str, _timeout: Duration) -> bool {
        false
    }
}

/// The system dialog for this platform.
pub struct NativePrompter {
    counter: AtomicU64,
}

impl NativePrompter {
    pub fn new() -> Self {
        Self {
            counter: AtomicU64::new(1),
        }
    }
}

impl Default for NativePrompter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(windows)]
mod imp {
    use super::*;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        FindWindowW, MessageBoxW, PostMessageW, IDYES, MB_DEFBUTTON2, MB_ICONWARNING,
        MB_SETFOREGROUND, MB_TOPMOST, MB_YESNO, WM_COMMAND,
    };

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    #[async_trait]
    impl Prompter for NativePrompter {
        async fn ask(&self, title: &str, text: &str, timeout: Duration) -> bool {
            // A unique title lets us find our own dialog to dismiss it on timeout.
            let n = self.counter.fetch_add(1, Ordering::Relaxed);
            let title = format!("{title} (#{n})");
            let body = format!("{text}\n\nYes = allow, No = deny.");
            let t = title.clone();
            let dialog = tokio::task::spawn_blocking(move || {
                let (t, b) = (wide(&t), wide(&body));
                // SAFETY: both buffers are NUL-terminated and outlive the call.
                let answer = unsafe {
                    MessageBoxW(
                        None,
                        PCWSTR(b.as_ptr()),
                        PCWSTR(t.as_ptr()),
                        MB_YESNO | MB_ICONWARNING | MB_TOPMOST | MB_SETFOREGROUND | MB_DEFBUTTON2,
                    )
                };
                answer == IDYES
            });
            tokio::pin!(dialog);
            tokio::select! {
                r = &mut dialog => r.unwrap_or(false),
                _ = tokio::time::sleep(timeout) => {
                    let t = wide(&title);
                    // SAFETY: `t` is NUL-terminated; posting a message to a window we may not own is harmless.
                    unsafe {
                        if let Ok(hwnd) = FindWindowW(PCWSTR::null(), PCWSTR(t.as_ptr())) {
                            // IDNO = 7: press the "No" button.
                            let _ = PostMessageW(Some(hwnd), WM_COMMAND, WPARAM(7), LPARAM(0));
                        }
                    }
                    let _ = dialog.await;
                    false
                }
            }
        }
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use super::*;
    use tokio::process::Command;

    #[async_trait]
    impl Prompter for NativePrompter {
        async fn ask(&self, title: &str, text: &str, timeout: Duration) -> bool {
            // Text goes in as arguments, never into the script source.
            let script = "on run argv\n\
                set r to display dialog (item 2 of argv) with title (item 1 of argv) buttons {\"Deny\", \"Allow\"} default button \"Deny\" with icon caution giving up after (item 3 of argv as integer)\n\
                return (button returned of r) & \"|\" & (gave up of r)\n\
                end run";
            let secs = timeout.as_secs().to_string();
            let out = Command::new("osascript")
                .arg("-e")
                .arg(script)
                .arg(title)
                .arg(text)
                .arg(&secs)
                .output()
                .await;
            match out {
                Ok(o) if o.status.success() => {
                    String::from_utf8_lossy(&o.stdout).trim() == "Allow|false"
                }
                _ => false,
            }
        }
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
mod imp {
    use super::*;

    #[async_trait]
    impl Prompter for NativePrompter {
        async fn ask(&self, _title: &str, _text: &str, _timeout: Duration) -> bool {
            false
        }
    }
}

/// Adapts a [`Prompter`] to the approval interfaces the tool host and executor use.
pub struct DialogApprover {
    prompter: Box<dyn Prompter>,
    timeout: Duration,
}

impl DialogApprover {
    pub fn new(prompter: Box<dyn Prompter>) -> Self {
        Self {
            prompter,
            timeout: ANSWER_TIMEOUT,
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

#[async_trait]
impl Approver for DialogApprover {
    async fn ask(&self, request: ApprovalRequest) -> Decision {
        let title = format!("Relay: {}", request.title);
        if self
            .prompter
            .ask(&title, &request.detail, self.timeout)
            .await
        {
            Decision::Approved
        } else {
            Decision::Denied
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use relay_protocol::Risk;

    struct Fixed(bool);

    #[async_trait]
    impl Prompter for Fixed {
        async fn ask(&self, _t: &str, _x: &str, _d: Duration) -> bool {
            self.0
        }
    }

    fn request() -> ApprovalRequest {
        ApprovalRequest {
            run_id: 1,
            title: "Move to Recycle Bin".into(),
            detail: "old.zip".into(),
            risk: Risk::Destructive,
        }
    }

    #[tokio::test]
    async fn the_owners_answer_decides() {
        let yes = DialogApprover::new(Box::new(Fixed(true)));
        let no = DialogApprover::new(Box::new(Fixed(false)));
        assert_eq!(yes.ask(request()).await, Decision::Approved);
        assert_eq!(no.ask(request()).await, Decision::Denied);
        let deny = DialogApprover::new(Box::new(DenyPrompter));
        assert_eq!(deny.ask(request()).await, Decision::Denied);
    }
}
