//! Projects, as in the Claude app: a named folder of chats with its own
//! instructions, which Claude reads at the start of each chat in it. Stored in
//! `%LOCALAPPDATA%\Relay\projects.json`; chats name their project (see `chats`).

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Project {
    pub id: String,
    pub name: String,
    /// What Claude should know for every chat in this project.
    #[serde(default)]
    pub instructions: String,
    #[serde(default)]
    pub created: u64,
}

pub fn file() -> PathBuf {
    crate::paths::data_dir().join("projects.json")
}

/// All projects, oldest first. A missing or broken file is no projects.
pub fn list(path: &Path) -> Vec<Project> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

pub fn get(path: &Path, id: &str) -> Option<Project> {
    list(path).into_iter().find(|p| p.id == id)
}

fn write(path: &Path, all: &[Project]) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(all)?)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Create a project (empty `id`) or change one.
pub fn save(path: &Path, id: &str, name: &str, instructions: &str) -> Result<Project> {
    let name = name.trim();
    ensure!(!name.is_empty(), "a project needs a name");
    let mut all = list(path);
    let project = if let Some(p) = all.iter_mut().find(|p| !id.is_empty() && p.id == id) {
        p.name = name.into();
        p.instructions = instructions.trim().into();
        p.clone()
    } else {
        ensure!(id.is_empty(), "this project no longer exists");
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let p = Project {
            id: format!("p{now:x}"),
            name: name.into(),
            instructions: instructions.trim().into(),
            created: now,
        };
        all.push(p.clone());
        p
    };
    write(path, &all)?;
    Ok(project)
}

pub fn delete(path: &Path, id: &str) -> Result<()> {
    let mut all = list(path);
    all.retain(|p| p.id != id);
    write(path, &all)
}

/// The text put before the first message of a chat in a project, so Claude knows it.
pub fn preamble(project: &Project) -> Option<String> {
    let text = project.instructions.trim();
    (!text.is_empty()).then(|| {
        format!(
            "This chat is in the project \"{}\". The owner's instructions for it:\n{}\n\n",
            project.name, text
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projects_are_created_changed_and_deleted() {
        let path = std::env::temp_dir().join(format!("relay-projects-{}.json", std::process::id()));
        let _ = std::fs::remove_file(&path);
        assert!(list(&path).is_empty());
        let p = save(&path, "", " Job hunt ", "Use my CV in Documents.").unwrap();
        assert_eq!(p.name, "Job hunt");
        let p2 = save(&path, &p.id, "Jobs", "").unwrap();
        assert_eq!((p2.id.as_str(), p2.name.as_str()), (p.id.as_str(), "Jobs"));
        assert_eq!(list(&path).len(), 1);
        assert!(preamble(&p2).is_none());
        assert!(preamble(&p).unwrap().contains("Use my CV"));
        assert!(save(&path, "", "  ", "").is_err());
        assert!(save(&path, "nope", "x", "").is_err());
        delete(&path, &p.id).unwrap();
        assert!(get(&path, &p.id).is_none());
        let _ = std::fs::remove_file(&path);
    }
}
