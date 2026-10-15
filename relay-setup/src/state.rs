//! What Relay Setup offers, decided from what is installed. Pure logic, no Windows
//! calls, so it is unit-tested on any OS.
//!
//! Tauri's NSIS template writes `Software\Microsoft\Windows\CurrentVersion\Uninstall\Relay`
//! under HKLM for a per-machine install (Relay 0.1.1 and later, D-063) and under HKCU
//! for the per-user Relay 0.1.0. The values read here are the ones it writes:
//! `DisplayVersion` ("0.1.5"), `InstallLocation` (quoted: `"C:\Program Files\Relay"`)
//! and `UninstallString` (quoted: `"C:\Program Files\Relay\uninstall.exe"`).

use serde::Serialize;
use std::cmp::Ordering;

/// The registry key both Tauri's NSIS template and Relay Setup use.
pub const UNINSTALL_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\Relay";

/// A dotted version such as `0.1.5`, `v1.2` or `1.0.0-beta.2+build7`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    parts: Vec<u64>,
    pre: Option<String>,
}

impl Version {
    pub fn parse(text: &str) -> Option<Version> {
        let text = text.trim();
        let text = text.strip_prefix(['v', 'V']).unwrap_or(text);
        let text = text.split('+').next().unwrap_or_default();
        let (core, pre) = match text.split_once('-') {
            Some((core, pre)) if !pre.is_empty() => (core, Some(pre.to_string())),
            Some(_) => return None,
            None => (text, None),
        };
        if core.is_empty() {
            return None;
        }
        let parts = core
            .split('.')
            .map(|p| p.parse::<u64>().ok())
            .collect::<Option<Vec<_>>>()?;
        Some(Version { parts, pre })
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        let len = self.parts.len().max(other.parts.len());
        for i in 0..len {
            let a = self.parts.get(i).copied().unwrap_or(0);
            let b = other.parts.get(i).copied().unwrap_or(0);
            match a.cmp(&b) {
                Ordering::Equal => {}
                unequal => return unequal,
            }
        }
        // A release sorts after its pre-releases (1.0.0-rc.1 < 1.0.0).
        match (&self.pre, &other.pre) {
            (None, None) => Ordering::Equal,
            (None, Some(_)) => Ordering::Greater,
            (Some(_), None) => Ordering::Less,
            (Some(a), Some(b)) => compare_pre(a, b),
        }
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Semver pre-release order: numeric identifiers numerically, below text ones.
fn compare_pre(a: &str, b: &str) -> Ordering {
    let mut left = a.split('.');
    let mut right = b.split('.');
    loop {
        match (left.next(), right.next()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) => {
                let order = match (x.parse::<u64>(), y.parse::<u64>()) {
                    (Ok(x), Ok(y)) => x.cmp(&y),
                    (Ok(_), Err(_)) => Ordering::Less,
                    (Err(_), Ok(_)) => Ordering::Greater,
                    (Err(_), Err(_)) => x.cmp(y),
                };
                if order != Ordering::Equal {
                    return order;
                }
            }
        }
    }
}

/// Where an install's registry key lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    /// HKLM: for all users, in Program Files (0.1.1 and later).
    Machine,
    /// HKCU: the per-user Relay 0.1.0 (C:\relay by default).
    User,
}

/// One Relay install as its uninstall key describes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Installed {
    pub version: String,
    pub location: Option<String>,
    pub uninstall_string: Option<String>,
    pub scope: Scope,
}

/// How the installed Relay compares with the one inside this setup.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    NotInstalled,
    Older,
    Same,
    Newer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Install,
    Update,
    Repair,
    Uninstall,
}

impl Action {
    pub fn parse(text: &str) -> Option<Action> {
        match text {
            "install" => Some(Action::Install),
            "update" => Some(Action::Update),
            "repair" => Some(Action::Repair),
            "uninstall" => Some(Action::Uninstall),
            _ => None,
        }
    }

    /// Install, update and repair all run the embedded NSIS installer.
    pub fn runs_payload(self) -> bool {
        !matches!(self, Action::Uninstall)
    }
}

/// What the first screen shows: the install it is about, how it compares, and the
/// actions on offer, main one first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Plan {
    pub status: Status,
    pub installed: Option<Installed>,
    pub actions: Vec<Action>,
}

/// Decide what to offer. The per-machine install wins when both keys exist: the
/// per-user 0.1.0 left behind is removed by the installer's pre-install hook anyway.
/// A version that can't be read is treated as older, so the owner can update over it.
pub fn plan(machine: Option<Installed>, user: Option<Installed>, payload: &str) -> Plan {
    let Some(installed) = machine.or(user) else {
        return Plan {
            status: Status::NotInstalled,
            installed: None,
            actions: vec![Action::Install],
        };
    };
    let status = match (Version::parse(&installed.version), Version::parse(payload)) {
        (Some(have), Some(ship)) => match have.cmp(&ship) {
            Ordering::Less => Status::Older,
            Ordering::Equal => Status::Same,
            Ordering::Greater => Status::Newer,
        },
        _ => Status::Older,
    };
    let actions = match status {
        Status::NotInstalled => vec![Action::Install],
        Status::Older => vec![Action::Update, Action::Uninstall],
        Status::Same => vec![Action::Repair, Action::Uninstall],
        Status::Newer => vec![Action::Uninstall],
    };
    Plan {
        status,
        installed: Some(installed),
        actions,
    }
}

/// `"C:\Program Files\Relay"` -> `C:\Program Files\Relay` (also drops a trailing `\`).
pub fn unquote(text: &str) -> String {
    let text = text.trim();
    let text = text
        .strip_prefix('"')
        .and_then(|t| t.strip_suffix('"'))
        .unwrap_or(text);
    text.trim_end_matches('\\').to_string()
}

/// The program in an UninstallString, without quotes or arguments.
pub fn uninstaller_path(uninstall_string: &str) -> Option<String> {
    let text = uninstall_string.trim();
    let path = if let Some(rest) = text.strip_prefix('"') {
        rest.split('"').next().unwrap_or_default()
    } else {
        // Unquoted: everything up to and including ".exe".
        let lower = text.to_ascii_lowercase();
        match lower.find(".exe") {
            Some(end) => &text[..end + 4],
            None => text,
        }
    };
    let path = path.trim();
    (!path.is_empty()).then(|| path.to_string())
}

/// The folder an uninstaller lives in, the `_?=` NSIS needs to run it in place.
pub fn parent_dir(path: &str) -> Option<String> {
    let path = path.trim_end_matches('\\');
    let (dir, _) = path.rsplit_once('\\')?;
    (!dir.is_empty()).then(|| dir.to_string())
}

/// Quote one argument for a Windows command line (CommandLineToArgvW rules).
pub fn quote_arg(arg: &str) -> String {
    if !arg.is_empty() && !arg.contains([' ', '\t', '"']) {
        return arg.to_string();
    }
    let mut out = String::from('"');
    let mut backslashes = 0;
    for c in arg.chars() {
        match c {
            '\\' => backslashes += 1,
            '"' => {
                out.push_str(&"\\".repeat(backslashes * 2 + 1));
                out.push('"');
                backslashes = 0;
            }
            _ => {
                out.push_str(&"\\".repeat(backslashes));
                out.push(c);
                backslashes = 0;
            }
        }
    }
    out.push_str(&"\\".repeat(backslashes * 2));
    out.push('"');
    out
}

/// A plain-words reason for an NSIS or helper exit code.
pub fn explain_exit(code: u32) -> String {
    match code {
        0 => "Finished.".into(),
        1 => "Setup was cancelled before it finished.".into(),
        2 => "Setup stopped: Relay may still be running, or a newer Relay is installed.".into(),
        1223 => "Windows permission was declined, so nothing was changed.".into(),
        other => format!("Setup ended with exit code {other}."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(text: &str) -> Version {
        Version::parse(text).unwrap()
    }

    fn installed(version: &str, scope: Scope) -> Installed {
        Installed {
            version: version.into(),
            location: Some(r"C:\Program Files\Relay".into()),
            uninstall_string: Some(r#""C:\Program Files\Relay\uninstall.exe""#.into()),
            scope,
        }
    }

    #[test]
    fn versions_compare_numerically() {
        assert!(v("0.1.5") > v("0.1.4"));
        assert!(v("0.1.10") > v("0.1.9"));
        assert!(v("1.0.0") > v("0.99.99"));
        assert_eq!(v("0.1.5"), v("0.1.5"));
        assert_eq!(v("0.2").cmp(&v("0.2.0")), Ordering::Equal);
        assert_eq!(v("v0.1.5").cmp(&v("0.1.5")), Ordering::Equal);
        assert_eq!(v("0.1.5+abc").cmp(&v("0.1.5")), Ordering::Equal);
    }

    #[test]
    fn pre_releases_sort_before_the_release() {
        assert!(v("1.0.0-rc.1") < v("1.0.0"));
        assert!(v("1.0.0-alpha") < v("1.0.0-beta"));
        assert!(v("1.0.0-beta.2") < v("1.0.0-beta.11"));
        assert!(v("1.0.0-1") < v("1.0.0-alpha"));
        assert!(v("1.0.0-beta") < v("1.0.0-beta.1"));
    }

    #[test]
    fn junk_versions_do_not_parse() {
        for junk in ["", "abc", "1..2", "1.x", "1.0-", "v"] {
            assert_eq!(Version::parse(junk), None, "{junk:?}");
        }
    }

    #[test]
    fn nothing_installed_offers_install() {
        let p = plan(None, None, "0.1.5");
        assert_eq!(p.status, Status::NotInstalled);
        assert_eq!(p.actions, vec![Action::Install]);
        assert_eq!(p.installed, None);
    }

    #[test]
    fn older_offers_update_then_uninstall() {
        let p = plan(Some(installed("0.1.4", Scope::Machine)), None, "0.1.5");
        assert_eq!(p.status, Status::Older);
        assert_eq!(p.actions, vec![Action::Update, Action::Uninstall]);
    }

    #[test]
    fn same_offers_repair_then_uninstall() {
        let p = plan(Some(installed("0.1.5", Scope::Machine)), None, "0.1.5");
        assert_eq!(p.status, Status::Same);
        assert_eq!(p.actions, vec![Action::Repair, Action::Uninstall]);
    }

    #[test]
    fn newer_offers_only_uninstall() {
        let p = plan(Some(installed("0.2.0", Scope::Machine)), None, "0.1.5");
        assert_eq!(p.status, Status::Newer);
        assert_eq!(p.actions, vec![Action::Uninstall]);
    }

    #[test]
    fn per_user_010_alone_is_an_update() {
        let p = plan(None, Some(installed("0.1.0", Scope::User)), "0.1.5");
        assert_eq!(p.status, Status::Older);
        assert_eq!(p.installed.unwrap().scope, Scope::User);
    }

    #[test]
    fn per_machine_wins_over_a_leftover_per_user_copy() {
        let p = plan(
            Some(installed("0.1.5", Scope::Machine)),
            Some(installed("0.1.0", Scope::User)),
            "0.1.5",
        );
        assert_eq!(p.status, Status::Same);
        assert_eq!(p.installed.unwrap().scope, Scope::Machine);
    }

    #[test]
    fn unreadable_installed_version_is_treated_as_older() {
        let p = plan(Some(installed("", Scope::Machine)), None, "0.1.5");
        assert_eq!(p.status, Status::Older);
        assert_eq!(p.actions[0], Action::Update);
    }

    #[test]
    fn registry_strings_are_unquoted() {
        assert_eq!(
            unquote(r#""C:\Program Files\Relay""#),
            r"C:\Program Files\Relay"
        );
        assert_eq!(unquote(r"C:\relay\"), r"C:\relay");
        assert_eq!(
            uninstaller_path(r#""C:\Program Files\Relay\uninstall.exe""#).as_deref(),
            Some(r"C:\Program Files\Relay\uninstall.exe")
        );
        assert_eq!(
            uninstaller_path(r#""C:\relay\uninstall.exe" /S"#).as_deref(),
            Some(r"C:\relay\uninstall.exe")
        );
        assert_eq!(
            uninstaller_path(r"C:\relay\uninstall.exe /S").as_deref(),
            Some(r"C:\relay\uninstall.exe")
        );
        assert_eq!(uninstaller_path("  "), None);
        assert_eq!(
            parent_dir(r"C:\Program Files\Relay\uninstall.exe").as_deref(),
            Some(r"C:\Program Files\Relay")
        );
        assert_eq!(parent_dir("uninstall.exe"), None);
    }

    #[test]
    fn arguments_are_quoted_for_the_command_line() {
        assert_eq!(quote_arg("plain"), "plain");
        assert_eq!(quote_arg(""), r#""""#);
        assert_eq!(
            quote_arg(r"C:\Program Files\Relay"),
            r#""C:\Program Files\Relay""#
        );
        assert_eq!(quote_arg(r"C:\a b\"), r#""C:\a b\\""#);
        assert_eq!(quote_arg(r#"say "hi""#), r#""say \"hi\"""#);
    }

    #[test]
    fn actions_parse_from_the_frontend() {
        assert_eq!(Action::parse("install"), Some(Action::Install));
        assert_eq!(Action::parse("update"), Some(Action::Update));
        assert_eq!(Action::parse("repair"), Some(Action::Repair));
        assert_eq!(Action::parse("uninstall"), Some(Action::Uninstall));
        assert_eq!(Action::parse("format c:"), None);
        assert!(Action::Update.runs_payload());
        assert!(!Action::Uninstall.runs_payload());
    }

    #[test]
    fn exit_codes_are_explained() {
        assert!(explain_exit(1223).contains("declined"));
        assert!(explain_exit(2).contains("running"));
        assert!(explain_exit(42).contains("42"));
    }
}
