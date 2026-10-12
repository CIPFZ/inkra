//! Watching open files for external changes (FS-007).
//!
//! Parent directories are watched (editors often save by replacing the file).
//! Events are debounced and delivered through a channel; the host decides
//! whether to reload or warn.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

use notify_debouncer_mini::{
    DebounceEventResult, Debouncer, new_debouncer, notify::RecommendedWatcher,
    notify::RecursiveMode,
};

/// File watcher.
pub struct Watcher {
    debouncer: Option<Debouncer<RecommendedWatcher>>,
    rx: Receiver<Vec<PathBuf>>,
    dirs: HashSet<PathBuf>,
}

impl Watcher {
    /// Creates a watcher; `wake` is called (from the watcher thread) when events
    /// arrive, e.g. to request a repaint.
    pub fn new(wake: impl Fn() + Send + 'static) -> Self {
        let (tx, rx) = channel();
        let debouncer = new_debouncer(
            Duration::from_millis(300),
            move |res: DebounceEventResult| {
                if let Ok(events) = res {
                    let paths: Vec<PathBuf> = events.into_iter().map(|e| e.path).collect();
                    if !paths.is_empty() && tx.send(paths).is_ok() {
                        wake();
                    }
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
        }
    }

    /// Watches the directories containing `files` (and stops watching others).
    pub fn set_files<'a>(&mut self, files: impl IntoIterator<Item = &'a Path>) {
        let want: HashSet<PathBuf> = files
            .into_iter()
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

    /// Paths reported changed since the last call.
    pub fn changed(&self) -> HashSet<PathBuf> {
        let mut out = HashSet::new();
        while let Ok(v) = self.rx.try_recv() {
            out.extend(v);
        }
        out
    }
}
