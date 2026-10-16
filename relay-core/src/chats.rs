//! Saved chats: one JSON file per conversation in `%LOCALAPPDATA%\Relay\chats`, so
//! the sidebar can list them and the owner can go back to one. The chat screen owns
//! what a conversation shows (`items`); Relay adds the Claude session it resumes.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Chat {
    pub id: String,
    pub title: String,
    /// Milliseconds since 1970, set on every save.
    #[serde(default)]
    pub updated: u64,
    /// The Claude Code session this conversation continues, if Claude answered in it.
    #[serde(default)]
    pub session: Option<String>,
    /// What the chat screen shows, as it sent it.
    #[serde(default)]
    pub items: Value,
}

/// A chat in the sidebar list.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Summary {
    pub id: String,
    pub title: String,
    pub updated: u64,
}

pub fn dir() -> PathBuf {
    crate::paths::data_dir().join("chats")
}

fn file(dir: &Path, id: &str) -> Result<PathBuf> {
    ensure!(
        !id.is_empty()
            && id.len() <= 64
            && id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
        "not a chat id: {id}"
    );
    Ok(dir.join(format!("{id}.json")))
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Every saved chat, newest first. Unreadable files are skipped.
pub fn list(dir: &Path) -> Vec<Summary> {
    let mut chats: Vec<Summary> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
                .filter_map(|e| std::fs::read_to_string(e.path()).ok())
                .filter_map(|t| serde_json::from_str::<Chat>(&t).ok())
                .map(|c| Summary {
                    id: c.id,
                    title: c.title,
                    updated: c.updated,
                })
                .collect()
        })
        .unwrap_or_default();
    chats.sort_by_key(|c| std::cmp::Reverse(c.updated));
    chats
}

pub fn load(dir: &Path, id: &str) -> Result<Chat> {
    let path = file(dir, id)?;
    let text = std::fs::read_to_string(&path).context("this chat is no longer saved")?;
    Ok(serde_json::from_str(&text)?)
}

/// Write a chat (through a temporary file, so a crash never leaves half of one).
pub fn save(dir: &Path, mut chat: Chat) -> Result<Chat> {
    let path = file(dir, &chat.id)?;
    std::fs::create_dir_all(dir)?;
    chat.updated = now_ms().max(chat.updated);
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec(&chat)?)?;
    std::fs::rename(&tmp, &path)?;
    Ok(chat)
}

pub fn delete(dir: &Path, id: &str) -> Result<()> {
    match std::fs::remove_file(file(dir, id)?) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn chat(id: &str, title: &str) -> Chat {
        Chat {
            id: id.into(),
            title: title.into(),
            updated: 0,
            session: Some("s-1".into()),
            items: json!([{"kind": "user", "text": title}]),
        }
    }

    #[test]
    fn saved_chats_come_back_newest_first() {
        let dir = std::env::temp_dir().join(format!("relay-chats-{}", now_ms()));
        save(&dir, chat("a", "First")).unwrap();
        let mut b = chat("b", "Second");
        b.updated = now_ms() + 10_000;
        save(&dir, b).unwrap();
        let listed = list(&dir);
        assert_eq!(
            listed.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
            ["b", "a"]
        );
        let a = load(&dir, "a").unwrap();
        assert_eq!(a.session.as_deref(), Some("s-1"));
        assert_eq!(a.items[0]["text"], "First");
        delete(&dir, "a").unwrap();
        delete(&dir, "a").unwrap();
        assert_eq!(list(&dir).len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ids_cannot_leave_the_folder() {
        let dir = std::env::temp_dir();
        assert!(load(&dir, "../settings").is_err());
        assert!(save(&dir, chat("a/b", "x")).is_err());
        assert!(delete(&dir, "").is_err());
        assert!(list(&dir.join("relay-no-such-dir")).is_empty());
    }
}
