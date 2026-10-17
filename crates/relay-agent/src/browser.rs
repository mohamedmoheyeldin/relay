//! Browser control for website tasks: Relay drives a real browser through the Playwright MCP server
//! (started with `npx`).
//!
//! Preferred: the owner's **own** browser, with their tabs and sign-ins, through the official
//! "Playwright MCP Bridge" extension (`--extension`). The owner approves the connection in the
//! browser; Relay never reads the browser's profile, passwords or cookies.
//! Fallback: a separate profile of Chrome or Edge that Relay keeps for itself (the owner signs in
//! there once). Brave Origin cannot be used that way: a fresh profile stops at its purchase screen.

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
/// The official Playwright MCP Bridge extension (Chrome Web Store id).
pub const BRIDGE_EXTENSION_ID: &str = "mmlmfjhmonkocbjadbfplnigmagldckm";
pub const BRIDGE_URL: &str =
    "https://chromewebstore.google.com/detail/playwright-mcp-bridge/mmlmfjhmonkocbjadbfplnigmagldckm";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserStatus {
    pub available: bool,
    pub detail: String,
    /// A link that fixes or improves the setup (for example, the extension's store page).
    pub action_url: Option<String>,
    /// `yours` (the owner's browser), `separate` (Relay's own profile) or `none`.
    pub mode: &'static str,
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

/// A Chromium-based browser installed on this PC.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledBrowser {
    pub name: &'static str,
    pub exe: PathBuf,
    /// The browser's own profile folder (only read to see whether the bridge extension is installed).
    pub user_data: Option<PathBuf>,
    /// Can Relay start it with a fresh profile of its own?
    pub separate_ok: bool,
}

struct Known {
    name: &'static str,
    exe: &'static str,
    data: &'static str,
    separate_ok: bool,
}

#[cfg(windows)]
const KNOWN: [Known; 5] = [
    Known {
        name: "Google Chrome",
        exe: r"Google\Chrome\Application\chrome.exe",
        data: r"Google\Chrome\User Data",
        separate_ok: true,
    },
    Known {
        name: "Microsoft Edge",
        exe: r"Microsoft\Edge\Application\msedge.exe",
        data: r"Microsoft\Edge\User Data",
        separate_ok: true,
    },
    Known {
        name: "Brave",
        exe: r"BraveSoftware\Brave-Browser\Application\brave.exe",
        data: r"BraveSoftware\Brave-Browser\User Data",
        separate_ok: true,
    },
    Known {
        name: "Brave Origin",
        exe: r"BraveSoftware\Brave-Origin\Application\brave.exe",
        data: r"BraveSoftware\Brave-Origin\User Data",
        separate_ok: false,
    },
    Known {
        name: "Brave Beta",
        exe: r"BraveSoftware\Brave-Browser-Beta\Application\brave.exe",
        data: r"BraveSoftware\Brave-Browser-Beta\User Data",
        separate_ok: true,
    },
];

/// Browsers on this PC, in order of preference.
pub fn installed_browsers() -> Vec<InstalledBrowser> {
    let mut out = Vec::new();
    #[cfg(windows)]
    {
        let roots: Vec<PathBuf> = ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"]
            .iter()
            .filter_map(std::env::var_os)
            .map(PathBuf::from)
            .collect();
        let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
        for k in KNOWN.iter() {
            if let Some(exe) = roots.iter().map(|r| r.join(k.exe)).find(|p| p.is_file()) {
                out.push(InstalledBrowser {
                    name: k.name,
                    exe,
                    user_data: local.as_ref().map(|l| l.join(k.data)),
                    separate_ok: k.separate_ok,
                });
            }
        }
    }
    #[cfg(not(windows))]
    {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        for (name, exe, data) in [
            (
                "Google Chrome",
                "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
                "Library/Application Support/Google/Chrome",
            ),
            (
                "Microsoft Edge",
                "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
                "Library/Application Support/Microsoft Edge",
            ),
            (
                "Brave",
                "/Applications/Brave Browser.app/Contents/MacOS/Brave Browser",
                "Library/Application Support/BraveSoftware/Brave-Browser",
            ),
        ] {
            let exe = PathBuf::from(exe);
            if exe.is_file() {
                out.push(InstalledBrowser {
                    name,
                    exe,
                    user_data: home.as_ref().map(|h| h.join(data)),
                    separate_ok: true,
                });
            }
        }
    }
    out
}

/// Is the bridge extension installed in any profile of this browser?
pub fn has_bridge(user_data: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(user_data) else {
        return false;
    };
    entries.flatten().any(|e| {
        e.path()
            .join("Extensions")
            .join(BRIDGE_EXTENSION_ID)
            .is_dir()
    })
}

/// How browser tasks will run on this PC.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrowserPlan {
    /// The owner's own browser through the bridge extension.
    Yours(InstalledBrowser),
    /// A separate profile Relay keeps for itself.
    Separate(InstalledBrowser),
    /// The only browsers here need the extension to be usable (e.g. Brave Origin).
    NeedsBridge(InstalledBrowser),
    NoBrowser,
}

pub fn plan_browser(browsers: &[InstalledBrowser]) -> BrowserPlan {
    if let Some(b) = browsers
        .iter()
        .find(|b| b.user_data.as_deref().is_some_and(has_bridge))
    {
        return BrowserPlan::Yours(b.clone());
    }
    if let Some(b) = browsers.iter().find(|b| b.separate_ok) {
        return BrowserPlan::Separate(b.clone());
    }
    match browsers.first() {
        Some(b) => BrowserPlan::NeedsBridge(b.clone()),
        None => BrowserPlan::NoBrowser,
    }
}

/// Arguments for the Playwright MCP server.
pub fn server_args(plan: &BrowserPlan, profile: &Path) -> Option<Vec<String>> {
    match plan {
        BrowserPlan::Yours(b) => Some(vec![
            "-y".into(),
            PLAYWRIGHT_MCP.into(),
            "--extension".into(),
            "--executable-path".into(),
            b.exe.display().to_string(),
        ]),
        BrowserPlan::Separate(b) => Some(vec![
            "-y".into(),
            PLAYWRIGHT_MCP.into(),
            "--executable-path".into(),
            b.exe.display().to_string(),
            "--user-data-dir".into(),
            profile.display().to_string(),
            "--block-service-workers".into(),
        ]),
        _ => None,
    }
}

pub fn status_for(plan: &BrowserPlan, node: bool) -> BrowserStatus {
    if !node {
        return BrowserStatus {
            available: false,
            detail:
                "Node.js was not found. Install it from nodejs.org to let Relay work in a browser."
                    .into(),
            action_url: Some("https://nodejs.org/".into()),
            mode: "none",
        };
    }
    match plan {
        BrowserPlan::Yours(b) => BrowserStatus {
            available: true,
            detail: if has_extension_token() {
                format!("Uses your {} with your sign-ins, through the Playwright extension. Connects without asking (token saved).", b.name)
            } else {
                format!("Uses your {} with your sign-ins, through the Playwright extension. Allow the connection in the tab Relay opens, or save the extension's token below so it connects without asking.", b.name)
            },
            action_url: None,
            mode: "yours",
        },
        BrowserPlan::Separate(b) => BrowserStatus {
            available: true,
            detail: format!(
                "Uses a separate {} window that Relay keeps for itself; sign in there once. To use your own browser and sign-ins instead, add the Playwright MCP Bridge extension.",
                b.name
            ),
            action_url: Some(BRIDGE_URL.into()),
            mode: "separate",
        },
        BrowserPlan::NeedsBridge(b) => BrowserStatus {
            available: false,
            detail: format!(
                "To let Relay use your {}, add the Playwright MCP Bridge extension to it (one click in the Chrome Web Store), then try again.",
                b.name
            ),
            action_url: Some(BRIDGE_URL.into()),
            mode: "none",
        },
        BrowserPlan::NoBrowser => BrowserStatus {
            available: false,
            detail: "No Chrome, Edge or Brave browser was found on this PC.".into(),
            action_url: None,
            mode: "none",
        },
    }
}

/// The bridge extension's connection token (shown in the extension's popup). With it, the owner does
/// not have to approve every connection. Kept in memory here; the app stores it in its settings file.
static EXTENSION_TOKEN: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

pub fn set_extension_token(token: Option<String>) {
    let clean = token
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty() && t.len() <= 512 && t.chars().all(|c| c.is_ascii_graphic()));
    if let Ok(mut slot) = EXTENSION_TOKEN.lock() {
        *slot = clean;
    }
}

pub fn has_extension_token() -> bool {
    EXTENSION_TOKEN.lock().map(|t| t.is_some()).unwrap_or(false)
}

fn extension_token() -> Option<String> {
    EXTENSION_TOKEN.lock().ok().and_then(|t| t.clone())
}

pub struct PlaywrightBrowser {
    profile: PathBuf,
    client: AsyncMutex<Option<(Arc<McpClient>, BrowserPlan)>>,
}

impl PlaywrightBrowser {
    pub fn new(profile_dir: PathBuf) -> Self {
        Self {
            profile: profile_dir,
            client: AsyncMutex::new(None),
        }
    }

    async fn client(&self) -> Result<(Arc<McpClient>, BrowserPlan), String> {
        let mut slot = self.client.lock().await;
        if let Some(c) = slot.as_ref() {
            return Ok(c.clone());
        }
        let npx = find_npx().ok_or("Node.js was not found")?;
        let plan = plan_browser(&installed_browsers());
        let args =
            server_args(&plan, &self.profile).ok_or_else(|| status_for(&plan, true).detail)?;
        if matches!(plan, BrowserPlan::Separate(_)) {
            let _ = std::fs::create_dir_all(&self.profile);
        }
        info!("starting the browser server: {plan:?}");
        let mut env = Vec::new();
        if matches!(plan, BrowserPlan::Yours(_)) {
            if let Some(t) = extension_token() {
                env.push(("PLAYWRIGHT_MCP_EXTENSION_TOKEN".to_string(), t));
            }
        }
        let cmd = ServerCommand {
            program: npx,
            args,
            env,
            ..ServerCommand::default()
        };
        let client = McpClient::start("playwright", &cmd, Duration::from_secs(120))
            .await
            .map_err(|e| e.to_string())?;
        *slot = Some((client.clone(), plan.clone()));
        Ok((client, plan))
    }
}

#[async_trait]
impl BrowserDriver for PlaywrightBrowser {
    async fn status(&self) -> BrowserStatus {
        status_for(&plan_browser(&installed_browsers()), find_npx().is_some())
    }

    async fn call(
        &self,
        tool: &str,
        args: Value,
        timeout: Duration,
    ) -> Result<BrowserOutput, String> {
        let (client, plan) = self.client().await?;
        match client.call(tool, args, timeout).await {
            Ok(r) => Ok(BrowserOutput {
                text: r.text,
                is_error: r.is_error,
            }),
            Err(McpError::Timeout(_)) if matches!(plan, BrowserPlan::Yours(_)) => {
                // Usually the connection is waiting for the owner's OK in the browser.
                let name = match &plan {
                    BrowserPlan::Yours(b) => b.name,
                    _ => "browser",
                };
                // Forget this server: the next try starts fresh (and picks up a token if one was added).
                *self.client.lock().await = None;
                Err(format!(
                    "Your {name} is waiting for you to allow the connection. Click Allow in the Playwright tab that opened, then say continue. To skip this every time, copy the token from the Playwright extension (click its icon) into Settings, AI & Privacy, Browser control."
                ))
            }
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
        if let Some((c, plan)) = client {
            // Never close the owner's own browser; only the separate one Relay started.
            if matches!(plan, BrowserPlan::Separate(_)) {
                let _ = c
                    .call(
                        "browser_close",
                        serde_json::json!({}),
                        Duration::from_secs(5),
                    )
                    .await;
            }
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
            action_url: None,
            mode: "none",
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
                action_url: None,
                mode: "none",
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

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("relay-browser-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap_or_default();
        d
    }

    fn b(name: &'static str, data: Option<PathBuf>, separate_ok: bool) -> InstalledBrowser {
        InstalledBrowser {
            name,
            exe: PathBuf::from(format!("{name}.exe")),
            user_data: data,
            separate_ok,
        }
    }

    #[test]
    fn the_owners_browser_with_the_extension_wins() {
        let data = temp("bridge");
        let _ = std::fs::create_dir_all(
            data.join("Profile 1")
                .join("Extensions")
                .join(BRIDGE_EXTENSION_ID),
        );
        let plan = plan_browser(&[
            b("Google Chrome", None, true),
            b("Brave Origin", Some(data.clone()), false),
        ]);
        assert!(matches!(plan, BrowserPlan::Yours(ref x) if x.name == "Brave Origin"));
        let args = server_args(&plan, Path::new("p")).unwrap_or_default();
        assert!(
            args.contains(&"--extension".to_string())
                && !args.contains(&"--user-data-dir".to_string())
        );
        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn brave_origin_without_the_extension_asks_for_it_instead_of_opening_a_purchase_screen() {
        let empty = temp("nobridge");
        let plan = plan_browser(&[b("Brave Origin", Some(empty.clone()), false)]);
        assert!(matches!(plan, BrowserPlan::NeedsBridge(_)));
        assert!(
            server_args(&plan, Path::new("p")).is_none(),
            "nothing is started"
        );
        let st = status_for(&plan, true);
        assert!(!st.available && st.action_url.as_deref() == Some(BRIDGE_URL));
        let sep = plan_browser(&[
            b("Brave Origin", Some(empty.clone()), false),
            b("Google Chrome", None, true),
        ]);
        assert!(matches!(sep, BrowserPlan::Separate(ref x) if x.name == "Google Chrome"));
        assert!(server_args(&sep, Path::new("prof"))
            .unwrap_or_default()
            .contains(&"--user-data-dir".to_string()));
        let _ = std::fs::remove_dir_all(&empty);
    }

    #[test]
    fn missing_node_is_explained() {
        let st = status_for(&BrowserPlan::NoBrowser, false);
        assert!(!st.available && st.detail.contains("Node.js"));
    }
}
