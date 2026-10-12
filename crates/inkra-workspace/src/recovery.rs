//! Crash recovery (FS-004, FS-006): unsaved content is written every few seconds
//! to `<data_dir>/recovery/<id>.md` plus metadata; after a crash the next start
//! offers to restore it.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::fs::atomic_write;
use crate::time::Utc;

/// Metadata of a recovery entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoveryMeta {
    /// Entry id.
    pub id: String,
    /// Original file, if the document had one.
    pub path: Option<PathBuf>,
    /// Display title.
    pub title: String,
    /// When it was written (UTC, `YYYY-MM-DD HH:mm`).
    pub time: String,
}

/// The recovery directory.
#[derive(Debug, Clone)]
pub struct Recovery {
    dir: PathBuf,
}

impl Recovery {
    /// A store in `dir`.
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    fn content(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.md"))
    }

    fn meta(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.json"))
    }

    /// Writes (or replaces) an entry.
    pub fn write(
        &self,
        id: &str,
        path: Option<&Path>,
        title: &str,
        text: &str,
    ) -> std::io::Result<()> {
        let meta = RecoveryMeta {
            id: id.to_owned(),
            path: path.map(Path::to_owned),
            title: title.to_owned(),
            time: Utc::now().date_time(),
        };
        atomic_write(&self.content(id), text.as_bytes())?;
        let json = serde_json::to_string_pretty(&meta).map_err(std::io::Error::other)?;
        atomic_write(&self.meta(id), json.as_bytes())
    }

    /// Removes an entry (after saving, or when the user discards changes).
    pub fn remove(&self, id: &str) {
        let _ = std::fs::remove_file(self.content(id));
        let _ = std::fs::remove_file(self.meta(id));
    }

    /// All entries, newest first.
    pub fn list(&self) -> Vec<RecoveryMeta> {
        let Ok(rd) = std::fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        let mut out: Vec<RecoveryMeta> = rd
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
            .filter_map(|e| std::fs::read_to_string(e.path()).ok())
            .filter_map(|s| serde_json::from_str(&s).ok())
            .filter(|m: &RecoveryMeta| self.content(&m.id).exists())
            .collect();
        out.sort_by(|a, b| b.time.cmp(&a.time));
        out
    }

    /// Content of an entry.
    pub fn read(&self, id: &str) -> Option<String> {
        std::fs::read_to_string(self.content(id)).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_list_remove() {
        let dir = tempfile::tempdir().unwrap();
        let r = Recovery::new(dir.path());
        r.write("w1", Some(Path::new("/x/a.md")), "a.md", "hello")
            .unwrap();
        let l = r.list();
        assert_eq!(l.len(), 1);
        assert_eq!(l[0].path.as_deref(), Some(Path::new("/x/a.md")));
        assert_eq!(r.read("w1").as_deref(), Some("hello"));
        r.remove("w1");
        assert!(r.list().is_empty());
    }
}
