//! # inkra-markdown
//!
//! **Layer L1a** (see SPEC 2.2).
//!
//! Inkra's own incremental Markdown parser (AD-1): CommonMark 0.31 block and
//! inline structure, GFM tables, task lists, strikethrough and extended autolinks,
//! plus Inkra extensions (math, footnotes, front matter, `[TOC]`, alerts,
//! highlight, super/subscript, emoji). Every node carries exact byte ranges,
//! including those of its markers; [`Document::edit`] re-parses only the affected
//! blocks; the source is never rewritten wholesale, so untouched bytes round-trip
//! exactly (MD-091).

pub mod anchor;
pub mod ast;
mod block;
pub mod emoji;
mod entities;
pub mod html;
mod incremental;
pub mod inline;
pub mod options;
pub mod refs;
pub mod scan;
mod source;
pub mod tokens;
pub mod walk;

pub use ast::{Block, BlockKind, Inline, InlineKind, Span};
pub use incremental::{BlockRef, Document, EditStats};
pub use inline::{InlineContext, LinkTarget};
pub use options::ParseOptions;
pub use source::Source;

use block::BlockParser;

/// Line index (relative to the document start) of the closing front matter fence,
/// if the document starts with YAML front matter (MD-012).
pub(crate) fn front_matter_close<S: Source + ?Sized>(src: &S) -> Option<usize> {
    let mut idx = 0usize;
    let mut found = None;
    src.for_each_line_from(0, &mut |line| {
        let t = line
            .trim_end_matches(['\n', '\r'])
            .trim_end_matches([' ', '\t']);
        if idx == 0 {
            if t != "---" {
                return false;
            }
        } else if t == "---" || t == "..." {
            found = Some(idx);
            return false;
        }
        idx += 1;
        true
    });
    found
}

/// Parses top-level blocks from `start` (a line start where only the document is
/// open). If `stop` is given it is called after each line with the parser; when
/// it returns `true` the parse ends before the block that just started.
pub(crate) fn parse_blocks_from<S: Source + ?Sized>(
    src: &S,
    o: &ParseOptions,
    start: usize,
    stop: &mut dyn FnMut(&BlockParser<'_>) -> bool,
) -> (Vec<Block>, Vec<bool>, bool) {
    let fm = if start == 0 && o.front_matter {
        front_matter_close(src)
    } else {
        None
    };
    let mut p = BlockParser::new(o, start, fm);
    let mut stopped = false;
    src.for_each_line_from(start, &mut |line| {
        p.feed(line);
        if stop(&p) {
            stopped = true;
            return false;
        }
        true
    });
    let (blocks, fresh) = if stopped {
        p.finish_before_new_top_level()
    } else {
        p.finish()
    };
    (blocks, fresh, stopped)
}

/// Collects link reference definitions and footnote labels from `blocks`.
pub fn collect_context(blocks: &[Block]) -> InlineContext {
    let mut cx = InlineContext::default();
    for b in blocks {
        b.walk(&mut |b| match &b.kind {
            BlockKind::LinkDef { label, dest, title } => {
                cx.refs.entry(label.clone()).or_insert_with(|| LinkTarget {
                    dest: dest.clone(),
                    title: title.clone(),
                });
            }
            BlockKind::FootnoteDef { label, .. } => {
                cx.footnotes.insert(label.clone());
            }
            _ => {}
        });
    }
    cx
}

/// Builds the inline subject of a leaf block (table cells get `\|` unescaped).
pub fn subject_of<S: Source + ?Sized>(src: &S, b: &Block) -> inline::Subject {
    inline::Subject::new(src, &b.lines, true, matches!(b.kind, BlockKind::TableCell))
}

/// Parses the inline content of every leaf block in `blocks`.
pub fn parse_all_inlines<S: Source + ?Sized>(
    src: &S,
    blocks: &mut [Block],
    o: &ParseOptions,
    cx: &InlineContext,
) {
    for b in blocks {
        b.walk_mut(&mut |b| {
            if b.kind.has_inlines() {
                let subject = subject_of(src, b);
                b.inlines = inline::parse_inlines(&subject, o, cx);
            }
        });
    }
}

/// A parsed document: blocks plus the definitions used for inline parsing.
#[derive(Debug, Clone, PartialEq)]
pub struct Parsed {
    /// Top-level blocks in source order.
    pub blocks: Vec<Block>,
    /// Reference definitions and footnote labels.
    pub cx: InlineContext,
}

/// Parses a whole document.
pub fn parse<S: Source + ?Sized>(src: &S, o: &ParseOptions) -> Parsed {
    let (mut blocks, _, _) = parse_blocks_from(src, o, 0, &mut |_| false);
    let cx = collect_context(&blocks);
    parse_all_inlines(src, &mut blocks, o, &cx);
    Parsed { blocks, cx }
}
