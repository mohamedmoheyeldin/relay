//! Spotting real sensitive information, so the word "sensitive" is only shown when it is true.
//! A click or a file listing is not sensitive by itself; a password field or a key file is.

use crate::plan::PlanAction;

const WORDS: [&str; 22] = [
    "password",
    "passcode",
    "passphrase",
    "credit card",
    "card number",
    "cvv",
    "cvc",
    "security code",
    "ssn",
    "social security",
    "bank account",
    "routing number",
    "iban",
    "tax return",
    "medical record",
    "diagnosis",
    "private key",
    "secret key",
    "api key",
    "access token",
    "passport",
    "seed phrase",
];

const FILE_HINTS: [&str; 12] = [
    ".env",
    "id_rsa",
    "id_ed25519",
    ".pem",
    ".pfx",
    ".kdbx",
    "wallet",
    "passwords",
    "credentials",
    "secrets",
    "cookies",
    "login data",
];

/// Does this text (a field label, a file name, a request) mention real sensitive information?
pub fn text_is_sensitive(text: &str) -> bool {
    let t = text.to_lowercase();
    WORDS.iter().any(|w| t.contains(w)) || FILE_HINTS.iter().any(|w| t.contains(w))
}

/// Does this plan action touch sensitive information?
pub fn action_is_sensitive(action: &PlanAction) -> bool {
    match action {
        PlanAction::ReadTextFile { path, .. }
        | PlanAction::ListDir { path }
        | PlanAction::WriteTextFile { path, .. } => text_is_sensitive(path),
        PlanAction::MovePath { from, to } => text_is_sensitive(from) || text_is_sensitive(to),
        PlanAction::MoveToTrash { paths } => paths.iter().any(|p| text_is_sensitive(p)),
        PlanAction::FindFiles { query, .. } => text_is_sensitive(query),
        PlanAction::UiSetText { target, .. } | PlanAction::UiClick { target } => {
            [&target.name, &target.automation_id]
                .into_iter()
                .flatten()
                .any(|t| text_is_sensitive(t))
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::UiTarget;

    #[test]
    fn ordinary_things_are_not_sensitive() {
        for t in [
            "Easy Apply",
            "Downloads",
            "report.pdf",
            "Search",
            "Open Notepad",
        ] {
            assert!(!text_is_sensitive(t), "{t}");
        }
        assert!(!action_is_sensitive(&PlanAction::OpenApp {
            name: "Notepad".into()
        }));
        assert!(!action_is_sensitive(&PlanAction::ReadTextFile {
            path: r"C:\Users\me\Documents\notes.txt".into(),
            max_bytes: None
        }));
    }

    #[test]
    fn real_sensitive_information_is_recognised() {
        for t in [
            "Password",
            "Card number",
            "Social Security Number",
            "my passwords.kdbx",
            r"C:\Users\me\.ssh\id_rsa",
            ".env",
        ] {
            assert!(text_is_sensitive(t), "{t}");
        }
        let field = PlanAction::UiSetText {
            target: UiTarget {
                name: Some("Password".into()),
                ..UiTarget::default()
            },
            text: "x".into(),
        };
        assert!(action_is_sensitive(&field));
    }
}
