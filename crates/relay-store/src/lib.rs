//! SQLite persistence for chats and projects.
//!
//! The UI owns the shape of a conversation's `items` (stored as opaque JSON); the store keeps
//! them with metadata, full-text-ish search text, and the AI session context for each chat.

use std::path::Path;
use std::sync::{Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("invalid data: {0}")]
    Invalid(String),
}

pub type Result<T> = std::result::Result<T, StoreError>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConvSummary {
    pub id: String,
    pub title: String,
    pub project_id: Option<String>,
    pub pinned: bool,
    pub archived: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Conversation {
    #[serde(flatten)]
    pub summary: ConvSummary,
    pub items: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub color: String,
    pub instructions: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchHit {
    #[serde(flatten)]
    pub summary: ConvSummary,
    pub snippet: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SearchResults {
    pub conversations: Vec<SearchHit>,
    pub projects: Vec<Project>,
}

/// Changes to apply to a conversation; `None` leaves a field alone.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ConvPatch {
    pub title: Option<String>,
    pub pinned: Option<bool>,
    pub archived: Option<bool>,
    /// `Some(None)` removes the chat from its project.
    #[serde(default, deserialize_with = "double_option")]
    pub project_id: Option<Option<String>>,
}

fn double_option<'de, D, T>(d: D) -> std::result::Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(d).map(Some)
}

/// Changes to apply to a project.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ProjectPatch {
    pub name: Option<String>,
    pub color: Option<String>,
    pub instructions: Option<String>,
}

/// What the agent remembers about one conversation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AiContext {
    pub session_id: Option<String>,
    /// Recent (request, answer) pairs.
    pub history: Vec<(String, String)>,
}

pub struct Store {
    conn: Mutex<Connection>,
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default()
}

fn new_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS conversations (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    project_id TEXT,
    pinned INTEGER NOT NULL DEFAULT 0,
    archived INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    items_json TEXT NOT NULL DEFAULT '[]',
    search_text TEXT NOT NULL DEFAULT '',
    session_id TEXT,
    history_json TEXT NOT NULL DEFAULT '[]'
);
CREATE INDEX IF NOT EXISTS conv_updated ON conversations(updated_at DESC);
CREATE INDEX IF NOT EXISTS conv_project ON conversations(project_id);
CREATE TABLE IF NOT EXISTS projects (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    color TEXT NOT NULL,
    instructions TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
";

const SUMMARY_COLS: &str = "id, title, project_id, pinned, archived, created_at, updated_at";

fn summary_from(row: &rusqlite::Row<'_>) -> rusqlite::Result<ConvSummary> {
    Ok(ConvSummary {
        id: row.get(0)?,
        title: row.get(1)?,
        project_id: row.get(2)?,
        pinned: row.get::<_, i64>(3)? != 0,
        archived: row.get::<_, i64>(4)? != 0,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

fn project_from(row: &rusqlite::Row<'_>) -> rusqlite::Result<Project> {
    Ok(Project {
        id: row.get(0)?,
        name: row.get(1)?,
        color: row.get(2)?,
        instructions: row.get(3)?,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
    })
}

/// A short window of `text` around the first case-insensitive match of `needle`.
pub fn snippet(text: &str, needle: &str) -> String {
    let lower = text.to_lowercase();
    let Some(byte_pos) = lower.find(&needle.to_lowercase()) else {
        return text.chars().take(80).collect();
    };
    // `lower` may differ in byte length from `text` for exotic characters; work in chars.
    let char_pos = lower[..byte_pos].chars().count();
    let chars: Vec<char> = text.chars().collect();
    let start = char_pos.saturating_sub(30);
    let end = (char_pos + needle.chars().count() + 50).min(chars.len());
    let mut out: String = chars[start..end].iter().collect();
    out = out.replace('\n', " ");
    format!(
        "{}{}{}",
        if start > 0 { "…" } else { "" },
        out.trim(),
        if end < chars.len() { "…" } else { "" }
    )
}

fn like_pattern(query: &str) -> String {
    let escaped = query
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    format!("%{escaped}%")
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| StoreError::Invalid(format!("creating {}: {e}", parent.display())))?;
        }
        Self::from_connection(Connection::open(path)?)
    }

    pub fn in_memory() -> Result<Self> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(conn: Connection) -> Result<Self> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn db(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|p| p.into_inner())
    }

    // ---- conversations ----

    pub fn conv_create(&self, project_id: Option<&str>) -> Result<ConvSummary> {
        let now = now_ms();
        let summary = ConvSummary {
            id: new_id(),
            title: "New chat".into(),
            project_id: project_id.map(str::to_string),
            pinned: false,
            archived: false,
            created_at: now,
            updated_at: now,
        };
        self.db().execute(
            "INSERT INTO conversations (id, title, project_id, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?4)",
            params![summary.id, summary.title, summary.project_id, now],
        )?;
        Ok(summary)
    }

    pub fn conv_list(&self, archived: bool) -> Result<Vec<ConvSummary>> {
        let db = self.db();
        let mut stmt = db.prepare(&format!(
            "SELECT {SUMMARY_COLS} FROM conversations WHERE archived = ?1 ORDER BY updated_at DESC"
        ))?;
        let rows = stmt.query_map([archived as i64], summary_from)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn conv_summary(&self, id: &str) -> Result<ConvSummary> {
        self.db()
            .query_row(
                &format!("SELECT {SUMMARY_COLS} FROM conversations WHERE id = ?1"),
                [id],
                summary_from,
            )
            .optional()?
            .ok_or_else(|| StoreError::NotFound(format!("conversation {id}")))
    }

    pub fn conv_get(&self, id: &str) -> Result<Conversation> {
        let summary = self.conv_summary(id)?;
        let json: String = self.db().query_row(
            "SELECT items_json FROM conversations WHERE id = ?1",
            [id],
            |r| r.get(0),
        )?;
        let items = serde_json::from_str(&json).unwrap_or(serde_json::Value::Array(Vec::new()));
        Ok(Conversation { summary, items })
    }

    /// Store the rendered items and search text. Does not touch pinned/archived/project.
    pub fn conv_save(
        &self,
        id: &str,
        title: Option<&str>,
        items: &serde_json::Value,
        search_text: &str,
    ) -> Result<()> {
        if !items.is_array() {
            return Err(StoreError::Invalid("items must be an array".into()));
        }
        let changed = self.db().execute(
            "UPDATE conversations SET items_json = ?2, search_text = ?3, updated_at = ?4,
             title = COALESCE(?5, title) WHERE id = ?1",
            params![id, items.to_string(), search_text, now_ms(), title],
        )?;
        if changed == 0 {
            return Err(StoreError::NotFound(format!("conversation {id}")));
        }
        Ok(())
    }

    pub fn conv_update(&self, id: &str, patch: &ConvPatch) -> Result<ConvSummary> {
        {
            let db = self.db();
            let exists: bool = db
                .query_row("SELECT 1 FROM conversations WHERE id = ?1", [id], |_| {
                    Ok(true)
                })
                .optional()?
                .unwrap_or(false);
            if !exists {
                return Err(StoreError::NotFound(format!("conversation {id}")));
            }
            if let Some(title) = &patch.title {
                db.execute(
                    "UPDATE conversations SET title = ?2 WHERE id = ?1",
                    params![id, title],
                )?;
            }
            if let Some(pinned) = patch.pinned {
                db.execute(
                    "UPDATE conversations SET pinned = ?2 WHERE id = ?1",
                    params![id, pinned as i64],
                )?;
            }
            if let Some(archived) = patch.archived {
                db.execute(
                    "UPDATE conversations SET archived = ?2 WHERE id = ?1",
                    params![id, archived as i64],
                )?;
            }
            if let Some(project) = &patch.project_id {
                db.execute(
                    "UPDATE conversations SET project_id = ?2 WHERE id = ?1",
                    params![id, project],
                )?;
            }
        }
        self.conv_summary(id)
    }

    pub fn conv_delete(&self, id: &str) -> Result<()> {
        self.db()
            .execute("DELETE FROM conversations WHERE id = ?1", [id])?;
        Ok(())
    }

    // ---- AI context ----

    pub fn ai_context(&self, conv_id: &str) -> AiContext {
        let row: Option<(Option<String>, String)> = self
            .db()
            .query_row(
                "SELECT session_id, history_json FROM conversations WHERE id = ?1",
                [conv_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .unwrap_or(None);
        match row {
            Some((session_id, history)) => AiContext {
                session_id,
                history: serde_json::from_str(&history).unwrap_or_default(),
            },
            None => AiContext::default(),
        }
    }

    pub fn save_ai_context(&self, conv_id: &str, ctx: &AiContext) -> Result<()> {
        let history =
            serde_json::to_string(&ctx.history).map_err(|e| StoreError::Invalid(e.to_string()))?;
        self.db().execute(
            "UPDATE conversations SET session_id = ?2, history_json = ?3 WHERE id = ?1",
            params![conv_id, ctx.session_id, history],
        )?;
        Ok(())
    }

    /// Instructions of the project a conversation belongs to (empty if none).
    pub fn project_instructions_for(&self, conv_id: &str) -> String {
        self.db()
            .query_row(
                "SELECT p.instructions FROM conversations c JOIN projects p ON p.id = c.project_id WHERE c.id = ?1",
                [conv_id],
                |r| r.get(0),
            )
            .optional()
            .unwrap_or(None)
            .unwrap_or_default()
    }

    // ---- projects ----

    pub fn project_create(&self, name: &str, color: &str, instructions: &str) -> Result<Project> {
        let name = name.trim();
        if name.is_empty() {
            return Err(StoreError::Invalid("a project needs a name".into()));
        }
        let now = now_ms();
        let project = Project {
            id: new_id(),
            name: name.to_string(),
            color: color.to_string(),
            instructions: instructions.to_string(),
            created_at: now,
            updated_at: now,
        };
        self.db().execute(
            "INSERT INTO projects (id, name, color, instructions, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
            params![project.id, project.name, project.color, project.instructions, now],
        )?;
        Ok(project)
    }

    pub fn project_list(&self) -> Result<Vec<Project>> {
        let db = self.db();
        let mut stmt = db.prepare(
            "SELECT id, name, color, instructions, created_at, updated_at FROM projects ORDER BY name COLLATE NOCASE",
        )?;
        let rows = stmt.query_map([], project_from)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn project_get(&self, id: &str) -> Result<Project> {
        self.db()
            .query_row(
                "SELECT id, name, color, instructions, created_at, updated_at FROM projects WHERE id = ?1",
                [id],
                project_from,
            )
            .optional()?
            .ok_or_else(|| StoreError::NotFound(format!("project {id}")))
    }

    pub fn project_update(&self, id: &str, patch: &ProjectPatch) -> Result<Project> {
        self.project_get(id)?;
        {
            let db = self.db();
            if let Some(name) = &patch.name {
                if name.trim().is_empty() {
                    return Err(StoreError::Invalid("a project needs a name".into()));
                }
                db.execute(
                    "UPDATE projects SET name = ?2 WHERE id = ?1",
                    params![id, name.trim()],
                )?;
            }
            if let Some(color) = &patch.color {
                db.execute(
                    "UPDATE projects SET color = ?2 WHERE id = ?1",
                    params![id, color],
                )?;
            }
            if let Some(instructions) = &patch.instructions {
                db.execute(
                    "UPDATE projects SET instructions = ?2 WHERE id = ?1",
                    params![id, instructions],
                )?;
            }
            db.execute(
                "UPDATE projects SET updated_at = ?2 WHERE id = ?1",
                params![id, now_ms()],
            )?;
        }
        self.project_get(id)
    }

    /// Delete a project; its chats stay, unassigned.
    pub fn project_delete(&self, id: &str) -> Result<()> {
        let db = self.db();
        db.execute(
            "UPDATE conversations SET project_id = NULL WHERE project_id = ?1",
            [id],
        )?;
        db.execute("DELETE FROM projects WHERE id = ?1", [id])?;
        Ok(())
    }

    // ---- search ----

    pub fn search(&self, query: &str) -> Result<SearchResults> {
        let query = query.trim();
        if query.is_empty() {
            return Ok(SearchResults::default());
        }
        let pattern = like_pattern(query);
        let conversations = {
            let db = self.db();
            let mut stmt = db.prepare(&format!(
                "SELECT {SUMMARY_COLS}, search_text FROM conversations
                 WHERE title LIKE ?1 ESCAPE '\\' OR search_text LIKE ?1 ESCAPE '\\'
                 ORDER BY updated_at DESC LIMIT 50"
            ))?;
            let rows = stmt.query_map([&pattern], |r| {
                Ok((summary_from(r)?, r.get::<_, String>(7)?))
            })?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };
        let hits = conversations
            .into_iter()
            .map(|(summary, text)| {
                let haystack = if summary.title.to_lowercase().contains(&query.to_lowercase())
                    && !text.to_lowercase().contains(&query.to_lowercase())
                {
                    summary.title.clone()
                } else {
                    text
                };
                SearchHit {
                    snippet: snippet(&haystack, query),
                    summary,
                }
            })
            .collect();
        let projects = {
            let db = self.db();
            let mut stmt = db.prepare(
                "SELECT id, name, color, instructions, created_at, updated_at FROM projects
                 WHERE name LIKE ?1 ESCAPE '\\' OR instructions LIKE ?1 ESCAPE '\\' ORDER BY name COLLATE NOCASE LIMIT 20",
            )?;
            let rows = stmt.query_map([&pattern], project_from)?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };
        Ok(SearchResults {
            conversations: hits,
            projects,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn store() -> Store {
        Store::in_memory().unwrap()
    }

    #[test]
    fn create_save_get_round_trip() {
        let s = store();
        let c = s.conv_create(None).unwrap();
        assert_eq!(c.title, "New chat");
        s.conv_save(
            &c.id,
            Some("Clean downloads"),
            &json!([{"kind":"user","text":"hi"}]),
            "hi there",
        )
        .unwrap();
        let got = s.conv_get(&c.id).unwrap();
        assert_eq!(got.summary.title, "Clean downloads");
        assert_eq!(got.items[0]["text"], "hi");
        assert!(got.summary.updated_at >= c.updated_at);
        // Title is kept when not supplied.
        s.conv_save(&c.id, None, &json!([]), "").unwrap();
        assert_eq!(s.conv_get(&c.id).unwrap().summary.title, "Clean downloads");
    }

    #[test]
    fn saving_requires_an_existing_chat_and_array_items() {
        let s = store();
        assert!(matches!(
            s.conv_save("nope", None, &json!([]), ""),
            Err(StoreError::NotFound(_))
        ));
        let c = s.conv_create(None).unwrap();
        assert!(matches!(
            s.conv_save(&c.id, None, &json!({}), ""),
            Err(StoreError::Invalid(_))
        ));
        assert!(matches!(s.conv_get("nope"), Err(StoreError::NotFound(_))));
    }

    #[test]
    fn list_orders_by_recent_and_filters_archived() {
        let s = store();
        let a = s.conv_create(None).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
        let b = s.conv_create(None).unwrap();
        let ids: Vec<_> = s
            .conv_list(false)
            .unwrap()
            .into_iter()
            .map(|c| c.id)
            .collect();
        assert_eq!(ids, vec![b.id.clone(), a.id.clone()]);
        s.conv_update(
            &a.id,
            &ConvPatch {
                archived: Some(true),
                ..ConvPatch::default()
            },
        )
        .unwrap();
        assert_eq!(s.conv_list(false).unwrap().len(), 1);
        assert_eq!(s.conv_list(true).unwrap()[0].id, a.id);
    }

    #[test]
    fn update_pin_rename_and_project_assignment() {
        let s = store();
        let p = s.project_create("Job hunt", "blue", "Be brief").unwrap();
        let c = s.conv_create(None).unwrap();
        let u = s
            .conv_update(
                &c.id,
                &ConvPatch {
                    title: Some("Renamed".into()),
                    pinned: Some(true),
                    project_id: Some(Some(p.id.clone())),
                    ..ConvPatch::default()
                },
            )
            .unwrap();
        assert_eq!(
            (u.title.as_str(), u.pinned, u.project_id.as_deref()),
            ("Renamed", true, Some(p.id.as_str()))
        );
        let u = s
            .conv_update(
                &c.id,
                &ConvPatch {
                    project_id: Some(None),
                    ..ConvPatch::default()
                },
            )
            .unwrap();
        assert_eq!(u.project_id, None);
        assert!(s.conv_update("missing", &ConvPatch::default()).is_err());
    }

    #[test]
    fn patch_distinguishes_missing_from_null_project() {
        let none: ConvPatch = serde_json::from_str(r#"{"title":"x"}"#).unwrap();
        assert!(none.project_id.is_none());
        let null: ConvPatch = serde_json::from_str(r#"{"project_id":null}"#).unwrap();
        assert_eq!(null.project_id, Some(None));
        let set: ConvPatch = serde_json::from_str(r#"{"project_id":"p1"}"#).unwrap();
        assert_eq!(set.project_id, Some(Some("p1".to_string())));
    }

    #[test]
    fn projects_crud_and_deleting_unassigns_chats() {
        let s = store();
        assert!(s.project_create("   ", "red", "").is_err());
        let p = s.project_create("Games", "green", "").unwrap();
        let c = s.conv_create(Some(&p.id)).unwrap();
        let p2 = s
            .project_update(
                &p.id,
                &ProjectPatch {
                    name: Some("Gaming".into()),
                    instructions: Some("fun".into()),
                    ..ProjectPatch::default()
                },
            )
            .unwrap();
        assert_eq!(
            (
                p2.name.as_str(),
                p2.instructions.as_str(),
                p2.color.as_str()
            ),
            ("Gaming", "fun", "green")
        );
        assert_eq!(s.project_list().unwrap().len(), 1);
        assert_eq!(s.project_instructions_for(&c.id), "fun");
        s.project_delete(&p.id).unwrap();
        assert!(s.project_list().unwrap().is_empty());
        assert_eq!(s.conv_summary(&c.id).unwrap().project_id, None);
        assert_eq!(s.project_instructions_for(&c.id), "");
        assert!(s.project_update("gone", &ProjectPatch::default()).is_err());
    }

    #[test]
    fn deleting_a_chat_removes_it() {
        let s = store();
        let c = s.conv_create(None).unwrap();
        s.conv_delete(&c.id).unwrap();
        assert!(s.conv_get(&c.id).is_err());
        s.conv_delete(&c.id).unwrap();
    }

    #[test]
    fn search_finds_titles_text_and_projects_with_snippets() {
        let s = store();
        let c = s.conv_create(None).unwrap();
        s.conv_save(
            &c.id,
            Some("Disk space"),
            &json!([]),
            "please find what is using my disk space on drive C",
        )
        .unwrap();
        let other = s.conv_create(None).unwrap();
        s.conv_save(&other.id, Some("Poem"), &json!([]), "roses are red")
            .unwrap();
        s.project_create("Disk projects", "gray", "").unwrap();
        let r = s.search("DISK").unwrap();
        assert_eq!(r.conversations.len(), 1);
        assert_eq!(r.conversations[0].summary.id, c.id);
        assert!(r.conversations[0].snippet.to_lowercase().contains("disk"));
        assert_eq!(r.projects.len(), 1);
        assert!(s.search("   ").unwrap().conversations.is_empty());
        // Wildcards in the query are literal.
        assert!(s.search("100%").unwrap().conversations.is_empty());
        assert!(s.search("_").unwrap().conversations.is_empty());
    }

    #[test]
    fn ai_context_round_trips_per_chat() {
        let s = store();
        let a = s.conv_create(None).unwrap();
        let b = s.conv_create(None).unwrap();
        assert_eq!(s.ai_context(&a.id), AiContext::default());
        let ctx = AiContext {
            session_id: Some("sess".into()),
            history: vec![("q".into(), "a".into())],
        };
        s.save_ai_context(&a.id, &ctx).unwrap();
        assert_eq!(s.ai_context(&a.id), ctx);
        assert_eq!(s.ai_context(&b.id), AiContext::default());
        assert_eq!(s.ai_context("unknown"), AiContext::default());
    }

    #[test]
    fn snippets_are_windowed_and_char_safe() {
        let text = format!("{}needle{}", "é".repeat(100), "ü".repeat(100));
        let s = snippet(&text, "NEEDLE");
        assert!(s.contains("needle") && s.starts_with('…') && s.ends_with('…'));
        assert!(snippet("short text", "zzz").starts_with("short"));
    }

    #[test]
    fn data_survives_reopening_the_file() {
        let dir = std::env::temp_dir().join(format!("relay-store-{}", std::process::id()));
        let file = dir.join("relay.db");
        let id = {
            let s = Store::open(&file).unwrap();
            let c = s.conv_create(None).unwrap();
            s.conv_save(&c.id, Some("Kept"), &json!([1, 2]), "kept")
                .unwrap();
            c.id
        };
        let s = Store::open(&file).unwrap();
        assert_eq!(s.conv_get(&id).unwrap().summary.title, "Kept");
        drop(s);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
