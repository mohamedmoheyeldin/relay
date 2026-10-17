//! Chooses how a task is carried out: natively, with the local model, or with a cloud provider.

use serde::{Deserialize, Serialize};

use crate::permissions::PrivacyMode;
use crate::plan::{PlanAction, PlanStep};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    Claude,
    Codex,
}

impl ProviderKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ProviderKind::Claude => "claude",
            ProviderKind::Codex => "codex",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ProviderKind::Claude => "Claude Code",
            ProviderKind::Codex => "Codex",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "claude" => Some(Self::Claude),
            "codex" => Some(Self::Codex),
            _ => None,
        }
    }
}

/// How the task will be carried out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Strategy {
    /// A deterministic native operation; no AI reasoning needed.
    Native(Box<PlanStep>),
    /// The local model plans (from a structured snapshot of the desktop), Rust executes.
    LocalPlan,
    /// A cloud provider returns a structured plan, Rust validates and executes.
    CloudPlan(ProviderKind),
    /// The cloud provider works autonomously with Relay's tools (open-ended tasks, code).
    CloudAgent(ProviderKind),
    /// A question or request for text/code: answered in words by the local model, nothing is run.
    ChatLocal,
    /// A question answered by a cloud provider (no tools, nothing is run).
    ChatCloud(ProviderKind),
    /// Work in a web browser, one observed step at a time, decided by the local model.
    WebLocal,
    /// Work in a web browser, one observed step at a time, decided by a cloud provider.
    WebCloud(ProviderKind),
    /// Nothing can handle this under the current settings.
    Unavailable(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Route {
    pub strategy: Strategy,
    pub intent: Intent,
    /// One sentence for the timeline: why this path was chosen.
    pub reason: String,
}

/// What is available right now.
#[derive(Debug, Clone, Copy)]
pub struct Abilities {
    pub local_ready: bool,
    pub local_sees_screen: bool,
    /// The provider the owner chose as default, when it is installed and signed in.
    pub provider: Option<ProviderKind>,
    /// The other provider, when usable (fallback if the default fails).
    pub fallback: Option<ProviderKind>,
    /// Browser control (an MCP browser server) can start on this PC.
    pub browser_ready: bool,
}

/// Words that mark a task as sensitive: it is handled locally unless the owner says otherwise.
const SENSITIVE: [&str; 14] = [
    "password",
    "passcode",
    "credit card",
    "card number",
    "ssn",
    "social security",
    "bank",
    "tax return",
    "medical",
    "diagnosis",
    "private key",
    "secret",
    "api key",
    "token",
];

/// Words that mark work that needs a stronger model.
const HEAVY: [&str; 22] = [
    "write code",
    "write a script",
    "refactor",
    "debug",
    "implement",
    "algorithm",
    "analyze",
    "analyse",
    "summarize",
    "summarise",
    "research",
    "compare",
    "translate",
    "draft",
    "essay",
    "explain why",
    "figure out",
    "troubleshoot",
    "automate",
    "workflow",
    "multiple apps",
    "every ",
];

pub fn is_sensitive(task: &str) -> bool {
    let t = task.to_lowercase();
    SENSITIVE.iter().any(|w| t.contains(w))
}

pub fn is_heavy(task: &str) -> bool {
    let t = task.to_lowercase();
    t.chars().count() > 350 || HEAVY.iter().any(|w| t.contains(w))
}

fn clean(s: &str) -> String {
    s.trim()
        .trim_matches(|c| c == '"' || c == '\'' || c == '.')
        .trim()
        .to_string()
}

/// Recognise trivial requests that need no AI at all ("open notepad", "open https://…").
pub fn simple_intent(task: &str) -> Option<PlanStep> {
    let t = task.trim();
    let lower = t.to_lowercase();
    let rest = ["open ", "launch ", "start "]
        .iter()
        .find_map(|p| lower.strip_prefix(p).map(|_| clean(&t[p.len()..])))?;
    if rest.is_empty() || rest.split_whitespace().count() > 4 || rest.contains([',', ';', '&', '|'])
    {
        return None;
    }
    let lower_rest = rest.to_lowercase();
    let action = if lower_rest.starts_with("http://") || lower_rest.starts_with("https://") {
        PlanAction::OpenUrl { url: rest.clone() }
    } else if rest.contains(['\\', '/']) {
        return None; // paths and folders go through the normal planners
    } else {
        PlanAction::OpenApp { name: rest.clone() }
    };
    Some(PlanStep {
        id: "s1".into(),
        title: action.describe(),
        action,
        verify: None,
    })
}

/// What the owner wants from this message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Intent {
    /// A question, explanation or request for text or code. Nothing should be run.
    Chat,
    /// Do something on this PC.
    Action,
    /// Do something on a website.
    Web,
}

impl Intent {
    pub fn as_str(self) -> &'static str {
        match self {
            Intent::Chat => "chat",
            Intent::Action => "action",
            Intent::Web => "web",
        }
    }
}

/// Verbs that start a command. A message that begins with one is a request to act, not a question.
const COMMAND_VERBS: [&str; 30] = [
    "open",
    "launch",
    "start",
    "go to",
    "goto",
    "visit",
    "browse",
    "navigate",
    "apply",
    "click",
    "type",
    "press",
    "find",
    "move",
    "delete",
    "remove",
    "rename",
    "copy",
    "create",
    "make a folder",
    "tidy",
    "clean",
    "organize",
    "organise",
    "sort",
    "download",
    "upload",
    "send",
    "book",
    "buy",
];

/// Phrases that show the owner wants to be told something, even if the message also names a site.
const CHAT_MARKERS: [&str; 22] = [
    "example of",
    "examples of",
    "an example",
    "explain",
    "how do i",
    "how to",
    "how does",
    "how can i",
    "what is",
    "what's",
    "what are",
    "difference between",
    "code for",
    "write code",
    "snippet",
    "tutorial",
    "why does",
    "why is",
    "can you tell me",
    "tell me about",
    "what does",
    "script that",
];

fn has_url_or_site(lower: &str) -> bool {
    if lower.contains("http://") || lower.contains("https://") || lower.contains("www.") {
        return true;
    }
    lower.split_whitespace().any(|w| {
        let w = w.trim_matches(|c: char| !c.is_alphanumeric() && c != '.' && c != '-');
        let Some((name, tld)) = w.rsplit_once('.') else {
            return false;
        };
        !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.')
            && [
                "com", "org", "net", "io", "gov", "edu", "co", "dev", "ai", "app",
            ]
            .contains(&tld)
    })
}

const SITE_WORDS: [&str; 12] = [
    "website",
    "web site",
    "web page",
    "webpage",
    "in the browser",
    "on the web",
    "online",
    "linkedin",
    "indeed",
    "dice",
    "amazon",
    "google",
];

/// Decide whether a message is a question, a PC action or a website task.
pub fn classify(task: &str) -> Intent {
    let lower = task.trim().to_lowercase();
    let stripped = [
        "please ",
        "can you ",
        "could you ",
        "would you ",
        "i want you to ",
        "i need you to ",
        "just ",
    ]
    .iter()
    .fold(lower.as_str(), |acc, p| acc.strip_prefix(p).unwrap_or(acc));
    let starts_command = COMMAND_VERBS.iter().any(|v| {
        stripped == *v
            || stripped
                .strip_prefix(v)
                .is_some_and(|rest| rest.starts_with([' ', ':']))
    });
    let chat_marker = CHAT_MARKERS.iter().any(|m| lower.contains(m));
    let question = lower.ends_with('?')
        || [
            "what ",
            "why ",
            "how ",
            "which ",
            "who ",
            "when ",
            "where ",
            "is ",
            "are ",
            "does ",
            "do ",
            "should ",
            "explain",
            "write ",
            "give me",
            "show me an",
            "tell me",
        ]
        .iter()
        .any(|q| stripped.starts_with(q));
    if !starts_command && (chat_marker || question) {
        return Intent::Chat;
    }
    if has_url_or_site(&lower) || SITE_WORDS.iter().any(|w| lower.contains(w)) {
        return Intent::Web;
    }
    Intent::Action
}

/// Decide the strategy for a task.
fn route_action(task: &str, privacy: PrivacyMode, a: Abilities) -> Route {
    if let Some(step) = simple_intent(task) {
        return Route {
            intent: Intent::Action,
            reason: "A simple request that Relay can do directly, no AI needed.".into(),
            strategy: Strategy::Native(Box::new(step)),
        };
    }
    let sensitive = is_sensitive(task);
    let heavy = is_heavy(task);
    let local = a.local_ready;
    let provider = a.provider;

    match privacy {
        PrivacyMode::LocalOnly => {
            if local {
                Route {
                    intent: Intent::Action,
                    strategy: Strategy::LocalPlan,
                    reason: "Local only: nothing leaves this PC.".into(),
                }
            } else {
                Route {
                    intent: Intent::Action,
                    strategy: Strategy::Unavailable(
                        "Local only mode is on, but the local AI is not ready yet.".into(),
                    ),
                    reason: "Local only mode needs the local AI.".into(),
                }
            }
        }
        PrivacyMode::HybridAuto => {
            if sensitive && local {
                return Route {
                    intent: Intent::Action,
                    strategy: Strategy::LocalPlan,
                    reason: "This looks sensitive, so it stays on this PC.".into(),
                };
            }
            match (local, heavy, provider) {
                (true, false, _) => Route {
                    intent: Intent::Action,
                    strategy: Strategy::LocalPlan,
                    reason: "A routine task: the local AI plans it.".into(),
                },
                (_, true, Some(p)) | (false, _, Some(p)) => Route {
                    intent: Intent::Action,
                    strategy: Strategy::CloudPlan(p),
                    reason: if heavy {
                        format!("This needs deeper reasoning: {} plans it, Relay runs it here.", p.label())
                    } else {
                        format!("The local AI is not ready: {} plans it.", p.label())
                    },
                },
                (true, true, None) => Route {
                    intent: Intent::Action,
                    strategy: Strategy::LocalPlan,
                    reason: "No cloud provider is available, so the local AI plans it.".into(),
                },
                (false, _, None) => Route {
                    intent: Intent::Action,
                    strategy: Strategy::Unavailable(
                        "No AI is available: set up the local AI or sign in to Claude Code or Codex.".into(),
                    ),
                    reason: "Nothing available to plan this task.".into(),
                },
            }
        }
        PrivacyMode::CloudPreferred => match provider {
            Some(p) if !sensitive => Route {
                intent: Intent::Action,
                strategy: Strategy::CloudPlan(p),
                reason: format!(
                    "Cloud preferred: {} plans it, Relay runs it here.",
                    p.label()
                ),
            },
            _ if local => Route {
                intent: Intent::Action,
                strategy: Strategy::LocalPlan,
                reason: if sensitive {
                    "This looks sensitive, so it stays on this PC.".into()
                } else {
                    "No cloud provider is available, so the local AI plans it.".into()
                },
            },
            Some(p) => Route {
                intent: Intent::Action,
                strategy: Strategy::CloudPlan(p),
                reason: format!("{} plans it.", p.label()),
            },
            None => Route {
                intent: Intent::Action,
                strategy: Strategy::Unavailable("No AI is available right now.".into()),
                reason: "Nothing available to plan this task.".into(),
            },
        },
    }
}

/// Decide the strategy for a task: answer it, do it on this PC, or do it in a browser.
pub fn route(task: &str, privacy: PrivacyMode, a: Abilities) -> Route {
    if simple_intent(task).is_some() {
        return route_action(task, privacy, a);
    }
    let intent = classify(task);
    if intent == Intent::Action {
        return route_action(task, privacy, a);
    }
    let mut base = route_action(task, privacy, a);
    base.intent = intent;
    let codeish = {
        let t = task.to_lowercase();
        [
            "code",
            "script",
            "function",
            "snippet",
            "playwright",
            "regex",
            "sql",
            "api",
            "class ",
        ]
        .iter()
        .any(|w| t.contains(w))
    };
    let cloud_for_code = intent == Intent::Chat
        && privacy == PrivacyMode::HybridAuto
        && codeish
        && !is_sensitive(task)
        && a.provider.is_some();
    base.strategy = match (intent, base.strategy) {
        (_, Strategy::Unavailable(m)) => Strategy::Unavailable(m),
        (Intent::Chat, _) if cloud_for_code => {
            base.reason = "A coding question: the cloud AI answers it, nothing is run.".into();
            match a.provider {
                Some(p) => Strategy::ChatCloud(p),
                None => Strategy::ChatLocal,
            }
        }
        (Intent::Chat, Strategy::CloudPlan(p) | Strategy::CloudAgent(p)) => Strategy::ChatCloud(p),
        (Intent::Chat, _) => Strategy::ChatLocal,
        (Intent::Web, _) if !a.browser_ready => Strategy::Unavailable(
            "Browser control is not available on this PC yet. It needs Node.js; see Settings, AI & Privacy.".into(),
        ),
        (Intent::Web, Strategy::CloudPlan(p) | Strategy::CloudAgent(p)) => Strategy::WebCloud(p),
        // Reading unfamiliar websites goes better with the stronger cloud model when the owner allows it.
        (Intent::Web, Strategy::LocalPlan)
            if privacy == PrivacyMode::HybridAuto && !is_sensitive(task) && a.provider.is_some() =>
        {
            match a.provider {
                Some(p) => Strategy::WebCloud(p),
                None => Strategy::WebLocal,
            }
        }
        (Intent::Web, _) => Strategy::WebLocal,
        (_, other) => other,
    };
    if intent == Intent::Chat && !cloud_for_code {
        base.reason = match &base.strategy {
            Strategy::ChatLocal => "A question: the local AI answers it, nothing is run.".into(),
            Strategy::ChatCloud(p) => {
                format!("A question: {} answers it, nothing is run.", p.label())
            }
            _ => base.reason,
        };
    }
    if intent == Intent::Web {
        base.reason = match &base.strategy {
            Strategy::WebLocal => {
                "A website task: the local AI drives the browser, step by step.".into()
            }
            Strategy::WebCloud(p) => format!(
                "A website task: {} drives the browser, step by step.",
                p.label()
            ),
            _ => base.reason,
        };
    }
    base
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOTH: Abilities = Abilities {
        local_ready: true,
        local_sees_screen: false,
        provider: Some(ProviderKind::Claude),
        fallback: Some(ProviderKind::Codex),
        browser_ready: true,
    };
    const LOCAL_ONLY_ABILITY: Abilities = Abilities {
        provider: None,
        fallback: None,
        ..BOTH
    };
    const CLOUD_ONLY_ABILITY: Abilities = Abilities {
        local_ready: false,
        ..BOTH
    };
    const NOTHING: Abilities = Abilities {
        local_ready: false,
        provider: None,
        fallback: None,
        ..BOTH
    };

    /// The action-routing rules (questions and website tasks have their own tests below).
    fn strat(task: &str, mode: PrivacyMode, a: Abilities) -> Strategy {
        route_action(task, mode, a).strategy
    }

    #[test]
    fn trivial_requests_are_native_in_every_mode() {
        for mode in [
            PrivacyMode::LocalOnly,
            PrivacyMode::HybridAuto,
            PrivacyMode::CloudPreferred,
        ] {
            assert!(
                matches!(strat("open notepad", mode, NOTHING), Strategy::Native(_)),
                "{mode:?}"
            );
        }
        match strat("Open https://example.com", PrivacyMode::HybridAuto, BOTH) {
            Strategy::Native(step) => assert!(matches!(step.action, PlanAction::OpenUrl { .. })),
            other => panic!("{other:?}"),
        }
        match strat(
            "launch \"Visual Studio Code\"",
            PrivacyMode::HybridAuto,
            BOTH,
        ) {
            Strategy::Native(step) => assert!(
                matches!(&step.action, PlanAction::OpenApp { name } if name == "Visual Studio Code")
            ),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn open_requests_with_more_to_them_are_not_trivial() {
        for task in [
            "open notepad and type a poem about spring",
            "open C:\\Users\\me\\Documents",
            "open the file, then rename it",
            "open ",
        ] {
            assert!(simple_intent(task).is_none(), "{task}");
        }
    }

    #[test]
    fn local_only_never_uses_the_cloud() {
        assert_eq!(
            strat("clean up my downloads", PrivacyMode::LocalOnly, BOTH),
            Strategy::LocalPlan
        );
        assert_eq!(
            strat(
                "debug this crash and write code to fix it",
                PrivacyMode::LocalOnly,
                BOTH
            ),
            Strategy::LocalPlan
        );
        assert!(matches!(
            strat(
                "clean up my downloads",
                PrivacyMode::LocalOnly,
                CLOUD_ONLY_ABILITY
            ),
            Strategy::Unavailable(_)
        ));
    }

    #[test]
    fn hybrid_keeps_routine_and_sensitive_work_local() {
        assert_eq!(
            strat(
                "rename the files in my downloads folder",
                PrivacyMode::HybridAuto,
                BOTH
            ),
            Strategy::LocalPlan
        );
        assert_eq!(
            strat(
                "find my bank statements and my password file",
                PrivacyMode::HybridAuto,
                BOTH
            ),
            Strategy::LocalPlan
        );
    }

    #[test]
    fn hybrid_escalates_heavy_work_to_the_default_provider() {
        assert_eq!(
            strat(
                "debug why my build fails and write code to fix it",
                PrivacyMode::HybridAuto,
                BOTH
            ),
            Strategy::CloudPlan(ProviderKind::Claude)
        );
        let codex = Abilities {
            provider: Some(ProviderKind::Codex),
            fallback: Some(ProviderKind::Claude),
            ..BOTH
        };
        assert_eq!(
            strat(
                "research and compare three laptops",
                PrivacyMode::HybridAuto,
                codex
            ),
            Strategy::CloudPlan(ProviderKind::Codex)
        );
        assert_eq!(
            strat("x".repeat(400).as_str(), PrivacyMode::HybridAuto, BOTH),
            Strategy::CloudPlan(ProviderKind::Claude)
        );
    }

    #[test]
    fn hybrid_falls_back_when_one_side_is_missing() {
        assert_eq!(
            strat(
                "write a script to rename files",
                PrivacyMode::HybridAuto,
                LOCAL_ONLY_ABILITY
            ),
            Strategy::LocalPlan
        );
        assert_eq!(
            strat(
                "tidy my desktop",
                PrivacyMode::HybridAuto,
                CLOUD_ONLY_ABILITY
            ),
            Strategy::CloudPlan(ProviderKind::Claude)
        );
        assert!(matches!(
            strat("tidy my desktop", PrivacyMode::HybridAuto, NOTHING),
            Strategy::Unavailable(_)
        ));
    }

    #[test]
    fn cloud_preferred_uses_the_provider_except_for_sensitive_tasks() {
        assert_eq!(
            strat("tidy my desktop", PrivacyMode::CloudPreferred, BOTH),
            Strategy::CloudPlan(ProviderKind::Claude)
        );
        assert_eq!(
            strat(
                "what is in my password manager export",
                PrivacyMode::CloudPreferred,
                BOTH
            ),
            Strategy::LocalPlan
        );
        assert_eq!(
            strat(
                "tidy my desktop",
                PrivacyMode::CloudPreferred,
                LOCAL_ONLY_ABILITY
            ),
            Strategy::LocalPlan
        );
        assert!(matches!(
            strat("tidy my desktop", PrivacyMode::CloudPreferred, NOTHING),
            Strategy::Unavailable(_)
        ));
    }

    #[test]
    fn every_route_explains_itself() {
        for task in ["open notepad", "tidy my desktop", "debug my build"] {
            assert!(!route(task, PrivacyMode::HybridAuto, BOTH).reason.is_empty());
        }
    }

    #[test]
    fn providers_round_trip() {
        for p in [ProviderKind::Claude, ProviderKind::Codex] {
            assert_eq!(ProviderKind::parse(p.as_str()), Some(p));
            assert!(!p.label().is_empty());
        }
        assert_eq!(ProviderKind::parse("gpt"), None);
    }

    #[test]
    fn questions_are_chat_and_commands_are_not() {
        for q in [
            "example of playwright code for testing amazon search?",
            "how do I list files in powershell",
            "explain what a mutex is",
            "what is the difference between rust and go",
            "write a python function that reverses a string",
        ] {
            assert_eq!(classify(q), Intent::Chat, "{q}");
        }
        for (t, want) in [
            (
                "go to https://www.dice.com/ and apply for fde jobs with easy apply",
                Intent::Web,
            ),
            ("apply for jobs on dice", Intent::Web),
            ("search amazon for a usb hub", Intent::Web),
            ("tidy up my downloads folder", Intent::Action),
            ("find what is using my disk space", Intent::Action),
            ("can you open notepad?", Intent::Action),
            ("move my pdfs into a folder", Intent::Action),
        ] {
            assert_eq!(classify(t), want, "{t}");
        }
    }

    #[test]
    fn chat_never_becomes_a_plan_and_web_needs_the_browser() {
        let chat = route(
            "example of playwright code for testing amazon search?",
            PrivacyMode::HybridAuto,
            BOTH,
        );
        assert!(matches!(
            chat.strategy,
            Strategy::ChatCloud(ProviderKind::Claude)
        ));
        assert_eq!(chat.intent, Intent::Chat);
        let local = route("what is a mutex?", PrivacyMode::LocalOnly, BOTH);
        assert!(matches!(local.strategy, Strategy::ChatLocal));
        let web = route(
            "go to https://www.dice.com/ and apply for fde jobs",
            PrivacyMode::HybridAuto,
            BOTH,
        );
        assert_eq!(web.intent, Intent::Web);
        assert!(matches!(
            web.strategy,
            Strategy::WebLocal | Strategy::WebCloud(_)
        ));
        let no_browser = Abilities {
            browser_ready: false,
            ..BOTH
        };
        let blocked = route(
            "go to https://www.dice.com/ and apply",
            PrivacyMode::HybridAuto,
            no_browser,
        );
        assert!(matches!(blocked.strategy, Strategy::Unavailable(ref m) if m.contains("Browser")));
        let none = route("go to dice.com and apply", PrivacyMode::LocalOnly, NOTHING);
        assert!(matches!(none.strategy, Strategy::Unavailable(_)));
    }
}
