//! The sidebar file tree model (FS-022): lazily listed directories, natural
//! ordering (folders first), Markdown-only by default with optional hidden /
//! all files / custom glob filters.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use inkra_text::sort::natural_cmp;

/// Markdown file extensions (FS-002).
pub const MARKDOWN_EXTS: &[&str] = &[
    "md",
    "markdown",
    "mdown",
    "mmd",
    "text",
    "txt",
    "mdx",
    "rmarkdown",
    "rmd",
    "mkd",
    "mdwn",
    "mdtxt",
    "mdtext",
    "apib",
];

/// Whether `p` has a Markdown extension.
pub fn is_markdown(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| MARKDOWN_EXTS.contains(&e.to_ascii_lowercase().as_str()))
}

/// What the tree and lists show.
#[derive(Debug, Clone, Default)]
pub struct Filter {
    /// Show dot files and folders.
    pub hidden: bool,
    /// Show every file, not only Markdown.
    pub all_files: bool,
    /// Custom globs (`*.md,*.txt,!node_modules`); overrides `all_files`.
    pub globs: Option<ignore::overrides::Override>,
}

impl Filter {
    /// Builds a filter; `globs` is a comma-separated list (empty: none).
    pub fn new(root: &Path, hidden: bool, all_files: bool, globs: &str) -> Self {
        let globs = globs.trim();
        let globs = (!globs.is_empty())
            .then(|| {
                let mut b = ignore::overrides::OverrideBuilder::new(root);
                for g in globs.split(',').map(str::trim).filter(|g| !g.is_empty()) {
                    if let Err(e) = b.add(g) {
                        log::warn!("bad file filter `{g}`: {e}");
                    }
                }
                b.build().ok()
            })
            .flatten();
        Self {
            hidden,
            all_files,
            globs,
        }
    }

    /// Whether an entry is shown.
    pub fn accepts(&self, path: &Path, is_dir: bool) -> bool {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy())
            .unwrap_or_default();
        if !self.hidden && name.starts_with('.') {
            return false;
        }
        if let Some(o) = &self.globs {
            return !o.matched(path, is_dir).is_ignore();
        }
        is_dir || self.all_files || is_markdown(path)
    }
}

/// One directory entry.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    /// Full path.
    pub path: PathBuf,
    /// File name.
    pub name: String,
    /// Directory.
    pub is_dir: bool,
    /// Modification time.
    pub modified: Option<SystemTime>,
    /// Creation time (where the platform reports it).
    pub created: Option<SystemTime>,
}

/// Lists `dir` (not recursive): folders first, natural order.
pub fn list_dir(dir: &Path, filter: &Filter) -> Vec<Entry> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<Entry> = rd
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            let meta = std::fs::metadata(&path).ok()?;
            let is_dir = meta.is_dir();
            filter.accepts(&path, is_dir).then(|| Entry {
                name: e.file_name().to_string_lossy().into_owned(),
                path,
                is_dir,
                modified: meta.modified().ok(),
                created: meta.created().ok(),
            })
        })
        .collect();
    out.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| natural_cmp(&a.name, &b.name))
    });
    out
}

/// A walker over `root` honouring `.gitignore` (when `gitignore`) and the filter.
pub fn walker(root: &Path, filter: &Filter, gitignore: bool, recursive: bool) -> ignore::Walk {
    let mut b = ignore::WalkBuilder::new(root);
    b.hidden(!filter.hidden)
        .git_ignore(gitignore)
        .git_global(gitignore)
        .git_exclude(gitignore)
        .ignore(gitignore)
        .parents(gitignore);
    if !recursive {
        b.max_depth(Some(1));
    }
    if let Some(o) = &filter.globs {
        b.overrides(o.clone());
    }
    b.build()
}

/// All files under `root` accepted by the filter (for the article list and the
/// quick-open index).
pub fn files_under(
    root: &Path,
    filter: &Filter,
    gitignore: bool,
    recursive: bool,
    limit: usize,
) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for e in walker(root, filter, gitignore, recursive).flatten() {
        if e.file_type().is_some_and(|t| t.is_file()) && filter.accepts(e.path(), false) {
            out.push(e.into_path());
            if out.len() >= limit {
                break;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_and_filters() {
        let d = tempfile::tempdir().unwrap();
        let r = d.path();
        std::fs::create_dir_all(r.join("sub")).unwrap();
        std::fs::create_dir_all(r.join(".git")).unwrap();
        std::fs::create_dir_all(r.join("node_modules")).unwrap();
        for f in [
            "b10.md",
            "b2.md",
            "a.txt",
            "img.png",
            ".hidden.md",
            "sub/x.md",
            "node_modules/y.md",
        ] {
            std::fs::write(r.join(f), "x").unwrap();
        }
        let f = Filter::new(r, false, false, "");
        let names: Vec<String> = list_dir(r, &f).into_iter().map(|e| e.name).collect();
        assert_eq!(names, ["node_modules", "sub", "a.txt", "b2.md", "b10.md"]);
        let f = Filter::new(r, false, false, "*.md,!node_modules");
        let names: Vec<String> = list_dir(r, &f).into_iter().map(|e| e.name).collect();
        assert_eq!(names, ["sub", "b2.md", "b10.md"]);
        let f = Filter::new(r, false, true, "");
        assert!(list_dir(r, &f).iter().any(|e| e.name == "img.png"));
        let all = files_under(
            r,
            &Filter::new(r, false, false, "*.md,*.txt,!node_modules"),
            true,
            true,
            100,
        );
        assert_eq!(all.len(), 4, "{all:?}");
    }
}
