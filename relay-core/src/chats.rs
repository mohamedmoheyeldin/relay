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
    /// The project the chat belongs to, if any (see `projects`).
    #[serde(default)]
    pub project: Option<String>,
}

/// A chat in the sidebar list.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Summary {
    pub id: String,
    pub title: String,
    pub updated: u64,
    pub pinned: bool,
    pub project: Option<String>,
}

impl From<&Chat> for Summary {
    fn from(c: &Chat) -> Self {
        Self {
            id: c.id.clone(),
            title: c.title.clone(),
            updated: c.updated,
            pinned: c.pinned,
            project: c.project.clone(),
        }
    }
}

/// One task Relay ran on screen: a plan the local operator made, and how it ended.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Task {
    pub chat_id: String,
    pub chat_title: String,
    /// Which task in the chat, counting from 0; the chat screen opens its thread.
    pub index: usize,
    pub goal: String,
    pub app: String,
    pub steps: usize,
    /// None while it runs (or if Relay closed before it finished).
    pub ok: Option<bool>,
    pub result: String,
    pub updated: u64,
}

/// Every task in every saved chat, newest chat first.
pub fn tasks(dir: &Path) -> Vec<Task> {
    let mut out = Vec::new();
    for summary in list(dir) {
        let Ok(chat) = load(dir, &summary.id) else {
            continue;
        };
        out.extend(tasks_in(&chat));
    }
    out
}

fn tasks_in(chat: &Chat) -> Vec<Task> {
    let mut out: Vec<Task> = Vec::new();
    let Some(items) = chat.items.as_array() else {
        return out;
    };
    for item in items {
        let kind = item["kind"].as_str().unwrap_or("");
        let text = |k: &str| item[k].as_str().unwrap_or("").to_string();
        match kind {
            "plan" => out.push(Task {
                chat_id: chat.id.clone(),
                chat_title: chat.title.clone(),
                index: out.len(),
                goal: text("goal"),
                app: text("app"),
                steps: 0,
                ok: None,
                result: String::new(),
                updated: chat.updated,
            }),
            "step" => {
                if let Some(t) = out.last_mut().filter(|t| t.ok.is_none()) {
                    t.steps += 1;
                }
            }
            "done" => {
                if let Some(t) = out.last_mut().filter(|t| t.ok.is_none()) {
                    t.ok = Some(item["ok"].as_bool().unwrap_or(false));
                    t.result = text("text");
                }
            }
            "note" if text("text").starts_with("Stopping") => {
                if let Some(t) = out.last_mut().filter(|t| t.ok.is_none()) {
                    t.ok = Some(false);
                    t.result = "Stopped".into();
                }
            }
            // A reply while a task runs is an answer to it; the task goes on.
            "user" | "note" | "question" => {}
            // Cloud AI carried on after the task without a verdict from it: it ended.
            _ => {
                if let Some(t) = out.last_mut().filter(|t| t.ok.is_none()) {
                    t.ok = Some(true);
                }
            }
        }
    }
    out
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
        chat.project = old.project;
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

/// Move a chat into a project, or out of one with `None`.
pub fn move_to(dir: &Path, id: &str, project: Option<&str>) -> Result<Chat> {
    let mut chat = load(dir, id)?;
    chat.project = project.map(str::to_string);
    write(dir, &chat)?;
    Ok(chat)
}

/// A deleted project's chats stay, outside any project.
pub fn leave_project(dir: &Path, project: &str) {
    for s in list(dir) {
        if s.project.as_deref() == Some(project) {
            let _ = move_to(dir, &s.id, None);
        }
    }
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
            project: None,
        }
    }

    #[test]
    fn tasks_come_from_plans_and_their_ends() {
        let mut c = chat("t", "Trips");
        c.items = json!([
            {"kind": "user", "text": "Find a flight"},
            {"kind": "plan", "goal": "Find a flight", "app": "Brave", "steps": [], "doneWhen": ""},
            {"kind": "step", "step": 1}, {"kind": "step", "step": 2},
            {"kind": "done", "ok": true, "text": "", "seconds": 3.0},
            {"kind": "user", "text": "And a hotel"},
            {"kind": "plan", "goal": "Find a hotel", "app": "Brave", "steps": [], "doneWhen": ""},
            {"kind": "step", "step": 1},
            {"kind": "user", "text": "the cheaper one"},
            {"kind": "step", "step": 2},
            {"kind": "note", "text": "Stopping…"},
            {"kind": "plan", "goal": "Book it", "app": "Brave", "steps": [], "doneWhen": ""},
            {"kind": "assistant", "text": "Booked."},
        ]);
        let t = tasks_in(&c);
        assert_eq!(t.len(), 3);
        assert_eq!((t[0].steps, t[0].ok, t[0].index), (2, Some(true), 0));
        assert_eq!(
            (t[1].goal.as_str(), t[1].steps, t[1].ok),
            ("Find a hotel", 2, Some(false))
        );
        assert_eq!(t[2].ok, Some(true));
    }

    #[test]
    fn moving_keeps_the_project_across_saves() {
        let dir = std::env::temp_dir().join(format!("relay-chats-move-{}", now_ms()));
        save(&dir, chat("m", "One")).unwrap();
        move_to(&dir, "m", Some("p1")).unwrap();
        let again = save(&dir, chat("m", "One")).unwrap();
        assert_eq!(again.project.as_deref(), Some("p1"));
        leave_project(&dir, "p1");
        assert_eq!(load(&dir, "m").unwrap().project, None);
        let _ = std::fs::remove_dir_all(&dir);
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
