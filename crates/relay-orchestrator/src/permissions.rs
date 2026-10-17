//! What AI-proposed actions are allowed to do, and what needs the owner's say-so.

use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::plan::PlanAction;

/// How task context may be used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrivacyMode {
    /// Nothing about the task or the desktop ever goes to a cloud provider.
    LocalOnly,
    /// Route by capability, complexity and sensitivity.
    #[default]
    HybridAuto,
    /// Prefer the cloud provider for reasoning; execution stays local.
    CloudPreferred,
}

impl PrivacyMode {
    pub fn as_str(self) -> &'static str {
        match self {
            PrivacyMode::LocalOnly => "local_only",
            PrivacyMode::HybridAuto => "hybrid_auto",
            PrivacyMode::CloudPreferred => "cloud_preferred",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "local_only" => Some(Self::LocalOnly),
            "hybrid_auto" => Some(Self::HybridAuto),
            "cloud_preferred" => Some(Self::CloudPreferred),
            _ => None,
        }
    }
}

/// When the owner is asked first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfirmLevel {
    /// Never ask (unattended; not recommended).
    Off,
    /// Ask before destructive actions and before reading sensitive data.
    #[default]
    Destructive,
    /// Ask before every action that changes anything.
    Always,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Risk {
    /// Reads nothing private and changes nothing important.
    Safe,
    /// Reads personal content or changes things reversibly.
    Sensitive,
    /// Deletes, overwrites or moves data, or types into other apps.
    Destructive,
}

/// What categories of local context may be put in a cloud prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SharePolicy {
    pub window_titles: bool,
    pub ui_tree: bool,
    pub file_names: bool,
    pub file_contents: bool,
    pub screenshots: bool,
    /// Text of web pages the browser is working on (needed when a cloud AI drives the browser).
    #[serde(default = "yes")]
    pub web_pages: bool,
}

fn yes() -> bool {
    true
}

impl Default for SharePolicy {
    /// Minimum by default: titles of open windows only.
    fn default() -> Self {
        Self {
            window_titles: true,
            ui_tree: false,
            file_names: false,
            file_contents: false,
            screenshots: false,
            web_pages: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Policy {
    pub privacy: PrivacyMode,
    pub confirm: ConfirmLevel,
    pub share: SharePolicy,
    /// Folders the AI may touch. Anything outside is refused.
    pub allowed_roots: Vec<PathBuf>,
    /// Folders that are never touched, even inside an allowed root.
    pub protected: Vec<PathBuf>,
    /// Tool names that are switched off.
    pub disabled_tools: HashSet<String>,
}

impl Policy {
    pub fn new(allowed_roots: Vec<PathBuf>, protected: Vec<PathBuf>) -> Self {
        Self {
            privacy: PrivacyMode::default(),
            confirm: ConfirmLevel::default(),
            share: SharePolicy::default(),
            allowed_roots,
            protected,
            disabled_tools: HashSet::new(),
        }
    }
}

/// Result of checking one action against the policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Go ahead.
    Allow,
    /// Ask the owner first.
    Confirm { risk: Risk, reason: String },
    /// Never.
    Deny(String),
}

/// Resolve `.` and `..` without touching the disk, and make the path absolute-looking.
pub fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

fn lower(p: &Path) -> String {
    p.to_string_lossy().replace('/', "\\").to_lowercase()
}

/// Whether `path` is inside `root` (case-insensitive, on component boundaries).
pub fn is_within(path: &Path, root: &Path) -> bool {
    let (p, r) = (lower(&normalize(path)), lower(&normalize(root)));
    let r = r.trim_end_matches('\\');
    p == r || p.starts_with(&format!("{r}\\"))
}

impl Policy {
    fn path_verdict(&self, path: &str) -> Result<(), String> {
        let p = PathBuf::from(path);
        if !p.is_absolute() {
            return Err(format!("{path} is not an absolute path"));
        }
        if let Some(bad) = self.protected.iter().find(|r| is_within(&p, r)) {
            return Err(format!(
                "{path} is inside a protected folder ({})",
                bad.display()
            ));
        }
        if !self.allowed_roots.iter().any(|r| is_within(&p, r)) {
            return Err(format!("{path} is outside the folders Relay may use"));
        }
        Ok(())
    }

    /// The risk class of an action.
    pub fn risk_of(action: &PlanAction) -> Risk {
        match action {
            PlanAction::OpenApp { .. }
            | PlanAction::OpenUrl { .. }
            | PlanAction::FocusWindow { .. }
            | PlanAction::FindFiles { .. }
            | PlanAction::ListDir { .. }
            | PlanAction::DiskUsage { .. }
            | PlanAction::WaitFor { .. }
            | PlanAction::PressKeys { .. }
            | PlanAction::CreateDir { .. } => Risk::Safe,
            PlanAction::ReadTextFile { .. }
            | PlanAction::UiClick { .. }
            | PlanAction::UiSetText { .. }
            | PlanAction::TypeText { .. } => Risk::Sensitive,
            PlanAction::MovePath { .. }
            | PlanAction::WriteTextFile { .. }
            | PlanAction::MoveToTrash { .. } => Risk::Destructive,
        }
    }

    /// Check an action: allowed, needs confirmation, or refused.
    pub fn check(&self, action: &PlanAction) -> Verdict {
        if self.disabled_tools.contains(action.tool()) {
            return Verdict::Deny(format!("the {} tool is switched off", action.tool()));
        }
        // Path boundaries apply to every file action, whatever the confirmation level.
        let paths: Vec<&str> = match action {
            PlanAction::ListDir { path }
            | PlanAction::ReadTextFile { path, .. }
            | PlanAction::CreateDir { path }
            | PlanAction::WriteTextFile { path, .. } => vec![path.as_str()],
            PlanAction::MovePath { from, to } => vec![from.as_str(), to.as_str()],
            PlanAction::MoveToTrash { paths } => paths.iter().map(String::as_str).collect(),
            PlanAction::FindFiles {
                root: Some(root), ..
            } => vec![root.as_str()],
            PlanAction::DiskUsage {
                path: Some(path), ..
            } => vec![path.as_str()],
            _ => Vec::new(),
        };
        for p in paths {
            if let Err(why) = self.path_verdict(p) {
                return Verdict::Deny(why);
            }
        }
        let risk = Self::risk_of(action);
        let confirm = match (self.confirm, risk) {
            (ConfirmLevel::Off, _) => false,
            (ConfirmLevel::Always, Risk::Safe) => false,
            (ConfirmLevel::Always, _) => true,
            (ConfirmLevel::Destructive, Risk::Destructive) => true,
            // Only real sensitive information (key files, password stores, ...) needs a yes at this level.
            (ConfirmLevel::Destructive, Risk::Sensitive) => {
                crate::sensitivity::action_is_sensitive(action)
            }
            (ConfirmLevel::Destructive, Risk::Safe) => false,
        };
        if confirm {
            Verdict::Confirm {
                risk,
                reason: match risk {
                    Risk::Destructive => {
                        "This changes or removes data and may not be easy to undo.".into()
                    }
                    Risk::Sensitive => {
                        "This touches sensitive information (passwords, keys or ID numbers).".into()
                    }
                    Risk::Safe => "Confirmation is required for every action.".into(),
                },
            }
        } else {
            Verdict::Allow
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::UiTarget;

    fn policy() -> Policy {
        Policy::new(
            vec![
                PathBuf::from(r"C:\Users\me\Documents"),
                PathBuf::from(r"C:\Users\me\Downloads"),
            ],
            vec![PathBuf::from(r"C:\Users\me\Documents\Secrets")],
        )
    }

    fn trash(p: &str) -> PlanAction {
        PlanAction::MoveToTrash {
            paths: vec![p.into()],
        }
    }

    #[test]
    fn paths_outside_the_allowed_roots_are_refused() {
        let p = policy();
        for path in [
            r"C:\Windows\System32\cmd.exe",
            r"C:\Users\me\AppData\Roaming\x",
            r"C:\Users\me\Documents\..\..\other\x",
            r"C:\Users\me\DocumentsEvil\x",
            "relative\\path",
        ] {
            assert!(matches!(p.check(&trash(path)), Verdict::Deny(_)), "{path}");
        }
    }

    #[test]
    fn traversal_cannot_escape_a_root() {
        let p = policy();
        let sneaky = r"C:\Users\me\Documents\sub\..\..\..\Windows\x";
        assert!(matches!(
            p.check(&PlanAction::ListDir {
                path: sneaky.into()
            }),
            Verdict::Deny(_)
        ));
        assert_eq!(
            p.check(&PlanAction::ListDir {
                path: r"c:\USERS\me\documents\sub\..\notes".into()
            }),
            Verdict::Allow,
            "case-insensitive and normalised"
        );
    }

    #[test]
    fn protected_folders_win_over_allowed_roots() {
        let p = policy();
        let v = p.check(&PlanAction::ReadTextFile {
            path: r"C:\Users\me\Documents\Secrets\keys.txt".into(),
            max_bytes: None,
        });
        assert!(matches!(v, Verdict::Deny(why) if why.contains("protected")));
    }

    #[test]
    fn destructive_actions_ask_by_default_and_reads_of_files_ask() {
        let p = policy();
        assert!(matches!(
            p.check(&trash(r"C:\Users\me\Downloads\old.zip")),
            Verdict::Confirm {
                risk: Risk::Destructive,
                ..
            }
        ));
        assert!(matches!(
            p.check(&PlanAction::WriteTextFile {
                path: r"C:\Users\me\Documents\a.txt".into(),
                content: "x".into(),
                overwrite: true
            }),
            Verdict::Confirm { .. }
        ));
        // Reading an ordinary file does not ask; reading a password store does.
        assert_eq!(
            p.check(&PlanAction::ReadTextFile {
                path: r"C:\Users\me\Documents\a.txt".into(),
                max_bytes: None
            }),
            Verdict::Allow
        );
        assert!(matches!(
            p.check(&PlanAction::ReadTextFile {
                path: r"C:\Users\me\Documents\passwords.txt".into(),
                max_bytes: None
            }),
            Verdict::Confirm {
                risk: Risk::Sensitive,
                ..
            }
        ));
        assert_eq!(
            p.check(&PlanAction::OpenApp {
                name: "Notepad".into()
            }),
            Verdict::Allow
        );
        assert_eq!(
            p.check(&PlanAction::TypeText { text: "hi".into() }),
            Verdict::Allow
        );
        assert_eq!(
            p.check(&PlanAction::UiClick {
                target: UiTarget {
                    name: Some("Save".into()),
                    ..UiTarget::default()
                }
            }),
            Verdict::Allow
        );
    }

    #[test]
    fn confirmation_levels() {
        let mut p = policy();
        p.confirm = ConfirmLevel::Off;
        assert_eq!(
            p.check(&trash(r"C:\Users\me\Downloads\old.zip")),
            Verdict::Allow
        );
        // Boundaries still apply with confirmations off.
        assert!(matches!(p.check(&trash(r"C:\Windows\x")), Verdict::Deny(_)));
        p.confirm = ConfirmLevel::Always;
        assert!(matches!(
            p.check(&PlanAction::TypeText { text: "hi".into() }),
            Verdict::Confirm { .. }
        ));
        assert_eq!(
            p.check(&PlanAction::OpenApp {
                name: "Notepad".into()
            }),
            Verdict::Allow
        );
    }

    #[test]
    fn disabled_tools_are_refused() {
        let mut p = policy();
        p.disabled_tools.insert("move_to_trash".into());
        assert!(
            matches!(p.check(&trash(r"C:\Users\me\Downloads\x")), Verdict::Deny(why) if why.contains("switched off"))
        );
    }

    #[test]
    fn find_files_root_is_checked_and_defaults_are_minimal() {
        let p = policy();
        assert!(matches!(
            p.check(&PlanAction::FindFiles {
                query: "a".into(),
                root: Some(r"C:\Windows".into()),
                max_results: None
            }),
            Verdict::Deny(_)
        ));
        let s = SharePolicy::default();
        assert!(
            s.window_titles && !s.ui_tree && !s.file_names && !s.file_contents && !s.screenshots
        );
    }

    #[test]
    fn privacy_modes_round_trip() {
        for m in [
            PrivacyMode::LocalOnly,
            PrivacyMode::HybridAuto,
            PrivacyMode::CloudPreferred,
        ] {
            assert_eq!(PrivacyMode::parse(m.as_str()), Some(m));
        }
        assert_eq!(PrivacyMode::parse("x"), None);
    }

    #[test]
    fn normalisation_and_containment() {
        assert_eq!(
            normalize(Path::new(r"C:\a\b\..\c\.")),
            PathBuf::from(r"C:\a\c")
        );
        assert!(is_within(Path::new(r"C:\A\b"), Path::new(r"c:\a")));
        assert!(!is_within(Path::new(r"C:\ab"), Path::new(r"C:\a")));
        assert!(is_within(Path::new(r"C:\a"), Path::new(r"C:\a\")));
    }
}
