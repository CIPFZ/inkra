//! The Markdown AST. Every node carries exact byte ranges into the source,
//! including the ranges of the syntax markers themselves (AD-1).
//!
//! The source text is the only truth (AD-2): the AST is a derived cache and is
//! never serialized back wholesale. Edits are made to the source; the parser
//! re-derives the affected blocks.

use std::ops::Range;

/// A byte range in the document.
pub type Span = Range<usize>;

/// One content line of a leaf block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    /// Source range of the content (container prefixes and stripped indentation
    /// excluded, line terminator excluded).
    pub span: Span,
    /// Virtual spaces prepended to the content, from a tab that was only partially
    /// consumed by indentation (CommonMark tab handling).
    pub pad: u8,
}

/// Column alignment of a table column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Align {
    /// No colon.
    #[default]
    None,
    /// `:---`.
    Left,
    /// `:---:`.
    Center,
    /// `---:`.
    Right,
}

/// GitHub alert type (MD-004).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AlertKind {
    /// `[!NOTE]`.
    Note,
    /// `[!TIP]`.
    Tip,
    /// `[!IMPORTANT]`.
    Important,
    /// `[!WARNING]`.
    Warning,
    /// `[!CAUTION]`.
    Caution,
}

impl AlertKind {
    /// Lower-case name (`note`, `tip`, …).
    pub fn name(self) -> &'static str {
        match self {
            Self::Note => "note",
            Self::Tip => "tip",
            Self::Important => "important",
            Self::Warning => "warning",
            Self::Caution => "caution",
        }
    }

    /// Parses a name case-insensitively.
    pub fn from_name(s: &str) -> Option<Self> {
        Some(match s.to_ascii_lowercase().as_str() {
            "note" => Self::Note,
            "tip" => Self::Tip,
            "important" => Self::Important,
            "warning" => Self::Warning,
            "caution" => Self::Caution,
            _ => return None,
        })
    }
}

/// Code fence description.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fence {
    /// `` ` `` or `~`.
    pub ch: char,
    /// Number of fence characters.
    pub len: usize,
    /// Indentation of the opening fence (removed from content lines).
    pub indent: usize,
    /// Opening fence line (fence characters and info string).
    pub open: Span,
    /// Info string (trimmed), if any.
    pub info: Option<Span>,
    /// Closing fence line, if the block is closed.
    pub close: Option<Span>,
}

/// List marker kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListKind {
    /// `-`, `*` or `+`.
    Bullet(char),
    /// `1.` or `1)`: delimiter char.
    Ordered(char),
}

/// Task checkbox of a list item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    /// Whether checked (`[x]` / `[X]`).
    pub checked: bool,
    /// Range of `[ ]` / `[x]`.
    pub span: Span,
}

/// Kinds of block nodes.
#[derive(Debug, Clone, PartialEq)]
pub enum BlockKind {
    /// Paragraph; inline content in `lines` / `inlines`.
    Paragraph,
    /// ATX or setext heading.
    Heading {
        /// 1..=6.
        level: u8,
        /// Setext (`===` / `---` underline) rather than ATX.
        setext: bool,
        /// ATX: the opening `#`s; setext: the underline.
        marker: Span,
        /// ATX closing sequence (` ###`), if present.
        closing: Option<Span>,
    },
    /// `---`, `***`, `___`.
    ThematicBreak,
    /// Fenced (or, with `indented_code`, indented) code block; code lines in `lines`.
    CodeBlock {
        /// Fence data; `None` for indented code.
        fence: Option<Fence>,
    },
    /// Raw HTML block (CommonMark types 1–7); raw lines in `lines`.
    HtmlBlock {
        /// CommonMark HTML block type.
        kind: u8,
    },
    /// Link reference definition `[label]: dest "title"`.
    LinkDef {
        /// Normalized label (case-folded, whitespace collapsed).
        label: String,
        /// Destination (unescaped).
        dest: String,
        /// Title (unescaped).
        title: Option<String>,
    },
    /// `>` block quote; `alert` set for GitHub alerts.
    BlockQuote {
        /// Alert type and the `[!TYPE]` marker range.
        alert: Option<(AlertKind, Span)>,
    },
    /// List container of `ListItem`s.
    List {
        /// Marker kind.
        kind: ListKind,
        /// Start number of ordered lists.
        start: u64,
        /// Tight (no blank lines between items).
        tight: bool,
    },
    /// List item.
    ListItem {
        /// The marker (`-`, `1.`).
        marker: Span,
        /// Start number of an ordered item.
        number: Option<u64>,
        /// Task checkbox.
        task: Option<Task>,
        /// Column where item content starts (marker offset + padding).
        content_offset: usize,
    },
    /// GFM table; children are `TableRow`s (first is the header).
    Table {
        /// Column alignments (from the delimiter row).
        aligns: Vec<Align>,
        /// The delimiter row.
        delimiter: Span,
    },
    /// Table row; children are `TableCell`s.
    TableRow {
        /// Header row.
        header: bool,
    },
    /// Table cell; content in `lines` (one line) / `inlines`.
    TableCell,
    /// YAML front matter between `---` lines; raw lines in `lines`.
    FrontMatter {
        /// Opening `---` line.
        open: Span,
        /// Closing `---` / `...` line.
        close: Span,
    },
    /// `$$ … $$` math block; TeX lines in `lines`.
    MathBlock {
        /// Opening delimiter.
        open: Span,
        /// Closing delimiter, if closed.
        close: Option<Span>,
    },
    /// Footnote definition `[^label]:`; children are blocks.
    FootnoteDef {
        /// Normalized label.
        label: String,
        /// Range of `[^label]:`.
        marker: Span,
    },
    /// `[TOC]` paragraph.
    Toc,
}

impl BlockKind {
    /// Whether the block has inline content (`inlines`).
    pub fn has_inlines(&self) -> bool {
        matches!(
            self,
            Self::Paragraph | Self::Heading { .. } | Self::TableCell
        )
    }

    /// Short name for debugging and `inspect` output.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Paragraph => "paragraph",
            Self::Heading { .. } => "heading",
            Self::ThematicBreak => "thematic_break",
            Self::CodeBlock { .. } => "code_block",
            Self::HtmlBlock { .. } => "html_block",
            Self::LinkDef { .. } => "link_def",
            Self::BlockQuote { .. } => "block_quote",
            Self::List { .. } => "list",
            Self::ListItem { .. } => "list_item",
            Self::Table { .. } => "table",
            Self::TableRow { .. } => "table_row",
            Self::TableCell => "table_cell",
            Self::FrontMatter { .. } => "front_matter",
            Self::MathBlock { .. } => "math_block",
            Self::FootnoteDef { .. } => "footnote_def",
            Self::Toc => "toc",
        }
    }
}

/// A block node.
#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    /// Kind and kind-specific data.
    pub kind: BlockKind,
    /// Full source range: from the block's first character (for top-level blocks,
    /// the start of its first line) to the end of its last line (terminator excluded).
    pub span: Span,
    /// Child blocks of containers.
    pub children: Vec<Block>,
    /// Content lines of leaf blocks.
    pub lines: Vec<Line>,
    /// Inline content of paragraphs, headings and table cells.
    pub inlines: Vec<Inline>,
}

impl Block {
    /// Creates a block without children, lines or inlines.
    pub fn new(kind: BlockKind, span: Span) -> Self {
        Self {
            kind,
            span,
            children: Vec::new(),
            lines: Vec::new(),
            inlines: Vec::new(),
        }
    }

    /// Shifts every range in this subtree by `delta` bytes.
    pub fn shift(&mut self, delta: isize) {
        let sh = |r: &mut Span| {
            r.start = (r.start as isize + delta) as usize;
            r.end = (r.end as isize + delta) as usize;
        };
        sh(&mut self.span);
        match &mut self.kind {
            BlockKind::Heading {
                marker, closing, ..
            } => {
                sh(marker);
                if let Some(c) = closing {
                    sh(c);
                }
            }
            BlockKind::CodeBlock { fence: Some(f) } => {
                sh(&mut f.open);
                if let Some(i) = &mut f.info {
                    sh(i);
                }
                if let Some(c) = &mut f.close {
                    sh(c);
                }
            }
            BlockKind::BlockQuote {
                alert: Some((_, s)),
            } => sh(s),
            BlockKind::ListItem { marker, task, .. } => {
                sh(marker);
                if let Some(t) = task {
                    sh(&mut t.span);
                }
            }
            BlockKind::Table { delimiter, .. } => sh(delimiter),
            BlockKind::FrontMatter { open, close } => {
                sh(open);
                sh(close);
            }
            BlockKind::MathBlock { open, close } => {
                sh(open);
                if let Some(c) = close {
                    sh(c);
                }
            }
            BlockKind::FootnoteDef { marker, .. } => sh(marker),
            _ => {}
        }
        for l in &mut self.lines {
            sh(&mut l.span);
        }
        for i in &mut self.inlines {
            i.shift(delta);
        }
        for c in &mut self.children {
            c.shift(delta);
        }
    }

    /// Visits this block and all descendants in document order.
    pub fn walk<'a>(&'a self, f: &mut dyn FnMut(&'a Block)) {
        f(self);
        for c in &self.children {
            c.walk(f);
        }
    }

    /// Visits this block and all descendants mutably, in document order.
    pub fn walk_mut(&mut self, f: &mut dyn FnMut(&mut Block)) {
        f(self);
        for c in &mut self.children {
            c.walk_mut(f);
        }
    }
}

/// How a link was written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkSyntax {
    /// `[text](dest "title")`; range of `(…)`.
    Inline {
        /// Range of the destination inside the parentheses (empty if none).
        dest: Span,
    },
    /// `[text][label]`.
    Reference {
        /// Normalized label.
        label: String,
    },
    /// `[label][]`.
    Collapsed,
    /// `[label]`.
    Shortcut,
    /// `<https://…>` / `<a@b.c>`.
    Autolink,
    /// GFM extended autolink (`www.…`, bare URL or e-mail).
    Extended,
    /// HTML `<a href>` (reserved for the HTML subset renderer).
    Html,
}

/// Kinds of inline nodes.
#[derive(Debug, Clone, PartialEq)]
pub enum InlineKind {
    /// Literal text: the display text equals the source in `span`.
    Text,
    /// Backslash escape: displays the char after `\`.
    Escape,
    /// Entity or numeric character reference; displays `text`.
    Entity {
        /// Decoded text.
        text: String,
    },
    /// Soft line break.
    SoftBreak,
    /// Hard line break (two spaces or `\` before the line end).
    HardBreak,
    /// Code span; `content` is the inner source range, `text` the normalized code.
    Code {
        /// Inner range (between the backtick runs).
        content: Span,
        /// Normalized code text.
        text: String,
    },
    /// `*x*` / `_x_`.
    Emph {
        /// Opening delimiter.
        open: Span,
        /// Closing delimiter.
        close: Span,
    },
    /// `**x**` / `__x__`.
    Strong {
        /// Opening delimiter.
        open: Span,
        /// Closing delimiter.
        close: Span,
    },
    /// `~~x~~`.
    Strike {
        /// Opening delimiter.
        open: Span,
        /// Closing delimiter.
        close: Span,
    },
    /// `==x==`.
    Highlight {
        /// Opening delimiter.
        open: Span,
        /// Closing delimiter.
        close: Span,
    },
    /// `^x^`.
    Superscript {
        /// Opening delimiter.
        open: Span,
        /// Closing delimiter.
        close: Span,
    },
    /// `~x~`.
    Subscript {
        /// Opening delimiter.
        open: Span,
        /// Closing delimiter.
        close: Span,
    },
    /// Link; children are the link text.
    Link {
        /// Destination (unescaped, not URL-encoded).
        dest: String,
        /// Title.
        title: Option<String>,
        /// Range of the link text between `[` and `]` (empty for autolinks).
        text: Span,
        /// Syntax used.
        syntax: LinkSyntax,
    },
    /// Image; children are the alt text.
    Image {
        /// Source (unescaped).
        dest: String,
        /// Title.
        title: Option<String>,
        /// Range of the alt text between `![` and `]`.
        text: Span,
        /// Syntax used.
        syntax: LinkSyntax,
    },
    /// Raw inline HTML.
    Html,
    /// `$x$` (or `\(x\)`) inline math; `content` is the TeX source range.
    Math {
        /// TeX source range.
        content: Span,
    },
    /// Footnote reference `[^label]`.
    FootnoteRef {
        /// Normalized label.
        label: String,
    },
    /// `:shortcode:` emoji.
    Emoji {
        /// The emoji.
        emoji: &'static str,
    },
}

/// An inline node.
#[derive(Debug, Clone, PartialEq)]
pub struct Inline {
    /// Kind and data.
    pub kind: InlineKind,
    /// Full source range including markers.
    pub span: Span,
    /// Children (emphasis content, link text, image alt).
    pub children: Vec<Inline>,
}

impl Inline {
    /// A leaf inline.
    pub fn leaf(kind: InlineKind, span: Span) -> Self {
        Self {
            kind,
            span,
            children: Vec::new(),
        }
    }

    /// Shifts every range in this subtree by `delta` bytes.
    pub fn shift(&mut self, delta: isize) {
        let sh = |r: &mut Span| {
            r.start = (r.start as isize + delta) as usize;
            r.end = (r.end as isize + delta) as usize;
        };
        sh(&mut self.span);
        match &mut self.kind {
            InlineKind::Code { content, .. } | InlineKind::Math { content } => sh(content),
            InlineKind::Emph { open, close }
            | InlineKind::Strong { open, close }
            | InlineKind::Strike { open, close }
            | InlineKind::Highlight { open, close }
            | InlineKind::Superscript { open, close }
            | InlineKind::Subscript { open, close } => {
                sh(open);
                sh(close);
            }
            InlineKind::Link { text, syntax, .. } | InlineKind::Image { text, syntax, .. } => {
                sh(text);
                if let LinkSyntax::Inline { dest } = syntax {
                    sh(dest);
                }
            }
            _ => {}
        }
        for c in &mut self.children {
            c.shift(delta);
        }
    }

    /// The `(open, close)` marker ranges of delimited inlines, if any.
    pub fn markers(&self) -> Option<(Span, Span)> {
        match &self.kind {
            InlineKind::Emph { open, close }
            | InlineKind::Strong { open, close }
            | InlineKind::Strike { open, close }
            | InlineKind::Highlight { open, close }
            | InlineKind::Superscript { open, close }
            | InlineKind::Subscript { open, close } => Some((open.clone(), close.clone())),
            _ => None,
        }
    }

    /// Visits this node and all descendants.
    pub fn walk<'a>(&'a self, f: &mut dyn FnMut(&'a Inline)) {
        f(self);
        for c in &self.children {
            c.walk(f);
        }
    }
}
