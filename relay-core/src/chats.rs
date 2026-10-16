//! Saved chats: one JSON file per conversation in `%LOCALAPPDATA%\Relay\chats`, so
//! the sidebar can list them and the owner can go back to one. The chat screen owns
//! what a conversation shows (`items`); Relay adds the Claude session it resumes.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
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
    #[serde(default)]
    pub pinned: bool,
    /// The owner named it, so the first message no longer does.
    #[serde(default)]
    pub renamed: bool,
}

/// A chat in the sidebar list.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Summary {
    pub id: String,
    pub title: String,
    pub updated: u64,
    pub pinned: bool,
}

impl From<&Chat> for Summary {
    fn from(c: &Chat) -> Self {
        Self {
            id: c.id.clone(),
            title: c.title.clone(),
            updated: c.updated,
            pinned: c.pinned,
        }
    }
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

/// Chats deleted since Relay started. A save already on its way when the owner
/// deleted the chat must not bring it back.
static DELETED: Mutex<Option<HashSet<PathBuf>>> = Mutex::new(None);

fn was_deleted(path: &Path) -> bool {
    DELETED
        .lock()
        .map(|d| d.as_ref().is_some_and(|d| d.contains(path)))
        .unwrap_or(false)
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
                .map(|c| Summary::from(&c))
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

/// Save what the chat screen shows. A name the owner gave and the pin are kept.
pub fn save(dir: &Path, mut chat: Chat) -> Result<Chat> {
    ensure!(!was_deleted(&file(dir, &chat.id)?), "this chat was deleted");
    if let Ok(old) = load(dir, &chat.id) {
        chat.pinned = old.pinned;
        if old.renamed {
            chat.renamed = true;
            chat.title = old.title;
        }
    }
    chat.updated = now_ms().max(chat.updated);
    write(dir, &chat)?;
    Ok(chat)
}

pub fn rename(dir: &Path, id: &str, title: &str) -> Result<Chat> {
    let title = title.trim();
    ensure!(!title.is_empty(), "a chat needs a name");
    let mut chat = load(dir, id)?;
    chat.title = title.to_string();
    chat.renamed = true;
    write(dir, &chat)?;
    Ok(chat)
}

pub fn pin(dir: &Path, id: &str, pinned: bool) -> Result<Chat> {
    let mut chat = load(dir, id)?;
    chat.pinned = pinned;
    write(dir, &chat)?;
    Ok(chat)
}

/// Write through a temporary file, so a crash never leaves half a chat.
fn write(dir: &Path, chat: &Chat) -> Result<()> {
    let path = file(dir, &chat.id)?;
    std::fs::create_dir_all(dir)?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec(chat)?)?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

pub fn delete(dir: &Path, id: &str) -> Result<()> {
    let path = file(dir, id)?;
    if let Ok(mut d) = DELETED.lock() {
        d.get_or_insert_with(HashSet::new).insert(path.clone());
    }
    match std::fs::remove_file(path) {
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
            pinned: false,
            renamed: false,
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
        rename(&dir, "a", " Trip plans ").unwrap();
        pin(&dir, "a", true).unwrap();
        // A later save from the chat screen keeps both.
        let a = save(&dir, chat("a", "First")).unwrap();
        assert_eq!((a.title.as_str(), a.pinned), ("Trip plans", true));
        assert!(rename(&dir, "a", "  ").is_err());
        delete(&dir, "a").unwrap();
        delete(&dir, "a").unwrap();
        assert_eq!(list(&dir).len(), 1);
        // A save that arrives after the delete does not bring the chat back.
        assert!(save(&dir, chat("a", "First")).is_err());
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
