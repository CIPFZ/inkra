//! User preferences (PR-001, PR-003).
//!
//! Stored as a flat JSON object with dotted lowerCamel keys in
//! `<config_dir>/conf.user.json`. Unknown keys are preserved on save so that newer
//! or older versions of Inkra can share one file. Writes are atomic.

use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use inkra_workspace::fs::atomic_write;
use inkra_workspace::time::Utc;

/// Kind of value a preference holds (documentation and validation).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrefKind {
    /// `true`/`false`.
    Bool,
    /// A number.
    Number,
    /// Free text.
    String,
    /// One of the listed strings.
    Enum(&'static [&'static str]),
    /// A list of strings.
    StringList,
}

/// Static description of one preference key.
#[derive(Debug, Clone, Copy)]
pub struct PrefSpec {
    /// Dotted lowerCamel key, e.g. `editor.autoPair`.
    pub key: &'static str,
    /// Default value as JSON.
    pub default: &'static str,
    /// Value kind.
    pub kind: PrefKind,
    /// Requirement id, e.g. `PR-031`.
    pub req: &'static str,
    /// One-line English description for `docs/preferences.md`.
    pub doc: &'static str,
}

/// All known preference keys. `cargo xtask prefs` renders this table to
/// `docs/preferences.md`.
pub const PREFS: &[PrefSpec] = &[
    PrefSpec {
        key: "general.language",
        default: r#""system""#,
        kind: PrefKind::String,
        req: "PR-018",
        doc: "UI language: `system` to follow the OS, or a tag such as `zh-CN` / `en-US`.",
    },
    PrefSpec {
        key: "general.openFileIn",
        default: r#""reuse""#,
        kind: PrefKind::Enum(&["reuse", "newWindow", "current"]),
        req: "PR-011",
        doc: "Opening a file: reuse a blank window (else a new one), always a new window, or the current window when it has no unsaved changes.",
    },
    PrefSpec {
        key: "general.autoSave",
        default: "false",
        kind: PrefKind::Bool,
        req: "PR-012",
        doc: "Save automatically 3 s after changes and when the window loses focus (FS-004).",
    },
    PrefSpec {
        key: "general.autoReload",
        default: "true",
        kind: PrefKind::Bool,
        req: "PR-017",
        doc: "Reload files changed on disk when there are no local changes (FS-007).",
    },
    PrefSpec {
        key: "general.startup",
        default: r#""restore""#,
        kind: PrefKind::Enum(&["restore", "new", "folder"]),
        req: "PR-010",
        doc: "On startup: restore the last session, open a new document, or open a folder.",
    },
    PrefSpec {
        key: "general.defaultEncoding",
        default: r#""utf-8""#,
        kind: PrefKind::String,
        req: "PR-016",
        doc: "Encoding tried for files that are not valid UTF-8 (e.g. `gbk`).",
    },
    PrefSpec {
        key: "editor.defaultMode",
        default: r#""live""#,
        kind: PrefKind::Enum(&["live", "source"]),
        req: "PR-030",
        doc: "Open documents in live rendering or source mode.",
    },
    PrefSpec {
        key: "appearance.theme",
        default: r#""Paper""#,
        kind: PrefKind::String,
        req: "TH-003",
        doc: "Theme name.",
    },
    PrefSpec {
        key: "appearance.themeAuto",
        default: "false",
        kind: PrefKind::Bool,
        req: "TH-002",
        doc: "Follow the system light/dark setting (Auto theme).",
    },
    PrefSpec {
        key: "appearance.lightTheme",
        default: r#""Paper""#,
        kind: PrefKind::String,
        req: "TH-002",
        doc: "Theme used by Auto in light mode.",
    },
    PrefSpec {
        key: "appearance.darkTheme",
        default: r#""Ink""#,
        kind: PrefKind::String,
        req: "TH-002",
        doc: "Theme used by Auto in dark mode.",
    },
    PrefSpec {
        key: "general.defaultNewline",
        default: r#""lf""#,
        kind: PrefKind::Enum(&["lf", "crlf"]),
        req: "PR-016",
        doc: "Line ending of new documents (existing files keep theirs).",
    },
    PrefSpec {
        key: "general.trimTrailingOnSave",
        default: r#"false"#,
        kind: PrefKind::Bool,
        req: "PR-013",
        doc: "Remove trailing whitespace when saving (hard line breaks keep two spaces).",
    },
    PrefSpec {
        key: "general.normalizeOnSave",
        default: r#"false"#,
        kind: PrefKind::Bool,
        req: "MD-092",
        doc: "Normalize when saving: pad tables, renumber ordered lists, trim trailing whitespace, end with one newline.",
    },
    PrefSpec {
        key: "general.markdownExtensions",
        default: r#"["md", "markdown", "mdown", "mkd", "mkdn", "mdwn", "mdtxt", "mdtext", "text", "txt", "rmd", "qmd"]"#,
        kind: PrefKind::StringList,
        req: "PR-015",
        doc: "File extensions treated as Markdown (file tree, open dialog, drops).",
    },
    PrefSpec {
        key: "general.checkUpdates",
        default: r#"true"#,
        kind: PrefKind::Bool,
        req: "PR-020",
        doc: "Check GitHub Releases for a newer version at startup (never downloads).",
    },
    PrefSpec {
        key: "appearance.rememberWindowSize",
        default: r#"true"#,
        kind: PrefKind::Bool,
        req: "TH-008",
        doc: "Reopen windows at their last size and position.",
    },
    PrefSpec {
        key: "sidebar.fontSize",
        default: r#"13"#,
        kind: PrefKind::Number,
        req: "TH-008",
        doc: "Sidebar font size in points.",
    },
    PrefSpec {
        key: "sidebar.lineHeight",
        default: r#"1.6"#,
        kind: PrefKind::Number,
        req: "TH-008",
        doc: "Sidebar row height as a multiple of the font size.",
    },
    PrefSpec {
        key: "editor.firstLineIndent",
        default: r#"false"#,
        kind: PrefKind::Bool,
        req: "PR-034",
        doc: "Indent the first line of each paragraph by two characters.",
    },
    PrefSpec {
        key: "editor.copyAs",
        default: r#""markdown""#,
        kind: PrefKind::Enum(&["markdown", "plain"]),
        req: "PR-037",
        doc: "What Copy puts in text/plain: the Markdown source or plain text.",
    },
    PrefSpec {
        key: "editor.ctrlWheelZoom",
        default: r#"true"#,
        kind: PrefKind::Bool,
        req: "PR-039",
        doc: "Ctrl + mouse wheel zooms the document.",
    },
    PrefSpec {
        key: "editor.linkClick",
        default: r#""ctrlClick""#,
        kind: PrefKind::Enum(&["ctrlClick", "click"]),
        req: "PR-040",
        doc: "Open links with Ctrl+click or a plain click.",
    },
    PrefSpec {
        key: "editor.cursorBlink",
        default: r#"true"#,
        kind: PrefKind::Bool,
        req: "PR-042",
        doc: "Blink the caret.",
    },
    PrefSpec {
        key: "editor.smoothScroll",
        default: r#"true"#,
        kind: PrefKind::Bool,
        req: "PR-042",
        doc: "Animate scrolling to a position (outline, search hits).",
    },
    PrefSpec {
        key: "editor.spellcheckDictionary",
        default: r#""""#,
        kind: PrefKind::String,
        req: "PR-035",
        doc: "Personal dictionary file (empty: `dictionary.txt` in the configuration folder).",
    },
    PrefSpec {
        key: "view.wordCount",
        default: r#"true"#,
        kind: PrefKind::Bool,
        req: "PR-043",
        doc: "Show the word count in the status bar.",
    },
    PrefSpec {
        key: "image.maxWidthFollowsEditor",
        default: r#"true"#,
        kind: PrefKind::Bool,
        req: "TH-008",
        doc: "Limit image width to the text column (otherwise images may extend into the margins up to the window).",
    },
    PrefSpec {
        key: "image.showTitle",
        default: r#"false"#,
        kind: PrefKind::Bool,
        req: "TH-008",
        doc: "Show an image's title under it as a caption.",
    },
    PrefSpec {
        key: "markdown.codeLineNumbers",
        default: r#"false"#,
        kind: PrefKind::Bool,
        req: "MD-071",
        doc: "Show line numbers in code blocks.",
    },
    PrefSpec {
        key: "markdown.codeWrap",
        default: r#"false"#,
        kind: PrefKind::Bool,
        req: "MD-072",
        doc: "Wrap long lines in code blocks instead of scrolling them.",
    },
    PrefSpec {
        key: "markdown.preserveBlankLines",
        default: r#"false"#,
        kind: PrefKind::Bool,
        req: "MD-074",
        doc: "Show runs of blank lines as extra space (exported as `<br>`).",
    },
    PrefSpec {
        key: "image.loadRemote",
        default: r#"true"#,
        kind: PrefKind::Bool,
        req: "NF-004",
        doc: "Load images from http(s) URLs (the only network access besides the update check).",
    },
    PrefSpec {
        key: "image.insertAction",
        default: r#""assets""#,
        kind: PrefKind::Enum(&["assets", "folder", "none"]),
        req: "ED-092",
        doc: "Inserted images: copy to `./<name>.assets/`, copy to `image.copyFolder`, or keep the original path.",
    },
    PrefSpec {
        key: "image.copyFolder",
        default: r#""""#,
        kind: PrefKind::String,
        req: "ED-092",
        doc: "Target folder pattern for `folder` (supports `${filename}`, `${filepath}`).",
    },
    PrefSpec {
        key: "image.applyToLocal",
        default: "false",
        kind: PrefKind::Bool,
        req: "ED-092",
        doc: "Also copy inserted local image files (pasted bitmaps are always saved).",
    },
    PrefSpec {
        key: "image.preferRelative",
        default: "true",
        kind: PrefKind::Bool,
        req: "ED-092",
        doc: "Insert image paths relative to the document.",
    },
    PrefSpec {
        key: "markdown.tableAlign",
        default: "true",
        kind: PrefKind::Bool,
        req: "ED-085",
        doc: "Align table pipes with spaces when tables are rewritten.",
    },
    PrefSpec {
        key: "general.defaultExtension",
        default: r#""md""#,
        kind: PrefKind::Enum(&["md", "markdown", "txt"]),
        req: "PR-014",
        doc: "Extension of new files.",
    },
    PrefSpec {
        key: "sidebar.outlineDepth",
        default: "6",
        kind: PrefKind::Number,
        req: "FS-020",
        doc: "Deepest heading level shown in the outline.",
    },
    PrefSpec {
        key: "sidebar.articlesRecursive",
        default: "true",
        kind: PrefKind::Bool,
        req: "FS-021",
        doc: "The article list includes subfolders.",
    },
    PrefSpec {
        key: "sidebar.articlesSort",
        default: r#""name""#,
        kind: PrefKind::Enum(&["name", "modified", "created"]),
        req: "FS-021",
        doc: "Article list order.",
    },
    PrefSpec {
        key: "sidebar.articlesSortDesc",
        default: "false",
        kind: PrefKind::Bool,
        req: "FS-021",
        doc: "Article list in descending order.",
    },
    PrefSpec {
        key: "sidebar.showHidden",
        default: "false",
        kind: PrefKind::Bool,
        req: "FS-022",
        doc: "Show hidden files and folders.",
    },
    PrefSpec {
        key: "sidebar.showAllFiles",
        default: "false",
        kind: PrefKind::Bool,
        req: "FS-022",
        doc: "Show all files, not only Markdown.",
    },
    PrefSpec {
        key: "sidebar.fileFilter",
        default: r#""""#,
        kind: PrefKind::String,
        req: "FS-022",
        doc: "Custom glob filter, e.g. `*.md,*.txt,!node_modules`.",
    },
    PrefSpec {
        key: "sidebar.revealCurrent",
        default: "true",
        kind: PrefKind::Bool,
        req: "FS-023",
        doc: "Expand the file tree to the opened file.",
    },
    PrefSpec {
        key: "search.gitignore",
        default: "true",
        kind: PrefKind::Bool,
        req: "FS-031",
        doc: "Global search respects `.gitignore`.",
    },
    PrefSpec {
        key: "view.statusBar",
        default: "true",
        kind: PrefKind::Bool,
        req: "PR-043",
        doc: "Show the status bar.",
    },
    PrefSpec {
        key: "view.menuBar",
        default: "true",
        kind: PrefKind::Bool,
        req: "TH-008",
        doc: "Show the menu bar (Windows/Linux; Alt shows it temporarily).",
    },
    PrefSpec {
        key: "editor.focusMode",
        default: "false",
        kind: PrefKind::Bool,
        req: "PR-038",
        doc: "Focus mode (remembered).",
    },
    PrefSpec {
        key: "editor.typewriterMode",
        default: "false",
        kind: PrefKind::Bool,
        req: "PR-038",
        doc: "Typewriter mode (remembered).",
    },
    PrefSpec {
        key: "editor.rememberZoom",
        default: "true",
        kind: PrefKind::Bool,
        req: "PR-039",
        doc: "Remember the editor zoom (Ctrl+wheel, Ctrl+Shift+=/-).",
    },
    PrefSpec {
        key: "editor.zoom",
        default: "1.0",
        kind: PrefKind::Number,
        req: "PR-039",
        doc: "Editor zoom factor, 0.5 to 3.0.",
    },
    PrefSpec {
        key: "export.theme",
        default: r#""""#,
        kind: PrefKind::String,
        req: "EX-005",
        doc: "Theme used for exports (empty: the current theme).",
    },
    PrefSpec {
        key: "export.openAfter",
        default: r#"true"#,
        kind: PrefKind::Bool,
        req: "EX-002",
        doc: "Open the exported file when done.",
    },
    PrefSpec {
        key: "export.pdf.paper",
        default: r#""A4""#,
        kind: PrefKind::Enum(&["A4", "A3", "A5", "B5", "Letter", "Legal", "Custom"]),
        req: "EX-002",
        doc: "Paper size for PDF export and printing.",
    },
    PrefSpec {
        key: "export.pdf.customWidth",
        default: r#"210"#,
        kind: PrefKind::Number,
        req: "EX-002",
        doc: "Custom paper width in millimetres.",
    },
    PrefSpec {
        key: "export.pdf.customHeight",
        default: r#"297"#,
        kind: PrefKind::Number,
        req: "EX-002",
        doc: "Custom paper height in millimetres.",
    },
    PrefSpec {
        key: "export.pdf.orientation",
        default: r#""portrait""#,
        kind: PrefKind::Enum(&["portrait", "landscape"]),
        req: "EX-002",
        doc: "Page orientation.",
    },
    PrefSpec {
        key: "export.pdf.marginTop",
        default: r#"20"#,
        kind: PrefKind::Number,
        req: "EX-002",
        doc: "Top margin in millimetres.",
    },
    PrefSpec {
        key: "export.pdf.marginRight",
        default: r#"18"#,
        kind: PrefKind::Number,
        req: "EX-002",
        doc: "Right margin in millimetres.",
    },
    PrefSpec {
        key: "export.pdf.marginBottom",
        default: r#"20"#,
        kind: PrefKind::Number,
        req: "EX-002",
        doc: "Bottom margin in millimetres.",
    },
    PrefSpec {
        key: "export.pdf.marginLeft",
        default: r#"18"#,
        kind: PrefKind::Number,
        req: "EX-002",
        doc: "Left margin in millimetres.",
    },
    PrefSpec {
        key: "export.pdf.header",
        default: r#""""#,
        kind: PrefKind::String,
        req: "EX-002",
        doc: "Page header template: `{{title}}`, `{{page}}`, `{{pages}}`, `{{date}}`.",
    },
    PrefSpec {
        key: "export.pdf.footer",
        default: r#""{{page}} / {{pages}}""#,
        kind: PrefKind::String,
        req: "EX-002",
        doc: "Page footer template.",
    },
    PrefSpec {
        key: "export.pdf.background",
        default: r#"false"#,
        kind: PrefKind::Bool,
        req: "EX-002",
        doc: "Paint the theme's background on PDF pages (otherwise white pages and, for dark themes, the light default theme).",
    },
    PrefSpec {
        key: "export.html.images",
        default: r#""keep""#,
        kind: PrefKind::Enum(&["keep", "absolute", "embed"]),
        req: "EX-001",
        doc: "Image paths in HTML: as written, absolute file URLs, or embedded as base64.",
    },
    PrefSpec {
        key: "export.html.math",
        default: r#""svg""#,
        kind: PrefKind::Enum(&["svg", "tex"]),
        req: "EX-001",
        doc: "Formulas in HTML: inline SVG, or TeX in `$$` for MathJax/KaTeX pages.",
    },
    PrefSpec {
        key: "export.html.highlight",
        default: r#"true"#,
        kind: PrefKind::Bool,
        req: "EX-001",
        doc: "Syntax-highlight code blocks in HTML.",
    },
    PrefSpec {
        key: "export.html.softBreakAsBr",
        default: r#"false"#,
        kind: PrefKind::Bool,
        req: "MD-079",
        doc: "Export soft line breaks as `<br>`.",
    },
    PrefSpec {
        key: "export.html.head",
        default: r#""""#,
        kind: PrefKind::String,
        req: "EX-001",
        doc: "Extra HTML appended to `<head>` of exported HTML.",
    },
    PrefSpec {
        key: "export.html.foot",
        default: r#""""#,
        kind: PrefKind::String,
        req: "EX-001",
        doc: "Extra HTML before `</body>` of exported HTML.",
    },
    PrefSpec {
        key: "export.html.css",
        default: r#""""#,
        kind: PrefKind::String,
        req: "EX-001",
        doc: "Extra CSS for exported HTML.",
    },
    PrefSpec {
        key: "export.image.scale",
        default: r#"2"#,
        kind: PrefKind::Number,
        req: "EX-003",
        doc: "Resolution multiplier for image export (1–3).",
    },
    PrefSpec {
        key: "export.image.paged",
        default: r#"false"#,
        kind: PrefKind::Bool,
        req: "EX-003",
        doc: "Export one image per page instead of one long image.",
    },
    PrefSpec {
        key: "export.pandocPath",
        default: r#""""#,
        kind: PrefKind::String,
        req: "EX-004",
        doc: "Pandoc executable (empty: search PATH).",
    },
    PrefSpec {
        key: "export.custom",
        default: r#"[]"#,
        kind: PrefKind::StringList,
        req: "EX-004",
        doc: "Custom export commands, each `Label|extension|command template` with `$input`, `$output` and `$outputDir`, e.g. `Word (reference)|docx|pandoc $input -o $output --reference-doc ref.docx`.",
    },
    PrefSpec {
        key: "export.pandocArgs",
        default: r#""""#,
        kind: PrefKind::String,
        req: "EX-004",
        doc: "Extra Pandoc arguments, e.g. `--reference-doc ref.docx`.",
    },
    PrefSpec {
        key: "markdown.mermaidCli",
        default: r#""""#,
        kind: PrefKind::String,
        req: "MD-100",
        doc: "Path of the Mermaid CLI (`mmdc`) used for diagram types Inkra does not draw itself (empty: off).",
    },
    PrefSpec {
        key: "markdown.diagrams",
        default: "true",
        kind: PrefKind::Bool,
        req: "MD-064",
        doc: "Render ```` ```mermaid ```` blocks as diagrams (off: plain code blocks).",
    },
    PrefSpec {
        key: "editor.emojiCompletion",
        default: "true",
        kind: PrefKind::Bool,
        req: "PR-036",
        doc: "Typing `:` and two letters suggests emoji (ED-103).",
    },
    PrefSpec {
        key: "editor.emojiAsShortcode",
        default: "false",
        kind: PrefKind::Bool,
        req: "ED-103",
        doc: "Insert `:shortcode:` text instead of the emoji character.",
    },
    PrefSpec {
        key: "editor.linkCompletion",
        default: "true",
        kind: PrefKind::Bool,
        req: "PR-036",
        doc: "Typing `[` suggests the document's headings, `[^` its footnotes (ED-105).",
    },
    PrefSpec {
        key: "editor.autoPairBrackets",
        default: "true",
        kind: PrefKind::Bool,
        req: "PR-031",
        doc: "Auto-pair brackets and quotes.",
    },
    PrefSpec {
        key: "editor.autoPairMarkdown",
        default: "false",
        kind: PrefKind::Bool,
        req: "PR-032",
        doc: "Auto-pair Markdown symbols `*` `_` `~` `=`.",
    },
    PrefSpec {
        key: "editor.tabWidth",
        default: "4",
        kind: PrefKind::Number,
        req: "PR-041",
        doc: "Tab width in code blocks.",
    },
    PrefSpec {
        key: "editor.useSpaces",
        default: "true",
        kind: PrefKind::Bool,
        req: "PR-041",
        doc: "Indent code blocks with spaces.",
    },
    PrefSpec {
        key: "markdown.renumberLists",
        default: "true",
        kind: PrefKind::Bool,
        req: "MD-076",
        doc: "Renumber ordered lists after edits.",
    },
    PrefSpec {
        key: "markdown.listFromOne",
        default: "false",
        kind: PrefKind::Bool,
        req: "ED-101",
        doc: "Ordered lists always start at 1 (otherwise the start number is kept).",
    },
    PrefSpec {
        key: "markdown.smartPunctuation",
        default: "false",
        kind: PrefKind::Bool,
        req: "MD-065",
        doc: "Smart punctuation while typing (ED-102).",
    },
    PrefSpec {
        key: "markdown.smartQuotes",
        default: r#""curly""#,
        kind: PrefKind::Enum(&["curly", "corner", "guillemets", "off"]),
        req: "MD-066",
        doc: "Quote style of smart punctuation: “” ‘’, 「」『』 or «» ‹›.",
    },
    PrefSpec {
        key: "markdown.smartDashes",
        default: "true",
        kind: PrefKind::Bool,
        req: "MD-066",
        doc: "`--` becomes an en dash, `---` an em dash.",
    },
    PrefSpec {
        key: "markdown.smartEllipsis",
        default: "true",
        kind: PrefKind::Bool,
        req: "MD-066",
        doc: "`...` becomes an ellipsis.",
    },
    PrefSpec {
        key: "editor.spellcheck",
        default: "true",
        kind: PrefKind::Bool,
        req: "PR-035",
        doc: "Underline misspelled English words (ED-104).",
    },
    PrefSpec {
        key: "markdown.tocDepth",
        default: "6",
        kind: PrefKind::Number,
        req: "MD-078",
        doc: "Deepest heading level listed by `[TOC]`.",
    },
    PrefSpec {
        key: "markdown.mathNumbering",
        default: r#""none""#,
        kind: PrefKind::Enum(&["none", "ams", "all"]),
        req: "MD-068",
        doc: "Number display formulas: off, AMS environments (`equation`, `align`), or every block.",
    },
    PrefSpec {
        key: "markdown.inlineMath",
        default: "false",
        kind: PrefKind::Bool,
        req: "MD-060",
        doc: "Inline math `$...$`.",
    },
    PrefSpec {
        key: "markdown.subscript",
        default: "false",
        kind: PrefKind::Bool,
        req: "MD-061",
        doc: "Subscript `~x~`.",
    },
    PrefSpec {
        key: "markdown.superscript",
        default: "false",
        kind: PrefKind::Bool,
        req: "MD-062",
        doc: "Superscript `^x^`.",
    },
    PrefSpec {
        key: "markdown.highlight",
        default: "false",
        kind: PrefKind::Bool,
        req: "MD-063",
        doc: "Highlight `==x==`.",
    },
    PrefSpec {
        key: "markdown.strictMode",
        default: "false",
        kind: PrefKind::Bool,
        req: "MD-067",
        doc: "Strict CommonMark headings (`# ` needs a space).",
    },
    PrefSpec {
        key: "markdown.latexDelimiters",
        default: "false",
        kind: PrefKind::Bool,
        req: "MD-069",
        doc: "Math delimiters `\\[ \\]` and `\\( \\)`.",
    },
    PrefSpec {
        key: "markdown.looseInlineMath",
        default: "false",
        kind: PrefKind::Bool,
        req: "MD-070",
        doc: "Allow spaces next to inline `$` delimiters.",
    },
    PrefSpec {
        key: "markdown.strongMarker",
        default: r#""**""#,
        kind: PrefKind::Enum(&["**", "__"]),
        req: "MD-075",
        doc: "Marker used when commands create bold text.",
    },
    PrefSpec {
        key: "markdown.emphasisMarker",
        default: r#""*""#,
        kind: PrefKind::Enum(&["*", "_"]),
        req: "MD-075",
        doc: "Marker used when commands create italic text.",
    },
    PrefSpec {
        key: "markdown.bulletMarker",
        default: r#""-""#,
        kind: PrefKind::Enum(&["-", "*", "+"]),
        req: "MD-075",
        doc: "Marker used when commands create bullet lists.",
    },
    PrefSpec {
        key: "markdown.codeDefaultLanguage",
        default: r#""""#,
        kind: PrefKind::String,
        req: "MD-073",
        doc: "Language pre-filled in new code blocks.",
    },
];

/// Looks up the spec of `key`.
pub fn spec(key: &str) -> Option<&'static PrefSpec> {
    PREFS.iter().find(|p| p.key == key)
}

fn default_of(key: &str) -> Value {
    spec(key)
        .and_then(|s| serde_json::from_str(s.default).ok())
        .unwrap_or(Value::Null)
}

/// Whether `v` fits `kind`.
fn fits(kind: PrefKind, v: &Value) -> bool {
    match kind {
        PrefKind::Bool => v.is_boolean(),
        PrefKind::Number => v.is_number(),
        PrefKind::String => v.is_string(),
        PrefKind::Enum(options) => v.as_str().is_some_and(|s| options.contains(&s)),
        PrefKind::StringList => v.as_array().is_some_and(|a| a.iter().all(Value::is_string)),
    }
}

/// Problems found while loading preferences.
#[derive(Debug, Clone, PartialEq)]
pub enum LoadProblem {
    /// The file was not valid JSON (or not an object). It was moved to `backup` and
    /// defaults are used (PR-003).
    Corrupt {
        /// Where the broken file was moved.
        backup: PathBuf,
        /// Parse error.
        error: String,
    },
    /// The file could not be read.
    Unreadable {
        /// I/O error.
        error: String,
    },
}

/// The preference store.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Preferences {
    values: Map<String, Value>,
}

impl Preferences {
    /// Defaults only.
    pub fn new() -> Self {
        Self::default()
    }

    /// Parses a JSON object.
    pub fn from_json(text: &str) -> Result<Self, String> {
        match serde_json::from_str::<Value>(text) {
            Ok(Value::Object(values)) => Ok(Self { values }),
            Ok(_) => Err("top-level value is not an object".to_owned()),
            Err(e) => Err(e.to_string()),
        }
    }

    /// Loads `path`. A missing file yields defaults; a corrupt file is backed up as
    /// `conf.user.json.bak-<YYYYMMDDHHmmss>` and defaults are returned together with
    /// a [`LoadProblem`] so the UI can tell the user.
    pub fn load(path: &Path) -> (Self, Option<LoadProblem>) {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return (Self::new(), None),
            Err(e) => {
                log::error!("cannot read preferences {}: {e}", path.display());
                return (
                    Self::new(),
                    Some(LoadProblem::Unreadable {
                        error: e.to_string(),
                    }),
                );
            }
        };
        // Tolerate a UTF-8 BOM written by some editors.
        let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
        if text.trim().is_empty() {
            return (Self::new(), None);
        }
        match Self::from_json(text) {
            Ok(p) => (p, None),
            Err(error) => {
                let backup = backup_path(path);
                log::warn!(
                    "preferences {} are corrupt ({error}); backing up to {}",
                    path.display(),
                    backup.display()
                );
                if let Err(e) = std::fs::rename(path, &backup) {
                    log::error!("cannot back up corrupt preferences: {e}");
                }
                (Self::new(), Some(LoadProblem::Corrupt { backup, error }))
            }
        }
    }

    /// Writes the store atomically as pretty JSON.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let mut text = serde_json::to_string_pretty(&self.values).map_err(std::io::Error::other)?;
        text.push('\n');
        atomic_write(path, text.as_bytes())
    }

    /// Value of `key`: the stored value if it has the right kind, else the default.
    pub fn get(&self, key: &str) -> Value {
        match (self.values.get(key), spec(key)) {
            (Some(v), Some(s)) if fits(s.kind, v) => v.clone(),
            (Some(v), None) => v.clone(),
            (Some(v), Some(_)) => {
                log::warn!("preference `{key}` has invalid value {v}; using default");
                default_of(key)
            }
            (None, _) => default_of(key),
        }
    }

    /// Boolean value of `key` (`false` if not a boolean).
    pub fn bool(&self, key: &str) -> bool {
        self.get(key).as_bool().unwrap_or(false)
    }

    /// Numeric value of `key` (`0.0` if not a number).
    pub fn f64(&self, key: &str) -> f64 {
        self.get(key).as_f64().unwrap_or(0.0)
    }

    /// String value of `key` (empty if not a string).
    pub fn string(&self, key: &str) -> String {
        self.get(key).as_str().unwrap_or_default().to_owned()
    }

    /// String list value of `key`.
    pub fn strings(&self, key: &str) -> Vec<String> {
        self.get(key)
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Sets `key`. Setting a known key to its default removes it from the file so
    /// that future default changes apply. Returns `false` if the value has the wrong
    /// kind for a known key.
    pub fn set(&mut self, key: &str, value: Value) -> bool {
        if let Some(s) = spec(key) {
            if !fits(s.kind, &value) {
                return false;
            }
            if value == default_of(key) {
                self.values.remove(key);
                return true;
            }
        }
        self.values.insert(key.to_owned(), value);
        true
    }

    /// Raw stored entries, including unknown keys.
    pub fn raw(&self) -> &Map<String, Value> {
        &self.values
    }

    /// Forgets every known key (PR-021 "reset settings"); unknown keys stay.
    pub fn reset(&mut self) {
        self.values.retain(|k, _| spec(k).is_none());
    }

    /// Merges a settings file (PR-002): known keys with valid values replace
    /// the current ones, unknown keys are kept. Returns the keys rejected.
    pub fn import(&mut self, text: &str) -> Result<Vec<String>, String> {
        let other = Self::from_json(text)?;
        let mut rejected = Vec::new();
        for (k, v) in other.values {
            if !self.set(&k, v) {
                rejected.push(k);
            }
        }
        Ok(rejected)
    }
}

fn backup_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let stamp = Utc::now().compact();
    let mut candidate = path.with_file_name(format!("{name}.bak-{stamp}"));
    let mut n = 1;
    while candidate.exists() {
        candidate = path.with_file_name(format!("{name}.bak-{stamp}-{n}"));
        n += 1;
    }
    candidate
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_valid_json_of_the_right_kind() {
        let mut keys = std::collections::HashSet::new();
        for p in PREFS {
            let v: Value =
                serde_json::from_str(p.default).unwrap_or_else(|e| panic!("{}: {e}", p.key));
            assert!(fits(p.kind, &v), "{} default does not fit its kind", p.key);
            assert!(keys.insert(p.key), "duplicate key {}", p.key);
            assert!(
                crate::command::is_valid_id(p.key) || !p.key.contains('.'),
                "{}",
                p.key
            );
        }
    }

    #[test]
    fn round_trip_preserves_unknown_keys() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("conf.user.json");
        std::fs::write(
            &path,
            r#"{"future.key": [1, 2], "general.language": "zh-CN"}"#,
        )
        .unwrap();
        let (mut p, problem) = Preferences::load(&path);
        assert!(problem.is_none());
        assert_eq!(p.string("general.language"), "zh-CN");
        assert!(p.set("general.language", Value::from("en-US")));
        p.save(&path).unwrap();
        let (q, _) = Preferences::load(&path);
        assert_eq!(q.raw().get("future.key"), Some(&serde_json::json!([1, 2])));
        assert_eq!(q.string("general.language"), "en-US");
    }

    #[test]
    fn corrupt_file_is_backed_up() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("conf.user.json");
        std::fs::write(&path, "{ not json").unwrap();
        let (p, problem) = Preferences::load(&path);
        assert_eq!(p, Preferences::new());
        let Some(LoadProblem::Corrupt { backup, .. }) = problem else {
            panic!("{problem:?}")
        };
        assert!(
            backup
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("conf.user.json.bak-")
        );
        assert_eq!(std::fs::read_to_string(backup).unwrap(), "{ not json");
        assert!(!path.exists());
    }

    #[test]
    fn missing_file_and_wrong_types() {
        let dir = tempfile::tempdir().unwrap();
        let (mut p, problem) = Preferences::load(&dir.path().join("nope.json"));
        assert!(problem.is_none());
        assert_eq!(p.string("general.language"), "system");
        assert!(!p.set("general.language", Value::from(3)));
        let q = Preferences::from_json(r#"{"general.language": 3}"#).unwrap();
        assert_eq!(q.string("general.language"), "system");
        assert!(Preferences::from_json("[1]").is_err());
    }

    #[test]
    fn setting_default_removes_key() {
        let mut p = Preferences::new();
        p.set("general.language", Value::from("zh-CN"));
        assert!(p.raw().contains_key("general.language"));
        p.set("general.language", Value::from("system"));
        assert!(!p.raw().contains_key("general.language"));
    }
}
