//! App updates from GitHub Releases (D-044).
//!
//! Relay asks GitHub for the latest release of its own repository, and when it is
//! newer, downloads its `Relay-Setup*.exe` into the owner's Downloads folder and runs
//! it. Signed in-place updates through Tauri's updater come with code signing.

use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::download::{self, Cancel};

pub const REPO: &str = "mohamedmoheyeldin/relay";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UpdateInfo {
    pub current: String,
    pub latest: String,
    pub available: bool,
    pub notes: String,
    pub url: String,
    pub asset_name: String,
    pub asset_url: String,
    pub asset_size: u64,
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    body: String,
    html_url: String,
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    size: u64,
}

/// "v1.2.3" or "1.2.3-beta" -> (1, 2, 3).
pub fn parse_semver(v: &str) -> Option<(u64, u64, u64)> {
    let core = v.trim().trim_start_matches('v');
    let core = core.split(['-', '+']).next()?;
    let mut parts = core.split('.').map(|p| p.parse::<u64>().ok());
    Some((
        parts.next()??,
        parts.next().flatten().unwrap_or(0),
        parts.next().flatten().unwrap_or(0),
    ))
}

pub fn is_newer(latest: &str, current: &str) -> bool {
    match (parse_semver(latest), parse_semver(current)) {
        (Some(l), Some(c)) => l > c,
        _ => false,
    }
}

pub async fn check(current: &str) -> Result<UpdateInfo> {
    let client = download::client()?;
    let resp = client
        .get(format!(
            "https://api.github.com/repos/{REPO}/releases/latest"
        ))
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .context("could not reach GitHub")?;
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(UpdateInfo {
            current: current.into(),
            latest: current.into(),
            available: false,
            notes: "No release has been published yet.".into(),
            url: format!("https://github.com/{REPO}/releases"),
            asset_name: String::new(),
            asset_url: String::new(),
            asset_size: 0,
        });
    }
    if !resp.status().is_success() {
        bail!("GitHub answered {}", resp.status());
    }
    let release: Release = resp.json().await?;
    let asset = release
        .assets
        .iter()
        .find(|a| a.name.starts_with("Relay-Setup") && a.name.ends_with(".exe"));
    Ok(UpdateInfo {
        current: current.into(),
        available: asset.is_some() && is_newer(&release.tag_name, current),
        latest: release.tag_name.trim_start_matches('v').to_string(),
        notes: release.body,
        url: release.html_url,
        asset_name: asset.map(|a| a.name.clone()).unwrap_or_default(),
        asset_url: asset
            .map(|a| a.browser_download_url.clone())
            .unwrap_or_default(),
        asset_size: asset.map(|a| a.size).unwrap_or(0),
    })
}

/// Download the new installer into Downloads and return its path.
pub async fn download_installer(
    info: &UpdateInfo,
    cancel: &Cancel,
    on_progress: &(dyn Fn(u64, u64) + Send + Sync),
) -> Result<PathBuf> {
    if info.asset_url.is_empty() {
        bail!("this release has no installer");
    }
    let dest = crate::paths::downloads_dir().join(&info.asset_name);
    download::download(
        &info.asset_url,
        &dest,
        "",
        Some(info.asset_size),
        cancel,
        on_progress,
    )
    .await?;
    Ok(dest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert_eq!(parse_semver("v1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_semver("0.2"), Some((0, 2, 0)));
        assert_eq!(parse_semver("1.0.0-beta.1"), Some((1, 0, 0)));
        assert_eq!(parse_semver("nightly"), None);
        assert!(is_newer("v0.2.0", "0.1.0"));
        assert!(!is_newer("v0.1.0", "0.1.0"));
        assert!(!is_newer("garbage", "0.1.0"));
    }
}
