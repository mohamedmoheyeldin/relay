//! Chooses between the local model and the cloud CLI for a task.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteMode {
    /// Local model first for OS and automation work, cloud for the hard parts.
    #[default]
    Auto,
    LocalOnly,
    CloudOnly,
}

impl RouteMode {
    pub fn as_str(self) -> &'static str {
        match self {
            RouteMode::Auto => "auto",
            RouteMode::LocalOnly => "local_only",
            RouteMode::CloudOnly => "cloud_only",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "auto" => Some(RouteMode::Auto),
            "local_only" => Some(RouteMode::LocalOnly),
            "cloud_only" => Some(RouteMode::CloudOnly),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    Local,
    Cloud,
}

/// What the local model can do right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LocalAbility {
    pub ready: bool,
    pub vision: bool,
}

/// Work that needs the web, long reasoning, writing or coding goes to the cloud CLI.
const CLOUD_HINTS: [&str; 26] = [
    "http://",
    "https://",
    ".com",
    "website",
    "web site",
    "browser",
    "search for",
    "research",
    "apply",
    "job",
    "resume",
    "linkedin",
    "email",
    "gmail",
    "github",
    "pull request",
    "refactor",
    "debug",
    "implement",
    "analy",
    "summariz",
    "translate",
    "essay",
    "report",
    "write a ",
    "build me",
];

/// Work that only makes sense with eyes on the screen.
const VISUAL_HINTS: [&str; 7] = [
    "click",
    "screenshot",
    "on screen",
    "on the screen",
    "what do you see",
    "drag",
    "button",
];

pub fn decide(mode: RouteMode, task: &str, local: LocalAbility) -> Engine {
    match mode {
        RouteMode::CloudOnly => Engine::Cloud,
        RouteMode::LocalOnly => Engine::Local,
        RouteMode::Auto => {
            if !local.ready {
                return Engine::Cloud;
            }
            let lower = task.to_lowercase();
            if lower.chars().count() > 600 || CLOUD_HINTS.iter().any(|h| lower.contains(h)) {
                return Engine::Cloud;
            }
            if !local.vision && VISUAL_HINTS.iter().any(|h| lower.contains(h)) {
                return Engine::Cloud;
            }
            Engine::Local
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const READY: LocalAbility = LocalAbility {
        ready: true,
        vision: false,
    };
    const SEEING: LocalAbility = LocalAbility {
        ready: true,
        vision: true,
    };
    const OFF: LocalAbility = LocalAbility {
        ready: false,
        vision: false,
    };

    #[test]
    fn os_chores_stay_local() {
        for task in [
            "empty my downloads folder of installers older than 30 days",
            "what processes use the most memory",
            "rename all .txt files here to .md",
            "set the volume to 20 percent",
        ] {
            assert_eq!(
                decide(RouteMode::Auto, task, READY),
                Engine::Local,
                "{task}"
            );
        }
    }

    #[test]
    fn web_and_heavy_work_go_to_the_cloud() {
        for task in [
            "go to https://www.dice.com/ and search for fde jobs",
            "research the best laptops",
            "debug this stack trace",
            "write a cover letter",
        ] {
            assert_eq!(
                decide(RouteMode::Auto, task, SEEING),
                Engine::Cloud,
                "{task}"
            );
        }
    }

    #[test]
    fn long_tasks_go_to_the_cloud() {
        assert_eq!(
            decide(RouteMode::Auto, &"x".repeat(700), READY),
            Engine::Cloud
        );
    }

    #[test]
    fn screen_work_needs_a_model_that_can_see() {
        assert_eq!(
            decide(RouteMode::Auto, "click the OK button", READY),
            Engine::Cloud
        );
        assert_eq!(
            decide(RouteMode::Auto, "click the OK button", SEEING),
            Engine::Local
        );
    }

    #[test]
    fn no_local_model_means_cloud_in_auto_mode() {
        assert_eq!(decide(RouteMode::Auto, "list my files", OFF), Engine::Cloud);
    }

    #[test]
    fn explicit_modes_win() {
        assert_eq!(
            decide(RouteMode::CloudOnly, "list my files", READY),
            Engine::Cloud
        );
        assert_eq!(
            decide(RouteMode::LocalOnly, "research x", OFF),
            Engine::Local
        );
    }

    #[test]
    fn mode_names_round_trip() {
        for m in [RouteMode::Auto, RouteMode::LocalOnly, RouteMode::CloudOnly] {
            assert_eq!(RouteMode::parse(m.as_str()), Some(m));
        }
        assert_eq!(RouteMode::parse("?"), None);
    }
}
