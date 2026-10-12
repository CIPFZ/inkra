//! Traversal helpers over the AST.

use crate::Source;
use crate::ast::{Block, BlockKind, Inline, InlineKind};

/// Path of blocks (outermost first) containing `offset`. Uses inclusive ends so
/// a caret at the end of a block belongs to it.
pub fn path_at(blocks: &[Block], offset: usize) -> Vec<&Block> {
    let mut path = Vec::new();
    let mut level = blocks;
    loop {
        let idx = level.partition_point(|b| b.span.start <= offset);
        if idx == 0 {
            break;
        }
        let b = &level[idx - 1];
        if offset > b.span.end {
            break;
        }
        path.push(b);
        level = &b.children;
        if level.is_empty() {
            break;
        }
    }
    path
}

/// Index of the top-level block containing `offset` (inclusive end), if any.
pub fn top_level_at(blocks: &[Block], offset: usize) -> Option<usize> {
    let idx = blocks.partition_point(|b| b.span.start <= offset);
    (idx > 0 && offset <= blocks[idx - 1].span.end).then(|| idx - 1)
}

/// Appends the plain text of `inlines` (markers removed, entities decoded,
/// breaks as spaces) to `out`.
pub fn plain_text<S: Source + ?Sized>(src: &S, inlines: &[Inline], out: &mut String) {
    for i in inlines {
        match &i.kind {
            InlineKind::Text => out.push_str(&src.slice(i.span.clone())),
            InlineKind::Escape => out.push_str(&src.slice(i.span.start + 1..i.span.end)),
            InlineKind::Entity { text } => out.push_str(text),
            InlineKind::SoftBreak | InlineKind::HardBreak => out.push(' '),
            InlineKind::Code { text, .. } => out.push_str(text),
            InlineKind::Math { content } => out.push_str(&src.slice(content.clone())),
            InlineKind::Html => {}
            InlineKind::FootnoteRef { .. } => {}
            InlineKind::Emoji { emoji } => out.push_str(emoji),
            _ => plain_text(src, &i.children, out),
        }
    }
}

/// Plain text of a leaf block's inline content.
pub fn block_text<S: Source + ?Sized>(src: &S, b: &Block) -> String {
    let mut s = String::new();
    plain_text(src, &b.inlines, &mut s);
    s
}

/// A heading found in the document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadingInfo {
    /// Level 1..=6.
    pub level: u8,
    /// Plain text.
    pub text: String,
    /// GitHub-style anchor id (unique within the document).
    pub anchor: String,
    /// Source range of the heading block.
    pub span: crate::Span,
}

/// All headings (including those nested in containers), with unique anchors.
pub fn headings<S: Source + ?Sized>(src: &S, blocks: &[Block]) -> Vec<HeadingInfo> {
    let mut out = Vec::new();
    let mut slugs = crate::anchor::Slugger::default();
    for b in blocks {
        b.walk(&mut |b| {
            if let BlockKind::Heading { level, .. } = b.kind {
                let text = block_text(src, b);
                let anchor = slugs.slug(&text);
                out.push(HeadingInfo {
                    level,
                    text,
                    anchor,
                    span: b.span.clone(),
                });
            }
        });
    }
    out
}

/// Deepest inline nodes containing `offset` (outermost first, inclusive ends).
pub fn inline_path_at(inlines: &[Inline], offset: usize) -> Vec<&Inline> {
    let mut path = Vec::new();
    let mut level = inlines;
    'outer: loop {
        for i in level {
            if i.span.start <= offset && offset <= i.span.end {
                path.push(i);
                level = &i.children;
                continue 'outer;
            }
        }
        break;
    }
    path
}
