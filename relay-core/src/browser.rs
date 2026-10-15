//! The browser lane: Claude drives the owner's Chromium browser (Brave, Chrome,
//! Edge, ...) through Playwright MCP, registered with Claude Code as relay-browser.
//!
//! Chromium refuses remote control of the everyday profile, so the lane uses its
//! own profile under `%LOCALAPPDATA%\Relay\browser-profile`. The owner signs in to
//! sites there once ("Open Relay's browser") and the sign-ins stay.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::mcp::{ServerSpec, RELAY_BROWSER};
use crate::settings::Settings;
use crate::system::{self, BrowserInfo};
use crate::{paths, proc};

/// Pinned so an upstream release can't change the tools under Claude's feet.
pub const PLAYWRIGHT_MCP: &str = "@playwright/mcp@0.0.83";

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
    args.extend(
        [
            "-y",
            PLAYWRIGHT_MCP,
            "--executable-path",
            &browser.to_string_lossy(),
            "--user-data-dir",
            &profile_dir().to_string_lossy(),
            "--output-dir",
            &settings
                .privacy
                .runs_dir()
                .join("browser")
                .to_string_lossy(),
        ]
        .map(String::from),
    );
    ServerSpec::Stdio {
        name: RELAY_BROWSER.into(),
        command: npx.command.clone(),
        args,
        env: Default::default(),
    }
}

/// What Settings > Browser shows.
#[derive(Debug, Clone, Serialize)]
pub struct LaneState {
    pub enabled: bool,
    pub browsers: Vec<BrowserInfo>,
    pub browser: Option<BrowserInfo>,
    pub node: bool,
    pub profile_dir: PathBuf,
    /// Why the lane can't run, in words for the owner.
    pub problem: Option<String>,
}

pub fn state(settings: &Settings) -> LaneState {
    let browsers = system::browsers();
    let browser = pick(&settings.browser.preferred, &browsers);
    let node = npx().is_some();
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
    } else {
        None
    };
    LaneState {
        enabled: settings.browser.lane_enabled,
        browsers,
        browser,
        node,
        profile_dir: profile_dir(),
        problem,
    }
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
    fn spec_points_playwright_at_the_browser_and_relay_profile() {
        let npx = Npx {
            command: "node".into(),
            prefix: vec!["npx-cli.js".into()],
        };
        let spec = spec(&npx, Path::new("C:/brave.exe"), &Settings::default());
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
