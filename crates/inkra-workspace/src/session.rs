//! Session restore (FS-006) and recent files (FS-002).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::fs::atomic_write;

/// One window of the last session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct WindowSession {
    /// Open file.
    pub path: Option<PathBuf>,
    /// Caret offset.
    pub caret: usize,
    /// Scroll offset.
    pub scroll: f32,
    /// Source mode.
    pub source_mode: bool,
    /// Sidebar shown.
    pub sidebar: bool,
    /// Outer position and size `[x, y, w, h]`.
    pub rect: Option<[f32; 4]>,
    /// Maximized.
    pub maximized: bool,
    /// Fullscreen.
    #[serde(default)]
    pub fullscreen: bool,
    /// Document offset shown at the top of the viewport.
    #[serde(default)]
    pub top: Option<usize>,
}

/// The saved session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Session {
    /// Windows in order.
    pub windows: Vec<WindowSession>,
    /// Sidebar root folder.
    pub root: Option<PathBuf>,
    /// Recently opened files, newest first (max 20).
    pub recent: Vec<PathBuf>,
    /// Recently opened folders.
    #[serde(default)]
    pub recent_folders: Vec<PathBuf>,
}

/// Maximum number of recent files (FS-002).
pub const MAX_RECENT: usize = 20;

impl Session {
    /// Loads the session file (defaults when missing or unreadable).
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    /// Saves atomically.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        atomic_write(path, json.as_bytes())
    }

    /// Records an opened file.
    pub fn touch_recent(&mut self, p: &Path) {
        self.recent.retain(|x| x != p);
        self.recent.insert(0, p.to_owned());
        self.recent.truncate(MAX_RECENT);
    }

    /// Records an opened folder.
    pub fn touch_folder(&mut self, p: &Path) {
        self.recent_folders.retain(|x| x != p);
        self.recent_folders.insert(0, p.to_owned());
        self.recent_folders.truncate(MAX_RECENT);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_is_bounded_and_unique() {
        let mut s = Session::default();
        for i in 0..30 {
            s.touch_recent(Path::new(&format!("/f{i}.md")));
        }
        s.touch_recent(Path::new("/f10.md"));
        assert_eq!(s.recent.len(), MAX_RECENT);
        assert_eq!(s.recent[0], PathBuf::from("/f10.md"));
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("session.json");
        s.save(&p).unwrap();
        assert_eq!(Session::load(&p), s);
    }
}
