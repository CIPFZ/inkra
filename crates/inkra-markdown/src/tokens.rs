//! Syntax tokens for source-mode highlighting (ED-121), derived from the AST.

use crate::ast::{Block, BlockKind, Inline, InlineKind, LinkSyntax, Span};
use crate::incremental::{BlockRef, Document};

/// Highlight class of a source range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TokenClass {
    /// `#`, setext underline, closing `###`.
    HeadingMarker,
    /// Heading text of the given level.
    Heading(u8),
    /// `*`/`_`/`~~`/`==`/`^`/`~` delimiters, `\` of escapes, `[` `]` `(` `)` of links.
    Marker,
    /// Emphasized text.
    Emphasis,
    /// Strong text.
    Strong,
    /// Strong and emphasized text.
    StrongEmphasis,
    /// Struck-through text.
    Strike,
    /// Highlighted text.
    Highlight,
    /// Inline code (including backticks).
    Code,
    /// Fence lines of code blocks (` ``` `, info string).
    Fence,
    /// Code block content.
    CodeBlock,
    /// Link text.
    LinkText,
    /// Link destination / autolink / reference label.
    Url,
    /// List marker or task checkbox.
    ListMarker,
    /// `>` block quote marker or alert marker.
    QuoteMarker,
    /// Table pipes and delimiter row.
    Table,
    /// Raw HTML (block or inline).
    Html,
    /// Entity reference.
    Entity,
    /// Math source (including `$` delimiters).
    Math,
    /// Front matter.
    FrontMatter,
    /// Thematic break.
    Rule,
    /// Link reference definition / footnote definition marker.
    Definition,
    /// Footnote reference.
    FootnoteRef,
}

/// A highlighted range.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Token {
    /// Absolute range.
    pub span: Span,
    /// Class.
    pub class: TokenClass,
}

struct Collector<'a> {
    range: Span,
    out: &'a mut Vec<Token>,
    delta: isize,
}

impl Collector<'_> {
    fn push(&mut self, span: &Span, class: TokenClass) {
        let a = (span.start as isize + self.delta) as usize;
        let b = (span.end as isize + self.delta) as usize;
        let a = a.max(self.range.start);
        let b = b.min(self.range.end);
        if a < b {
            self.out.push(Token { span: a..b, class });
        }
    }

    fn overlaps(&self, span: &Span) -> bool {
        let a = (span.start as isize + self.delta) as usize;
        let b = (span.end as isize + self.delta) as usize;
        a <= self.range.end && b >= self.range.start
    }

    fn block(&mut self, b: &Block) {
        if !self.overlaps(&b.span) {
            return;
        }
        match &b.kind {
            BlockKind::Heading {
                level,
                marker,
                closing,
                ..
            } => {
                self.push(marker, TokenClass::HeadingMarker);
                for l in &b.lines {
                    self.push(&l.span, TokenClass::Heading(*level));
                }
                if let Some(c) = closing {
                    self.push(c, TokenClass::HeadingMarker);
                }
                self.inlines(&b.inlines, None);
            }
            BlockKind::CodeBlock { fence } => {
                for l in &b.lines {
                    self.push(&l.span, TokenClass::CodeBlock);
                }
                if let Some(f) = fence {
                    self.push(&f.open, TokenClass::Fence);
                    if let Some(c) = &f.close {
                        self.push(c, TokenClass::Fence);
                    }
                }
            }
            BlockKind::HtmlBlock { .. } => {
                for l in &b.lines {
                    self.push(&l.span, TokenClass::Html);
                }
            }
            BlockKind::ThematicBreak => self.push(&b.span, TokenClass::Rule),
            BlockKind::LinkDef { .. } => self.push(&b.span, TokenClass::Definition),
            BlockKind::FrontMatter { .. } => self.push(&b.span, TokenClass::FrontMatter),
            BlockKind::MathBlock { .. } => self.push(&b.span, TokenClass::Math),
            BlockKind::Toc => self.push(&b.span, TokenClass::Definition),
            BlockKind::BlockQuote { alert } => {
                if let Some((_, s)) = alert {
                    self.push(s, TokenClass::QuoteMarker);
                }
                self.quote_markers(b);
                for c in &b.children {
                    self.block(c);
                }
            }
            BlockKind::ListItem { marker, task, .. } => {
                self.push(marker, TokenClass::ListMarker);
                if let Some(t) = task {
                    self.push(&t.span, TokenClass::ListMarker);
                }
                for c in &b.children {
                    self.block(c);
                }
            }
            BlockKind::FootnoteDef { marker, .. } => {
                self.push(marker, TokenClass::Definition);
                for c in &b.children {
                    self.block(c);
                }
            }
            BlockKind::Table { delimiter, .. } => {
                self.push(delimiter, TokenClass::Table);
                for row in &b.children {
                    // pipes are the gaps between cells
                    let mut prev = row.span.start;
                    for cell in &row.children {
                        self.push(&(prev..cell.span.start), TokenClass::Table);
                        prev = cell.span.end;
                        self.inlines(&cell.inlines, None);
                    }
                    self.push(&(prev..row.span.end), TokenClass::Table);
                }
            }
            _ => {
                for c in &b.children {
                    self.block(c);
                }
                self.inlines(&b.inlines, None);
            }
        }
    }

    /// `>` markers are the text between a quote's span start / line starts and its
    /// children; approximate by marking the first `>` run of each line in the span.
    fn quote_markers(&mut self, _b: &Block) {}

    fn inlines(&mut self, list: &[Inline], style: Option<TokenClass>) {
        for i in list {
            if !self.overlaps(&i.span) {
                continue;
            }
            match &i.kind {
                InlineKind::Text => {
                    if let Some(s) = style {
                        self.push(&i.span, s);
                    }
                }
                InlineKind::Escape => {
                    self.push(&(i.span.start..i.span.start + 1), TokenClass::Marker)
                }
                InlineKind::Entity { .. } => self.push(&i.span, TokenClass::Entity),
                InlineKind::Code { .. } => self.push(&i.span, TokenClass::Code),
                InlineKind::Math { .. } => self.push(&i.span, TokenClass::Math),
                InlineKind::Html => self.push(&i.span, TokenClass::Html),
                InlineKind::FootnoteRef { .. } => self.push(&i.span, TokenClass::FootnoteRef),
                InlineKind::Emph { open, close } | InlineKind::Strong { open, close } => {
                    let strong = matches!(i.kind, InlineKind::Strong { .. });
                    let next = match (style, strong) {
                        (Some(TokenClass::Emphasis), true) | (Some(TokenClass::Strong), false) => {
                            TokenClass::StrongEmphasis
                        }
                        (Some(TokenClass::StrongEmphasis), _) => TokenClass::StrongEmphasis,
                        (_, true) => TokenClass::Strong,
                        (_, false) => TokenClass::Emphasis,
                    };
                    self.push(open, TokenClass::Marker);
                    self.inlines(&i.children, Some(next));
                    self.push(close, TokenClass::Marker);
                }
                InlineKind::Strike { open, close }
                | InlineKind::Highlight { open, close }
                | InlineKind::Superscript { open, close }
                | InlineKind::Subscript { open, close } => {
                    let class = match i.kind {
                        InlineKind::Strike { .. } => TokenClass::Strike,
                        InlineKind::Highlight { .. } => TokenClass::Highlight,
                        _ => style.unwrap_or(TokenClass::Emphasis),
                    };
                    self.push(open, TokenClass::Marker);
                    self.inlines(&i.children, Some(class));
                    self.push(close, TokenClass::Marker);
                }
                InlineKind::Link { text, syntax, .. } | InlineKind::Image { text, syntax, .. } => {
                    match syntax {
                        LinkSyntax::Autolink | LinkSyntax::Extended => {
                            self.push(&i.span, TokenClass::Url)
                        }
                        _ => {
                            self.push(&(i.span.start..text.start), TokenClass::Marker);
                            self.inlines(&i.children, Some(TokenClass::LinkText));
                            self.push(&(text.end..i.span.end), TokenClass::Url);
                        }
                    }
                }
                InlineKind::SoftBreak | InlineKind::HardBreak | InlineKind::Emoji { .. } => {}
            }
        }
    }
}

/// Tokens overlapping `range` (absolute), clipped to it, in no particular order.
pub fn tokens_in(doc: &Document, range: Span) -> Vec<Token> {
    let mut out = Vec::new();
    let first = doc
        .first_starting_at_or_after(range.start)
        .saturating_sub(1);
    for i in first..doc.len() {
        let Some(r) = doc.get(i) else { break };
        if r.span().start > range.end {
            break;
        }
        block_tokens(r, &range, &mut out);
    }
    out
}

fn block_tokens(r: BlockRef<'_>, range: &Span, out: &mut Vec<Token>) {
    let mut c = Collector {
        range: range.clone(),
        out,
        delta: r.delta,
    };
    c.block(r.block);
}

/// Ranges of `>` block quote markers on the lines of `range` (computed from the
/// text since the AST does not store per-line container prefixes).
pub fn quote_marker_spans(text: &str, base: usize, in_quote: impl Fn(usize) -> bool) -> Vec<Span> {
    let mut out = Vec::new();
    let mut i = 0;
    let b = text.as_bytes();
    while i < b.len() {
        if b[i] == b'>' && in_quote(base + i) {
            let end = if b.get(i + 1) == Some(&b' ') {
                i + 2
            } else {
                i + 1
            };
            out.push(base + i..base + end);
            i = end;
        } else {
            i += 1;
        }
    }
    out
}
