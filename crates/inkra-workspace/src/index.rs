//! Background services over the sidebar root folder: a recursive watcher, the
//! file index used by quick open (FS-030) and the article list (FS-021).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use notify_debouncer_mini::{
    DebounceEventResult, Debouncer, new_debouncer, notify::RecommendedWatcher,
    notify::RecursiveMode,
};

use crate::tree::{self, Filter};

/// Maximum number of files indexed.
pub const MAX_FILES: usize = 200_000;

/// Watches a folder recursively (debounced 300 ms, FS-022).
pub struct RootWatcher {
    _debouncer: Debouncer<RecommendedWatcher>,
    rx: Receiver<Vec<PathBuf>>,
}

impl RootWatcher {
    /// Starts watching `root`; `wake` runs on the watcher thread after changes.
    pub fn new(root: &Path, wake: impl Fn() + Send + 'static) -> Option<Self> {
        let (tx, rx) = channel();
        let mut d = new_debouncer(
            Duration::from_millis(300),
            move |res: DebounceEventResult| {
                if let Ok(events) = res {
                    let paths: Vec<PathBuf> = events.into_iter().map(|e| e.path).collect();
                    if !paths.is_empty() && tx.send(paths).is_ok() {
                        wake();
                    }
                }
            },
        )
        .map_err(|e| log::warn!("cannot watch {}: {e}", root.display()))
        .ok()?;
        d.watcher()
            .watch(root, RecursiveMode::Recursive)
            .map_err(|e| log::warn!("cannot watch {}: {e}", root.display()))
            .ok()?;
        Some(Self { _debouncer: d, rx })
    }

    /// Paths changed since the last call (deduplicated).
    pub fn changed(&self) -> Vec<PathBuf> {
        let mut out: Vec<PathBuf> = Vec::new();
        while let Ok(v) = self.rx.try_recv() {
            out.extend(v);
        }
        out.sort();
        out.dedup();
        out
    }
}

/// An indexed file.
#[derive(Debug, Clone, PartialEq)]
pub struct IndexedFile {
    /// Absolute path.
    pub path: PathBuf,
    /// Path relative to the root with `/` separators (what quick open matches).
    pub rel: String,
}

#[derive(Default)]
struct IndexState {
    files: Vec<IndexedFile>,
    ready: bool,
}

/// The file index of a root folder, built in the background.
pub struct FileIndex {
    root: PathBuf,
    filter: Filter,
    state: Arc<Mutex<IndexState>>,
    version: Arc<AtomicU64>,
}

fn rel_of(root: &Path, p: &Path) -> String {
    p.strip_prefix(root)
        .unwrap_or(p)
        .to_string_lossy()
        .replace('\\', "/")
}

impl FileIndex {
    /// Starts indexing `root` on a background thread.
    pub fn spawn(
        root: &Path,
        filter: Filter,
        gitignore: bool,
        wake: impl Fn() + Send + 'static,
    ) -> Self {
        let state: Arc<Mutex<IndexState>> = Default::default();
        let version = Arc::new(AtomicU64::new(0));
        let (st, ver, r, f) = (
            state.clone(),
            version.clone(),
            root.to_owned(),
            filter.clone(),
        );
        std::thread::Builder::new()
            .name("inkra-index".into())
            .spawn(move || {
                let t0 = std::time::Instant::now();
                let files: Vec<IndexedFile> = tree::files_under(&r, &f, gitignore, true, MAX_FILES)
                    .into_iter()
                    .map(|path| IndexedFile {
                        rel: rel_of(&r, &path),
                        path,
                    })
                    .collect();
                log::info!(
                    "indexed {} files under {} in {:.0} ms",
                    files.len(),
                    r.display(),
                    t0.elapsed().as_secs_f64() * 1e3
                );
                if let Ok(mut s) = st.lock() {
                    s.files = files;
                    s.ready = true;
                }
                ver.fetch_add(1, Ordering::SeqCst);
                wake();
            })
            .ok();
        Self {
            root: root.to_owned(),
            filter,
            state,
            version,
        }
    }

    /// The root folder.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Whether the initial walk finished.
    pub fn ready(&self) -> bool {
        self.state.lock().map(|s| s.ready).unwrap_or(false)
    }

    /// Changes whenever the file list changes.
    pub fn version(&self) -> u64 {
        self.version.load(Ordering::SeqCst)
    }

    /// Runs `f` on the current file list.
    pub fn with_files<R>(&self, f: impl FnOnce(&[IndexedFile]) -> R) -> R {
        match self.state.lock() {
            Ok(s) => f(&s.files),
            Err(_) => f(&[]),
        }
    }

    /// Applies watcher events: new files are added, vanished ones removed.
    pub fn update(&self, changed: &[PathBuf]) {
        let Ok(mut s) = self.state.lock() else { return };
        let mut touched = false;
        for p in changed {
            if !p.starts_with(&self.root) {
                continue;
            }
            if p.is_dir() {
                // A new folder (or a moved one): index its contents.
                for f in tree::files_under(p, &self.filter, true, true, MAX_FILES) {
                    if !s.files.iter().any(|x| x.path == f) {
                        s.files.push(IndexedFile {
                            rel: rel_of(&self.root, &f),
                            path: f,
                        });
                        touched = true;
                    }
                }
            } else if p.is_file() {
                if self.filter.accepts(p, false) && !s.files.iter().any(|x| &x.path == p) {
                    s.files.push(IndexedFile {
                        rel: rel_of(&self.root, p),
                        path: p.clone(),
                    });
                    touched = true;
                }
            } else {
                let before = s.files.len();
                s.files.retain(|x| !x.path.starts_with(p));
                touched |= s.files.len() != before;
            }
        }
        if touched {
            self.version.fetch_add(1, Ordering::SeqCst);
        }
    }
}

/// An article list entry (FS-021).
#[derive(Debug, Clone, PartialEq)]
pub struct Article {
    /// Path.
    pub path: PathBuf,
    /// File name.
    pub name: String,
    /// Modification time.
    pub modified: Option<SystemTime>,
    /// Creation time.
    pub created: Option<SystemTime>,
    /// First lines of content, Markdown syntax stripped (≤ 2 lines).
    pub summary: String,
}

/// The first content lines of a Markdown file: front matter skipped, block
/// markers removed, at most two lines of up to 120 characters in total.
pub fn summary_of(text: &str) -> String {
    let mut lines = text.lines().peekable();
    if lines.peek().is_some_and(|l| l.trim_end() == "---") {
        lines.next();
        for l in lines.by_ref() {
            if l.trim_end() == "---" || l.trim_end() == "..." {
                break;
            }
        }
    }
    let mut out: Vec<String> = Vec::new();
    let mut in_fence = false;
    for l in lines {
        let t = l.trim();
        if t.starts_with("```") || t.starts_with("~~~") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence
            || t.is_empty()
            || t.starts_with("<!--")
            || t.chars().all(|c| "-*_=| :".contains(c))
        {
            continue;
        }
        let t = t.trim_start_matches(['#', '>', ' ']);
        let t = t
            .strip_prefix("- [ ] ")
            .or_else(|| t.strip_prefix("- [x] "))
            .unwrap_or(t);
        let t = t
            .strip_prefix("- ")
            .or_else(|| t.strip_prefix("* "))
            .or_else(|| t.strip_prefix("+ "))
            .unwrap_or(t);
        let clean: String = t.chars().filter(|c| !"*_`~".contains(*c)).collect();
        let clean = clean.trim().to_owned();
        if !clean.is_empty() {
            out.push(clean);
        }
        if out.len() == 2 {
            break;
        }
    }
    let s = out.join(" ");
    if s.chars().count() > 120 {
        format!("{}…", s.chars().take(119).collect::<String>())
    } else {
        s
    }
}

#[derive(Default)]
struct ArticleState {
    list: Vec<Article>,
    cache: HashMap<PathBuf, (Option<SystemTime>, String)>,
    busy: bool,
}

/// The article list of a root folder, refreshed in the background.
pub struct Articles {
    state: Arc<Mutex<ArticleState>>,
    version: Arc<AtomicU64>,
    dirty: Arc<AtomicBool>,
}

impl Default for Articles {
    fn default() -> Self {
        Self {
            state: Default::default(),
            version: Arc::new(AtomicU64::new(0)),
            dirty: Arc::new(AtomicBool::new(true)),
        }
    }
}

impl Articles {
    /// Requests a refresh (after watcher events).
    pub fn invalidate(&self) {
        self.dirty.store(true, Ordering::SeqCst);
    }

    /// Changes whenever the list changes.
    pub fn version(&self) -> u64 {
        self.version.load(Ordering::SeqCst)
    }

    /// Starts a background refresh when needed.
    pub fn refresh(
        &self,
        root: &Path,
        filter: &Filter,
        recursive: bool,
        wake: impl Fn() + Send + 'static,
    ) {
        if !self.dirty.swap(false, Ordering::SeqCst) {
            return;
        }
        {
            let Ok(mut s) = self.state.lock() else { return };
            if s.busy {
                self.dirty.store(true, Ordering::SeqCst);
                return;
            }
            s.busy = true;
        }
        let (st, ver, r, f) = (
            self.state.clone(),
            self.version.clone(),
            root.to_owned(),
            filter.clone(),
        );
        std::thread::Builder::new()
            .name("inkra-articles".into())
            .spawn(move || {
                let files = tree::files_under(&r, &f, true, recursive, 20_000);
                let cache = st.lock().map(|s| s.cache.clone()).unwrap_or_default();
                let mut new_cache = HashMap::new();
                let mut list = Vec::with_capacity(files.len());
                for path in files {
                    let meta = std::fs::metadata(&path).ok();
                    let modified = meta.as_ref().and_then(|m| m.modified().ok());
                    let created = meta.as_ref().and_then(|m| m.created().ok());
                    let summary = match cache.get(&path) {
                        Some((m, s)) if *m == modified => s.clone(),
                        _ => read_head(&path).map(|t| summary_of(&t)).unwrap_or_default(),
                    };
                    new_cache.insert(path.clone(), (modified, summary.clone()));
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    list.push(Article {
                        path,
                        name,
                        modified,
                        created,
                        summary,
                    });
                }
                if let Ok(mut s) = st.lock() {
                    s.list = list;
                    s.cache = new_cache;
                    s.busy = false;
                }
                ver.fetch_add(1, Ordering::SeqCst);
                wake();
            })
            .ok();
    }

    /// Runs `f` on the current list.
    pub fn with_list<R>(&self, f: impl FnOnce(&[Article]) -> R) -> R {
        match self.state.lock() {
            Ok(s) => f(&s.list),
            Err(_) => f(&[]),
        }
    }
}

fn read_head(p: &Path) -> Option<String> {
    use std::io::Read;
    let mut f = std::fs::File::open(p).ok()?;
    let mut buf = vec![0u8; 4096];
    let n = f.read(&mut buf).ok()?;
    buf.truncate(n);
    Some(String::from_utf8_lossy(&buf).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summaries() {
        assert_eq!(
            summary_of("---\ntitle: x\n---\n\n# Title\n\nSome **bold** text.\nmore"),
            "Title Some bold text."
        );
        assert_eq!(summary_of("```\ncode\n```\n- item one\n"), "item one");
        assert!(summary_of(&"word ".repeat(100)).ends_with('…'));
    }

    #[test]
    fn index_builds_and_updates() {
        let d = tempfile::tempdir().unwrap();
        let r = d.path().canonicalize().unwrap();
        std::fs::write(r.join("a.md"), "x").unwrap();
        let idx = FileIndex::spawn(&r, Filter::new(&r, false, false, ""), true, || {});
        for _ in 0..100 {
            if idx.ready() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(idx.with_files(|f| f.len()), 1);
        std::fs::write(r.join("b.md"), "y").unwrap();
        idx.update(&[r.join("b.md")]);
        assert_eq!(
            idx.with_files(|f| f.iter().map(|x| x.rel.clone()).collect::<Vec<_>>()),
            ["a.md", "b.md"]
        );
        std::fs::remove_file(r.join("a.md")).unwrap();
        idx.update(&[r.join("a.md")]);
        assert_eq!(idx.with_files(|f| f.len()), 1);
    }
}
