//! Browser control for website tasks: Relay drives a real browser through the Playwright MCP server
//! (started with `npx`), using a browser already installed on this PC and a profile of its own, so
//! a sign-in the owner does once stays signed in. Nothing else about the owner's browser is touched.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use relay_mcp_client::{McpClient, McpError, ServerCommand};
use serde_json::Value;
use tokio::sync::Mutex as AsyncMutex;
use tracing::{info, warn};

/// The Playwright MCP release Relay was tested with.
pub const PLAYWRIGHT_MCP: &str = "@playwright/mcp@0.0.83";
pub const SERVER_LABEL: &str = "Playwright browser";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserStatus {
    pub available: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BrowserOutput {
    pub text: String,
    pub is_error: bool,
}

/// What the web loop needs from a browser. The real one is [`PlaywrightBrowser`].
#[async_trait]
pub trait BrowserDriver: Send + Sync {
    /// Name shown in tags, for example "Playwright browser".
    fn label(&self) -> &str {
        SERVER_LABEL
    }
    /// Whether browser control can run on this PC (cheap; no process is started).
    async fn status(&self) -> BrowserStatus;
    /// Run one browser tool, starting the browser on first use.
    async fn call(
        &self,
        tool: &str,
        args: Value,
        timeout: Duration,
    ) -> Result<BrowserOutput, String>;
    /// Close the browser and stop the server.
    async fn shutdown(&self);
}

/// Find a program on PATH (and `.cmd`/`.exe` variants on Windows).
pub fn find_on_path(names: &[&str]) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        for name in names {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// Where `npx` is, if Node.js is installed.
pub fn find_npx() -> Option<PathBuf> {
    let names: &[&str] = if cfg!(windows) {
        &["npx.cmd", "npx.exe"]
    } else {
        &["npx"]
    };
    if let Some(p) = find_on_path(names) {
        return Some(p);
    }
    let known: &[&str] = if cfg!(windows) {
        &[
            r"C:\Program Files\nodejs\npx.cmd",
            r"C:\Program Files (x86)\nodejs\npx.cmd",
        ]
    } else {
        &[
            "/usr/local/bin/npx",
            "/opt/homebrew/bin/npx",
            "/usr/bin/npx",
        ]
    };
    known.iter().map(PathBuf::from).find(|p| p.is_file())
}

/// A Chromium-based browser installed on this PC: Chrome, Edge or Brave, in that order.
pub fn find_browser() -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if cfg!(windows) {
        let roots = [
            std::env::var_os("ProgramFiles"),
            std::env::var_os("ProgramFiles(x86)"),
            std::env::var_os("LOCALAPPDATA"),
        ];
        for root in roots.into_iter().flatten().map(PathBuf::from) {
            for rel in [
                r"Google\Chrome\Application\chrome.exe",
                r"Microsoft\Edge\Application\msedge.exe",
                r"BraveSoftware\Brave-Browser\Application\brave.exe",
                r"BraveSoftware\Brave-Origin\Application\brave.exe",
                r"BraveSoftware\Brave-Browser-Beta\Application\brave.exe",
            ] {
                candidates.push(root.join(rel));
            }
        }
    } else {
        for p in [
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
            "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
            "/Applications/Brave Browser.app/Contents/MacOS/Brave Browser",
            "/usr/bin/google-chrome",
            "/usr/bin/chromium",
        ] {
            candidates.push(PathBuf::from(p));
        }
    }
    candidates.into_iter().find(|p| p.is_file())
}

/// Arguments for the Playwright MCP server.
pub fn server_args(browser: &Path, profile: &Path) -> Vec<String> {
    vec![
        "-y".into(),
        PLAYWRIGHT_MCP.into(),
        "--executable-path".into(),
        browser.display().to_string(),
        "--user-data-dir".into(),
        profile.display().to_string(),
        // Pages cannot reach local files or the owner's PC through the browser.
        "--block-service-workers".into(),
    ]
}

pub struct PlaywrightBrowser {
    profile: PathBuf,
    client: AsyncMutex<Option<Arc<McpClient>>>,
}

impl PlaywrightBrowser {
    pub fn new(profile_dir: PathBuf) -> Self {
        Self {
            profile: profile_dir,
            client: AsyncMutex::new(None),
        }
    }

    async fn client(&self) -> Result<Arc<McpClient>, String> {
        let mut slot = self.client.lock().await;
        if let Some(c) = slot.as_ref() {
            return Ok(c.clone());
        }
        let npx = find_npx().ok_or("Node.js was not found")?;
        let browser = find_browser().ok_or("no Chrome, Edge or Brave browser was found")?;
        let _ = std::fs::create_dir_all(&self.profile);
        info!("starting the browser server with {}", browser.display());
        let cmd = ServerCommand {
            program: npx,
            args: server_args(&browser, &self.profile),
            ..ServerCommand::default()
        };
        let client = McpClient::start("playwright", &cmd, Duration::from_secs(120))
            .await
            .map_err(|e| e.to_string())?;
        *slot = Some(client.clone());
        Ok(client)
    }
}

#[async_trait]
impl BrowserDriver for PlaywrightBrowser {
    async fn status(&self) -> BrowserStatus {
        let node = find_npx().is_some();
        let browser = find_browser();
        match (node, browser) {
            (true, Some(b)) => BrowserStatus {
                available: true,
                detail: format!(
                    "Ready. Uses {} and its own profile, so you only sign in once.",
                    b.file_stem().and_then(|s| s.to_str()).unwrap_or("your browser")
                ),
            },
            (false, _) => BrowserStatus {
                available: false,
                detail: "Node.js was not found. Install it from nodejs.org to let Relay work in a browser.".into(),
            },
            (true, None) => BrowserStatus {
                available: false,
                detail: "No Chrome, Edge or Brave browser was found on this PC.".into(),
            },
        }
    }

    async fn call(
        &self,
        tool: &str,
        args: Value,
        timeout: Duration,
    ) -> Result<BrowserOutput, String> {
        let client = self.client().await?;
        match client.call(tool, args, timeout).await {
            Ok(r) => Ok(BrowserOutput {
                text: r.text,
                is_error: r.is_error,
            }),
            Err(e @ (McpError::Closed(_) | McpError::Spawn(..))) => {
                // The server died: forget it so the next call starts a fresh one.
                warn!("browser server lost: {e}");
                *self.client.lock().await = None;
                Err(e.to_string())
            }
            Err(e) => Err(e.to_string()),
        }
    }

    async fn shutdown(&self) {
        let client = self.client.lock().await.take();
        if let Some(c) = client {
            let _ = c
                .call(
                    "browser_close",
                    serde_json::json!({}),
                    Duration::from_secs(5),
                )
                .await;
            c.shutdown().await;
        }
    }
}

/// Used when browser control is switched off or cannot start.
pub struct NoBrowser(pub String);

#[async_trait]
impl BrowserDriver for NoBrowser {
    async fn status(&self) -> BrowserStatus {
        BrowserStatus {
            available: false,
            detail: self.0.clone(),
        }
    }
    async fn call(&self, _: &str, _: Value, _: Duration) -> Result<BrowserOutput, String> {
        Err(self.0.clone())
    }
    async fn shutdown(&self) {}
}

/// A browser the owner can switch on and off from Settings without restarting.
pub struct SwitchableBrowser {
    enabled: std::sync::atomic::AtomicBool,
    inner: Arc<dyn BrowserDriver>,
}

impl SwitchableBrowser {
    pub fn new(inner: Arc<dyn BrowserDriver>, enabled: bool) -> Self {
        Self {
            enabled: std::sync::atomic::AtomicBool::new(enabled),
            inner,
        }
    }

    pub fn set_enabled(&self, on: bool) {
        self.enabled.store(on, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.load(std::sync::atomic::Ordering::Relaxed)
    }
}

#[async_trait]
impl BrowserDriver for SwitchableBrowser {
    fn label(&self) -> &str {
        self.inner.label()
    }
    async fn status(&self) -> BrowserStatus {
        if self.is_enabled() {
            self.inner.status().await
        } else {
            BrowserStatus {
                available: false,
                detail: "Browser control is switched off.".into(),
            }
        }
    }
    async fn call(
        &self,
        tool: &str,
        args: Value,
        timeout: Duration,
    ) -> Result<BrowserOutput, String> {
        if !self.is_enabled() {
            return Err("Browser control is switched off.".into());
        }
        self.inner.call(tool, args, timeout).await
    }
    async fn shutdown(&self) {
        self.inner.shutdown().await;
    }
}
