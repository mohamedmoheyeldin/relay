//! The browser lane: Claude drives the owner's Chromium browser (Brave, Chrome,
//! Edge, ...) through Playwright MCP, registered with Claude Code as relay-browser.
//!
//! By default it drives the browser the owner already has open, with their own
//! sign-ins, through the Playwright Extension: Chromium refuses remote debugging of
//! the everyday profile, but the extension bridges it. Its token (kept in Credential
//! Manager, handed over by Relay's launcher) skips the per-connection approval page.
//! The older mode runs the browser on Relay's own profile under
//! `%LOCALAPPDATA%\Relay\browser-profile` instead.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::mcp::{ServerSpec, RELAY_BROWSER};
use crate::settings::Settings;
use crate::system::{self, BrowserInfo};
use crate::{paths, proc};

/// Pinned so an upstream release can't change the tools under Claude's feet.
pub const PLAYWRIGHT_MCP: &str = "@playwright/mcp@0.0.83";

/// The Playwright Extension in the Chrome Web Store (Brave and Edge install from it too).
pub const EXTENSION_ID: &str = "mmlmfjhmonkocbjadbfplnigmagldckm";
pub const EXTENSION_URL: &str =
    "https://chromewebstore.google.com/detail/playwright-extension/mmlmfjhmonkocbjadbfplnigmagldckm";
/// The environment variable Playwright MCP reads the extension's token from, and the
/// name it is stored under in Credential Manager for relay-browser.
pub const TOKEN_SECRET: &str = "PLAYWRIGHT_MCP_EXTENSION_TOKEN";

pub fn profile_dir() -> PathBuf {
    paths::data_dir().join("browser-profile")
}

/// The browser the lane uses: the one picked in Settings, or the first
/// Chromium-based one found.
pub fn pick(preferred: &str, found: &[BrowserInfo]) -> Option<BrowserInfo> {
    let chromium = found.iter().filter(|b| b.chromium && !b.path.is_empty());
    match preferred {
        "auto" | "" => chromium
            .clone()
            .find(|b| b.name.to_lowercase().contains("brave"))
            .or_else(|| chromium.clone().next())
            .cloned(),
        name => chromium.clone().find(|b| b.name == name).cloned(),
    }
}

/// How to run npx: Node's own `npx-cli.js` when it sits next to node.exe (no
/// console window, no `.cmd` shim), else `cmd /c npx`.
#[derive(Debug, Clone, PartialEq)]
pub struct Npx {
    pub command: String,
    pub prefix: Vec<String>,
}

pub fn npx() -> Option<Npx> {
    let node = proc::which("node")?;
    let cli = node
        .parent()?
        .join("node_modules")
        .join("npm")
        .join("bin")
        .join("npx-cli.js");
    if cli.is_file() {
        return Some(Npx {
            command: node.to_string_lossy().into_owned(),
            prefix: vec![cli.to_string_lossy().into_owned()],
        });
    }
    proc::which("npx.cmd").or_else(|| proc::which("npx"))?;
    Some(Npx {
        command: "cmd".into(),
        prefix: vec!["/c".into(), "npx".into()],
    })
}

/// How Claude Code starts relay-browser.
pub fn spec(npx: &Npx, browser: &Path, settings: &Settings) -> ServerSpec {
    let mut args = npx.prefix.clone();
    args.extend(["-y", PLAYWRIGHT_MCP].map(String::from));
    if settings.browser.use_own {
        // Playwright opens its connect page in the running browser with this program.
        args.push("--extension".into());
    }
    args.extend([
        "--executable-path".into(),
        browser.to_string_lossy().into_owned(),
    ]);
    if !settings.browser.use_own {
        args.extend([
            "--user-data-dir".into(),
            profile_dir().to_string_lossy().into_owned(),
        ]);
    }
    args.extend([
        "--output-dir".into(),
        settings
            .privacy
            .runs_dir()
            .join("browser")
            .to_string_lossy()
            .into_owned(),
    ]);
    ServerSpec::Stdio {
        name: RELAY_BROWSER.into(),
        command: npx.command.clone(),
        args,
        env: Default::default(),
    }
}

/// The browser's everyday user data folder: `%LOCALAPPDATA%\<vendor>\<product>\User
/// Data` for a browser installed under `...\<vendor>\<product>\Application\`.
pub fn user_data_dir(browser: &BrowserInfo, local_app_data: &Path) -> Option<PathBuf> {
    let parts: Vec<&str> = browser.path.split(['\\', '/']).collect();
    let app = parts
        .iter()
        .rposition(|p| p.eq_ignore_ascii_case("Application"))?;
    let start = parts[..app].iter().rposition(|p| {
        p.to_lowercase().starts_with("program files")
            || p.eq_ignore_ascii_case("AppData")
            || p.eq_ignore_ascii_case("Local")
    })? + 1;
    if start >= app {
        return None;
    }
    let mut dir = local_app_data.to_path_buf();
    for p in &parts[start..app] {
        dir.push(p);
    }
    Some(dir.join("User Data"))
}

/// Whether any profile in `user_data` has the Playwright Extension installed.
pub fn extension_installed(user_data: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(user_data) else {
        return false;
    };
    entries.flatten().any(|e| {
        let name = e.file_name().to_string_lossy().into_owned();
        let profile = name == "Default" || name.starts_with("Profile ");
        profile && {
            let dir = e.path();
            dir.join("Extensions").join(EXTENSION_ID).is_dir()
                || ["Preferences", "Secure Preferences"].iter().any(|f| {
                    std::fs::read_to_string(dir.join(f))
                        .ok()
                        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
                        .is_some_and(|v| {
                            v["extensions"]["settings"][EXTENSION_ID]
                                .as_object()
                                .is_some_and(|o| !o.is_empty())
                        })
                })
        }
    })
}

/// The `HKLM\SOFTWARE\Policies` key a Chromium browser reads, from its program path.
pub fn policy_key(browser: &BrowserInfo) -> Option<&'static str> {
    let path = browser.path.to_lowercase();
    if path.contains("bravesoftware") {
        Some(r"SOFTWARE\Policies\BraveSoftware\Brave")
    } else if path.ends_with("chrome.exe") && path.contains("google") {
        Some(r"SOFTWARE\Policies\Google\Chrome")
    } else if path.ends_with("msedge.exe") {
        Some(r"SOFTWARE\Policies\Microsoft\Edge")
    } else {
        None
    }
}

/// Whether Relay's installer asked this browser to add the extension itself
/// (`ExtensionInstallForcelist`), so it only has to wait for the browser to do it.
#[cfg(windows)]
pub fn extension_by_policy(browser: &BrowserInfo) -> bool {
    use winreg::enums::HKEY_LOCAL_MACHINE;
    use winreg::RegKey;
    let Some(key) = policy_key(browser) else {
        return false;
    };
    RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey(format!(r"{key}\ExtensionInstallForcelist"))
        .map(|k| {
            k.enum_values()
                .flatten()
                .any(|(_, v)| v.to_string().starts_with(EXTENSION_ID))
        })
        .unwrap_or(false)
}

#[cfg(not(windows))]
pub fn extension_by_policy(_browser: &BrowserInfo) -> bool {
    false
}

pub fn has_token() -> bool {
    crate::secrets::get(RELAY_BROWSER, TOKEN_SECRET).is_ok_and(|t| !t.trim().is_empty())
}

/// Brave Origin is a paid build: every new profile, Relay's included, shows a
/// "restore your purchase" page and loads nothing else until the owner enters their
/// purchase ID there once. Brave records that in the profile's `Local State`.
pub fn needs_origin_purchase(browser: &BrowserInfo, profile: &Path) -> bool {
    if !browser.path.to_lowercase().contains("brave-origin") {
        return false;
    }
    let validated = std::fs::read_to_string(profile.join("Local State"))
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|v| v["brave"]["origin"]["purchase_validated"].as_bool());
    validated != Some(true)
}

pub const ORIGIN_PURCHASE: &str = "Brave Origin asks for your purchase ID once in Relay's own \
browser profile, and opens no pages until then. Click \"Open to sign in\", choose \
\"Restore Brave Origin purchase\" and enter your purchase ID.";

/// What Settings > Browser shows.
#[derive(Debug, Clone, Serialize)]
pub struct LaneState {
    pub enabled: bool,
    pub browsers: Vec<BrowserInfo>,
    pub browser: Option<BrowserInfo>,
    pub node: bool,
    pub profile_dir: PathBuf,
    /// Driving the owner's own open browser (the Playwright Extension).
    pub use_own: bool,
    /// The extension is installed in the browser's everyday profile.
    pub extension: bool,
    /// Its token is saved in Relay.
    pub token: bool,
    pub extension_url: String,
    /// Why the lane can't run, in words for the owner.
    pub problem: Option<String>,
}

pub fn state(settings: &Settings) -> LaneState {
    let browsers = system::browsers();
    let browser = pick(&settings.browser.preferred, &browsers);
    let node = npx().is_some();
    let use_own = settings.browser.use_own;
    let extension = use_own
        && browser
            .as_ref()
            .and_then(|b| user_data_dir(b, &local_app_data()))
            .is_some_and(|d| extension_installed(&d));
    let token = use_own && has_token();
    let name = browser.as_ref().map_or("your browser", |b| b.name.as_str());
    let problem = if browser.is_none() {
        Some(if settings.browser.preferred == "auto" {
            "No Chromium-based browser (Brave, Chrome, Edge) was found.".to_string()
        } else {
            format!(
                "{} can't be driven by Relay. Pick a Chromium-based browser.",
                settings.browser.preferred
            )
        })
    } else if !node {
        Some(
            "The browser lane needs Node.js. Install it from nodejs.org, then reopen Relay.".into(),
        )
    } else if use_own && !extension {
        Some(if browser.as_ref().is_some_and(extension_by_policy) {
            format!(
                "{name} is adding the Playwright Extension that Relay set up during install. \
                     It appears within a minute while {name} is open; if it doesn't, close and \
                     reopen {name}."
            )
        } else {
            format!(
                "Relay works in {name} through the Playwright Extension, which isn't \
                     installed yet. Run Relay Setup and choose Repair to add it, or add it in \
                     Settings > Browser."
            )
        })
    } else if use_own && !token {
        Some(format!(
            "One step is left, once: click the Playwright Extension's icon in {name} (under the \
             puzzle piece), copy the token it shows and paste it in Settings > Browser. The \
             extension makes this token itself, as your OK for Relay to use {name}."
        ))
    } else if !use_own
        && browser
            .as_ref()
            .is_some_and(|b| needs_origin_purchase(b, &profile_dir()))
    {
        Some(ORIGIN_PURCHASE.into())
    } else {
        None
    };
    LaneState {
        enabled: settings.browser.lane_enabled,
        browsers,
        browser,
        node,
        profile_dir: profile_dir(),
        use_own,
        extension,
        token,
        extension_url: EXTENSION_URL.into(),
        problem,
    }
}

fn local_app_data() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| dirs::data_local_dir().unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b(name: &str, chromium: bool) -> BrowserInfo {
        BrowserInfo {
            name: name.into(),
            path: format!("C:/{name}.exe"),
            chromium,
        }
    }

    #[test]
    fn auto_prefers_brave_then_any_chromium() {
        let found = [
            b("Firefox", false),
            b("Google Chrome", true),
            b("Brave Origin", true),
        ];
        assert_eq!(pick("auto", &found).unwrap().name, "Brave Origin");
        assert_eq!(pick("auto", &found[..2]).unwrap().name, "Google Chrome");
        assert!(pick("auto", &found[..1]).is_none());
        assert!(pick("Firefox", &found).is_none());
        assert_eq!(pick("Google Chrome", &found).unwrap().name, "Google Chrome");
    }

    #[test]
    fn brave_origin_needs_its_purchase_in_relays_profile() {
        let dir = std::env::temp_dir().join(format!("relay-origin-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let origin = BrowserInfo {
            name: "Brave Origin".into(),
            path: "C:/Program Files/BraveSoftware/Brave-Origin/Application/brave.exe".into(),
            chromium: true,
        };
        assert!(needs_origin_purchase(&origin, &dir)); // no Local State yet
        let state = dir.join("Local State");
        std::fs::write(
            &state,
            r#"{"brave":{"origin":{"purchase_validated":false}}}"#,
        )
        .unwrap();
        assert!(needs_origin_purchase(&origin, &dir));
        std::fs::write(
            &state,
            r#"{"brave":{"origin":{"purchase_validated":true}}}"#,
        )
        .unwrap();
        assert!(!needs_origin_purchase(&origin, &dir));
        assert!(!needs_origin_purchase(
            &b("Google Chrome", true),
            Path::new("nowhere")
        ));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn finds_the_everyday_user_data_folder() {
        let local = Path::new("C:/Users/me/AppData/Local");
        let brave = BrowserInfo {
            name: "Brave Origin".into(),
            path: r"C:\Program Files\BraveSoftware\Brave-Origin\Application\brave.exe".into(),
            chromium: true,
        };
        assert_eq!(
            user_data_dir(&brave, local).unwrap(),
            local
                .join("BraveSoftware")
                .join("Brave-Origin")
                .join("User Data")
        );
        let edge = BrowserInfo {
            name: "Microsoft Edge".into(),
            path: r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe".into(),
            chromium: true,
        };
        assert_eq!(
            user_data_dir(&edge, local).unwrap(),
            local.join("Microsoft").join("Edge").join("User Data")
        );
        assert!(user_data_dir(&b("Odd", true), local).is_none());
    }

    #[test]
    fn policy_key_follows_the_browser() {
        let at = |path: &str| BrowserInfo {
            name: "x".into(),
            path: path.into(),
            chromium: true,
        };
        assert_eq!(
            policy_key(&at(
                r"C:\Program Files\BraveSoftware\Brave-Origin\Application\brave.exe"
            )),
            Some(r"SOFTWARE\Policies\BraveSoftware\Brave")
        );
        assert_eq!(
            policy_key(&at(
                r"C:\Program Files\Google\Chrome\Application\chrome.exe"
            )),
            Some(r"SOFTWARE\Policies\Google\Chrome")
        );
        assert_eq!(
            policy_key(&at(
                r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe"
            )),
            Some(r"SOFTWARE\Policies\Microsoft\Edge")
        );
        assert_eq!(policy_key(&at(r"C:\Vivaldi\vivaldi.exe")), None);
    }

    #[test]
    fn sees_the_extension_in_any_profile() {
        let dir = std::env::temp_dir().join(format!("relay-ext-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("Default")).unwrap();
        assert!(!extension_installed(&dir));
        std::fs::create_dir_all(dir.join("Profile 2")).unwrap();
        std::fs::write(
            dir.join("Profile 2").join("Secure Preferences"),
            format!(r#"{{"extensions":{{"settings":{{"{EXTENSION_ID}":{{"state":1}}}}}}}}"#),
        )
        .unwrap();
        assert!(extension_installed(&dir));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn own_browser_spec_uses_the_extension_and_no_relay_profile() {
        let npx = Npx {
            command: "node".into(),
            prefix: vec!["npx-cli.js".into()],
        };
        let spec = spec(&npx, Path::new("C:/brave.exe"), &Settings::default());
        let ServerSpec::Stdio { args, .. } = spec else {
            panic!("expected stdio");
        };
        assert!(args.contains(&"--extension".to_string()));
        assert!(!args.contains(&"--user-data-dir".to_string()));
        let at = args.iter().position(|a| a == "--executable-path").unwrap();
        assert_eq!(args[at + 1], "C:/brave.exe");
    }

    #[test]
    fn spec_points_playwright_at_the_browser_and_relay_profile() {
        let npx = Npx {
            command: "node".into(),
            prefix: vec!["npx-cli.js".into()],
        };
        let mut settings = Settings::default();
        settings.browser.use_own = false;
        let spec = spec(&npx, Path::new("C:/brave.exe"), &settings);
        let ServerSpec::Stdio { command, args, .. } = spec else {
            panic!("expected stdio");
        };
        assert_eq!(command, "node");
        assert_eq!(args[..3], ["npx-cli.js", "-y", PLAYWRIGHT_MCP]);
        let at = args.iter().position(|a| a == "--executable-path").unwrap();
        assert_eq!(args[at + 1], "C:/brave.exe");
        assert!(args.contains(&"--user-data-dir".to_string()));
    }
}
