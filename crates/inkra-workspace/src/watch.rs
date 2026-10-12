//! Watching open files for external changes (FS-007).
//!
//! Parent directories are watched (editors often save by replacing the file),
//! but only changes to the watched files themselves are reported, and only when
//! their modification time or size changed: reads and unrelated activity in the
//! same directory never wake the UI (SPEC 2.5 idle CPU 0%).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use notify_debouncer_mini::{
    DebounceEventResult, Debouncer, new_debouncer, notify::RecommendedWatcher,
    notify::RecursiveMode,
};

/// Last seen `(mtime, len)` of a path (`None`: missing).
type Stamp = Option<(Option<std::time::SystemTime>, u64)>;

fn stamp(p: &Path) -> Stamp {
    std::fs::metadata(p)
        .ok()
        .map(|m| (m.modified().ok(), m.len()))
}

#[derive(Default)]
struct Shared {
    files: HashMap<PathBuf, Stamp>,
}

/// File watcher.
pub struct Watcher {
    debouncer: Option<Debouncer<RecommendedWatcher>>,
    rx: Receiver<Vec<PathBuf>>,
    dirs: HashSet<PathBuf>,
    shared: Arc<Mutex<Shared>>,
}

impl Watcher {
    /// Creates a watcher; `wake` is called (from the watcher thread) when a
    /// watched file changed, e.g. to request a repaint.
    pub fn new(wake: impl Fn() + Send + 'static) -> Self {
        let (tx, rx) = channel();
        let shared: Arc<Mutex<Shared>> = Default::default();
        let sh = shared.clone();
        let debouncer = new_debouncer(
            Duration::from_millis(300),
            move |res: DebounceEventResult| {
                let Ok(events) = res else { return };
                let mut changed = Vec::new();
                if let Ok(mut s) = sh.lock() {
                    for e in events {
                        if let Some(prev) = s.files.get_mut(&e.path) {
                            let now = stamp(&e.path);
                            if *prev != now {
                                *prev = now;
                                changed.push(e.path);
                            }
                        }
                    }
                }
                if !changed.is_empty() && tx.send(changed).is_ok() {
                    wake();
                }
            },
        );
        let debouncer = match debouncer {
            Ok(d) => Some(d),
            Err(e) => {
                log::warn!("file watching unavailable: {e}");
                None
            }
        };
        Self {
            debouncer,
            rx,
            dirs: HashSet::new(),
            shared,
        }
    }

    /// Watches `files` (and stops watching others).
    pub fn set_files<'a>(&mut self, files: impl IntoIterator<Item = &'a Path>) {
        let files: Vec<&Path> = files.into_iter().collect();
        if let Ok(mut s) = self.shared.lock() {
            s.files.retain(|p, _| files.contains(&p.as_path()));
            for f in &files {
                s.files.entry(f.to_path_buf()).or_insert_with(|| stamp(f));
            }
        }
        let want: HashSet<PathBuf> = files
            .iter()
            .filter_map(|f| f.parent().map(Path::to_owned))
            .collect();
        let Some(d) = self.debouncer.as_mut() else {
            return;
        };
        for gone in self.dirs.difference(&want) {
            let _ = d.watcher().unwatch(gone);
        }
        for new in want.difference(&self.dirs) {
            if let Err(e) = d.watcher().watch(new, RecursiveMode::NonRecursive) {
                log::debug!("cannot watch {}: {e}", new.display());
            }
        }
        self.dirs = want;
    }

    /// Records the current state of `file` as seen (e.g. after saving it
    /// ourselves), so the write is not reported as an external change.
    pub fn acknowledge(&mut self, file: &Path) {
        if let Ok(mut s) = self.shared.lock()
            && let Some(st) = s.files.get_mut(file)
        {
            *st = stamp(file);
        }
    }

    /// Watched files that changed since the last call.
    pub fn changed(&mut self) -> HashSet<PathBuf> {
        let mut out = HashSet::new();
        while let Ok(v) = self.rx.try_recv() {
            out.extend(v);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_changes_of_watched_files_only() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("a.md");
        let other = dir.path().join("b.log");
        std::fs::write(&f, "1").unwrap();
        let mut w = Watcher::new(|| {});
        w.set_files([f.as_path()]);
        std::thread::sleep(Duration::from_millis(200));
        // reads and other files are ignored
        let _ = std::fs::read(&f).unwrap();
        std::fs::write(&other, "x").unwrap();
        std::thread::sleep(Duration::from_millis(600));
        assert!(w.changed().is_empty());
        std::fs::write(&f, "22").unwrap();
        let mut seen = HashSet::new();
        for _ in 0..30 {
            std::thread::sleep(Duration::from_millis(100));
            seen.extend(w.changed());
            if !seen.is_empty() {
                break;
            }
        }
        assert!(seen.iter().any(|p| p.ends_with("a.md")), "{seen:?}");
    }
}
