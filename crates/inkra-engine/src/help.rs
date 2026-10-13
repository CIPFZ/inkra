//! Bundled help documents (SPEC 13 M7: quick start, Markdown reference, theme
//! guide), opened as ordinary documents from a copy in the cache folder so
//! edits never touch anything that matters.

use std::path::PathBuf;

use crate::command::{CommandError, CommandResult};
use crate::state::CommandContext;

/// A help document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Topic {
    /// Quick start.
    QuickStart,
    /// Markdown reference.
    MarkdownReference,
    /// Custom themes.
    Themes,
    /// Commands and shortcuts.
    Shortcuts,
    /// Change log.
    Changelog,
    /// Third-party licenses.
    ThirdParty,
}

impl Topic {
    /// File name and text for `language`.
    pub fn document(self, language: &str) -> (&'static str, &'static str) {
        let zh = language.starts_with("zh");
        match (self, zh) {
            (Self::QuickStart, true) => (
                "快速入门.md",
                include_str!("../../../docs/user/zh-CN/quick-start.md"),
            ),
            (Self::QuickStart, false) => (
                "Quick Start.md",
                include_str!("../../../docs/user/en-US/quick-start.md"),
            ),
            (Self::MarkdownReference, true) => (
                "Markdown 参考.md",
                include_str!("../../../docs/user/zh-CN/markdown-reference.md"),
            ),
            (Self::MarkdownReference, false) => (
                "Markdown Reference.md",
                include_str!("../../../docs/user/en-US/markdown-reference.md"),
            ),
            (Self::Themes, true) => (
                "自定义主题.md",
                include_str!("../../../docs/user/zh-CN/themes.md"),
            ),
            (Self::Themes, false) => (
                "Custom Themes.md",
                include_str!("../../../docs/user/en-US/themes.md"),
            ),
            (Self::Shortcuts, _) => (
                "Keyboard Shortcuts.md",
                include_str!("../../../docs/keybindings.md"),
            ),
            (Self::Changelog, _) => ("Changelog.md", include_str!("../../../CHANGELOG.md")),
            (Self::ThirdParty, _) => (
                "Third-party Software.md",
                include_str!("../../../docs/third-party.md"),
            ),
        }
    }
}

fn help_dir(cx: &CommandContext<'_>) -> PathBuf {
    cx.app.paths.as_ref().map_or_else(
        || std::env::temp_dir().join("inkra-help"),
        |p| p.cache.join("help"),
    )
}

/// Writes the topic to the help folder (fresh each time) and opens it.
pub fn open(cx: &mut CommandContext<'_>, topic: Topic) -> CommandResult {
    let (name, text) = topic.document(&cx.app.language);
    let dir = help_dir(cx);
    let path = dir.join(name);
    let fail = |e: std::io::Error| CommandError::Failed {
        id: "help".into(),
        msg: format!("{}: {e}", path.display()),
    };
    std::fs::create_dir_all(&dir).map_err(fail)?;
    // An open, edited copy is not overwritten.
    let edited = cx
        .app
        .editors()
        .any(|(_, e)| e.path.as_deref() == Some(path.as_path()) && e.doc.is_dirty());
    if !edited {
        std::fs::write(&path, text).map_err(fail)?;
    }
    crate::files::open_path(cx, &path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn help_documents_open() {
        let d = tempfile::tempdir().unwrap();
        let mut e = crate::Engine::new(crate::Preferences::default());
        e.state.paths = Some(inkra_workspace::Paths::rooted(d.path()));
        e.state.language = "zh-CN".into();
        e.execute("help.quickStart", &json!({})).unwrap();
        let text = e
            .state
            .editors()
            .map(|(_, ed)| ed.text())
            .find(|t| t.starts_with("# Inkra 快速入门"));
        assert!(text.is_some());
        for t in [
            Topic::QuickStart,
            Topic::MarkdownReference,
            Topic::Themes,
            Topic::Shortcuts,
            Topic::Changelog,
            Topic::ThirdParty,
        ] {
            for lang in ["zh-CN", "en-US"] {
                assert!(
                    t.document(lang).1.starts_with('#') || t.document(lang).1.starts_with("<!--")
                );
            }
        }
    }
}
