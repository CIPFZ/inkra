//! Global search (FS-031): a background grep over the root folder with
//! `.gitignore` support, results streamed per file.

use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, channel};

use grep_matcher::Matcher;
use grep_regex::RegexMatcherBuilder;
use grep_searcher::{Searcher, SearcherBuilder, sinks};

use crate::tree::{self, Filter};

/// Maximum number of matching lines (FS-031).
pub const MAX_HITS: usize = 10_000;

/// Search options.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Query {
    /// Text or regular expression.
    pub pattern: String,
    /// Case-sensitive.
    pub case_sensitive: bool,
    /// Whole words.
    pub whole_word: bool,
    /// `pattern` is a regular expression.
    pub regex: bool,
    /// File globs (`*.md,!drafts/**`), empty: the sidebar filter.
    pub globs: String,
    /// Respect `.gitignore`.
    pub gitignore: bool,
}

/// One matching line.
#[derive(Debug, Clone, PartialEq)]
pub struct LineHit {
    /// 1-based line number.
    pub line: u64,
    /// The line (trimmed of the line break).
    pub text: String,
    /// Byte ranges of the matches in `text`.
    pub ranges: Vec<Range<usize>>,
}

/// Results of one file.
#[derive(Debug, Clone, PartialEq)]
pub struct FileHits {
    /// File.
    pub path: PathBuf,
    /// Matching lines.
    pub lines: Vec<LineHit>,
}

/// Search progress.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// Matches in one file.
    File(FileHits),
    /// Finished.
    Done {
        /// Files searched.
        files: usize,
        /// Matching lines.
        hits: usize,
        /// Stopped at [`MAX_HITS`].
        truncated: bool,
        /// Duration.
        ms: u64,
    },
    /// The pattern is invalid.
    Error(String),
}

/// A running search; dropping it cancels.
pub struct Search {
    rx: Receiver<Event>,
    cancel: Arc<AtomicBool>,
}

impl Drop for Search {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::SeqCst);
    }
}

impl Search {
    /// Events received since the last call.
    pub fn poll(&self) -> Vec<Event> {
        let mut out = Vec::new();
        while let Ok(e) = self.rx.try_recv() {
            out.push(e);
        }
        out
    }
}

/// Starts searching `root`.
pub fn start(root: &Path, q: Query, filter: Filter, wake: impl Fn() + Send + 'static) -> Search {
    let (tx, rx) = channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let c = cancel.clone();
    let root = root.to_owned();
    std::thread::Builder::new()
        .name("inkra-search".into())
        .spawn(move || {
            let t0 = std::time::Instant::now();
            let matcher = match RegexMatcherBuilder::new()
                .case_insensitive(!q.case_sensitive)
                .word(q.whole_word)
                .fixed_strings(!q.regex)
                .build(&q.pattern)
            {
                Ok(m) => m,
                Err(e) => {
                    let _ = tx.send(Event::Error(e.to_string()));
                    wake();
                    return;
                }
            };
            let filter = if q.globs.trim().is_empty() {
                filter
            } else {
                Filter::new(&root, filter.hidden, true, &q.globs)
            };
            let mut searcher = SearcherBuilder::new().line_number(true).build();
            let (mut files, mut hits, mut truncated) = (0usize, 0usize, false);
            let mut last_wake = std::time::Instant::now();
            for entry in tree::walker(&root, &filter, q.gitignore, true).flatten() {
                if c.load(Ordering::SeqCst) {
                    return;
                }
                if !entry.file_type().is_some_and(|t| t.is_file())
                    || !filter.accepts(entry.path(), false)
                {
                    continue;
                }
                files += 1;
                let lines = search_file(&mut searcher, &matcher, entry.path(), MAX_HITS - hits);
                if lines.is_empty() {
                    continue;
                }
                hits += lines.len();
                if tx
                    .send(Event::File(FileHits {
                        path: entry.into_path(),
                        lines,
                    }))
                    .is_err()
                {
                    return;
                }
                if last_wake.elapsed().as_millis() > 50 {
                    wake();
                    last_wake = std::time::Instant::now();
                }
                if hits >= MAX_HITS {
                    truncated = true;
                    break;
                }
            }
            let _ = tx.send(Event::Done {
                files,
                hits,
                truncated,
                ms: t0.elapsed().as_millis() as u64,
            });
            wake();
        })
        .ok();
    Search { rx, cancel }
}

fn search_file(
    searcher: &mut Searcher,
    m: &grep_regex::RegexMatcher,
    path: &Path,
    budget: usize,
) -> Vec<LineHit> {
    let mut out = Vec::new();
    let r = searcher.search_path(
        m,
        path,
        sinks::UTF8(|line, text| {
            let text = text.trim_end_matches(['\n', '\r']);
            let mut ranges = Vec::new();
            let _ = m.find_iter(text.as_bytes(), |mt| {
                ranges.push(mt.start()..mt.end());
                ranges.len() < 64
            });
            out.push(LineHit {
                line,
                text: text.to_owned(),
                ranges,
            });
            Ok(out.len() < budget)
        }),
    );
    if let Err(e) = r {
        log::debug!("search {}: {e}", path.display());
    }
    out
}

/// Runs a search to completion (tests, CLI).
pub fn run_blocking(root: &Path, q: Query, filter: Filter) -> (Vec<FileHits>, Option<Event>) {
    let s = start(root, q, filter, || {});
    let mut files = Vec::new();
    loop {
        match s.rx.recv() {
            Ok(Event::File(f)) => files.push(f),
            Ok(e) => return (files, Some(e)),
            Err(_) => return (files, None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_matches_with_options() {
        let d = tempfile::tempdir().unwrap();
        let r = d.path();
        std::fs::write(r.join("a.md"), "Hello world\nhello again\nworldwide\n").unwrap();
        std::fs::write(r.join("b.md"), "nothing here\n").unwrap();
        std::fs::create_dir_all(r.join("ignored")).unwrap();
        std::fs::write(r.join("ignored/c.md"), "hello\n").unwrap();
        std::fs::write(r.join(".gitignore"), "ignored/\n").unwrap();
        std::fs::create_dir_all(r.join(".git")).unwrap();
        let f = Filter::new(r, false, false, "");
        let q = Query {
            pattern: "hello".into(),
            gitignore: true,
            ..Default::default()
        };
        let (files, done) = run_blocking(r, q.clone(), f.clone());
        assert_eq!(files.len(), 1, "gitignored folder skipped");
        assert_eq!(files[0].lines.len(), 2);
        assert_eq!(files[0].lines[0].ranges, vec![0..5]);
        assert!(matches!(done, Some(Event::Done { hits: 2, .. })));
        let (files, _) = run_blocking(
            r,
            Query {
                case_sensitive: true,
                ..q.clone()
            },
            f.clone(),
        );
        assert_eq!(files[0].lines.len(), 1);
        let (files, _) = run_blocking(
            r,
            Query {
                pattern: "world".into(),
                whole_word: true,
                ..q.clone()
            },
            f.clone(),
        );
        assert_eq!(files[0].lines.len(), 1);
        let (files, _) = run_blocking(
            r,
            Query {
                pattern: "h.l+o".into(),
                regex: true,
                ..q.clone()
            },
            f.clone(),
        );
        assert_eq!(files[0].lines.len(), 2);
        let (_, e) = run_blocking(
            r,
            Query {
                pattern: "(".into(),
                regex: true,
                ..q.clone()
            },
            f.clone(),
        );
        assert!(matches!(e, Some(Event::Error(_))));
        let (files, _) = run_blocking(
            r,
            Query {
                gitignore: false,
                ..q
            },
            f,
        );
        assert_eq!(files.len(), 2);
    }
}

#[cfg(test)]
mod perf {
    use super::*;

    /// FS-031: first results within 2 s on 10 000 files.
    #[test]
    fn ten_thousand_files() {
        let d = tempfile::tempdir().unwrap();
        let r = d.path();
        for i in 0..10_000 {
            let dir = r.join(format!("d{}", i % 100));
            if i < 100 {
                std::fs::create_dir_all(&dir).unwrap();
            }
            let body = format!(
                "# Note {i}\n\nSome text about topic {} and more words here.\n{}\n",
                i % 37,
                "filler line\n".repeat(20)
            );
            std::fs::write(dir.join(format!("n{i}.md")), body).unwrap();
        }
        let q = Query {
            pattern: "topic 5 ".into(),
            gitignore: true,
            ..Default::default()
        };
        let t = std::time::Instant::now();
        let s = start(r, q, Filter::new(r, false, false, ""), || {});
        let first = match s.rx.recv().unwrap() {
            Event::File(_) => t.elapsed(),
            Event::Done { .. } => panic!("no results"),
            Event::Error(e) => panic!("{e}"),
        };
        let mut total = 1;
        while let Ok(Event::File(_)) = s.rx.recv() {
            total += 1;
        }
        let all = t.elapsed();
        eprintln!(
            "search 10k files: first result {:.0} ms, all ({total} files) {:.0} ms",
            first.as_secs_f64() * 1e3,
            all.as_secs_f64() * 1e3
        );
        assert_eq!(total, 10_000 / 37 + 1);
        if !cfg!(debug_assertions) {
            assert!(first.as_secs_f64() < 2.0);
        }
    }
}
