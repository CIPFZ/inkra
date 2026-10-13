//! Live rendering layout (ED-001, ED-002): the document is flattened into leaf
//! blocks (paragraphs, headings, code blocks, …) that carry their container
//! context (block quotes, list items, footnotes). Each leaf is one view item, so
//! long lists and quotes are virtualized and cached per leaf.

pub mod inline;
pub mod leaf;

use std::ops::Range;

use inkra_doc::Doc;
use inkra_markdown::ast::{AlertKind, Block, BlockKind, ListKind};

/// A container around a leaf.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Frame {
    /// Block quote (or GitHub alert).
    Quote {
        /// Alert type and absolute marker range.
        alert: Option<(AlertKind, Range<usize>)>,
        /// First leaf of the quote.
        first: bool,
    },
    /// List item.
    Item {
        /// Marker as displayed: bullet depth or ordered number.
        marker: Marker,
        /// Task checkbox: absolute `[ ]` range and state.
        task: Option<(Range<usize>, bool)>,
        /// First leaf of the item (draws the marker).
        first: bool,
        /// The list is tight.
        tight: bool,
        /// Absolute start of the list (identity).
        list: usize,
    },
    /// Footnote definition.
    Footnote {
        /// Label.
        label: String,
        /// Absolute `[^x]:` range.
        marker: Range<usize>,
        /// First leaf.
        first: bool,
    },
}

/// List marker display.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Marker {
    /// Bullet at nesting depth (• ◦ ▪ cycle).
    Bullet(usize),
    /// Ordered number and delimiter.
    Number(u64, char),
}

/// What kind of leaf an item is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LeafKind {
    /// A real block.
    Block,
    /// A virtual empty paragraph where the caret sits on a blank line.
    EmptyLine,
}

/// One live item: a leaf block plus its container frames (outermost first).
#[derive(Debug, Clone, PartialEq)]
pub struct LiveItem {
    /// Absolute source span of the leaf.
    pub src: Range<usize>,
    /// Index of the top-level block.
    pub top: usize,
    /// Child index path from the top-level block to the leaf.
    pub path: Vec<usize>,
    /// Containers.
    pub frames: Vec<Frame>,
    /// Kind.
    pub kind: LeafKind,
}

impl LiveItem {
    /// The leaf block (stored coordinates) and the delta to absolute ones.
    pub fn block<'a>(&self, doc: &'a Doc) -> Option<(&'a Block, isize)> {
        let r = doc.ast().get(self.top)?;
        let mut b = r.block;
        for &i in &self.path {
            b = b.children.get(i)?;
        }
        Some((b, r.delta))
    }
}

struct Flatten<'a> {
    out: &'a mut Vec<LiveItem>,
    top: usize,
    delta: isize,
    text: &'a inkra_text::Text,
}

impl Flatten<'_> {
    fn abs(&self, r: &Range<usize>) -> Range<usize> {
        (r.start as isize + self.delta) as usize..(r.end as isize + self.delta) as usize
    }

    fn walk(&mut self, b: &Block, path: &mut Vec<usize>, frames: &mut Vec<Frame>, depth: usize) {
        match &b.kind {
            BlockKind::BlockQuote { alert } => {
                frames.push(Frame::Quote {
                    alert: alert.as_ref().map(|(k, r)| (*k, self.abs(r))),
                    first: true,
                });
                self.children(b, path, frames, depth);
                frames.pop();
            }
            BlockKind::List { kind, start, tight } => {
                for (i, item) in b.children.iter().enumerate() {
                    let BlockKind::ListItem { task, .. } = &item.kind else {
                        continue;
                    };
                    let marker = match kind {
                        ListKind::Bullet(_) => Marker::Bullet(depth),
                        ListKind::Ordered(d) => Marker::Number(start + i as u64, *d),
                    };
                    frames.push(Frame::Item {
                        marker,
                        task: task.as_ref().map(|t| (self.abs(&t.span), t.checked)),
                        first: true,
                        tight: *tight,
                        list: self.abs(&b.span).start,
                    });
                    path.push(i);
                    if item.children.is_empty() {
                        // An empty item still gets a line (so the caret can sit there),
                        // after the space following the marker or checkbox.
                        let mut at = self.abs(&item.span).end;
                        if self.text.char_at(at) == Some(' ') {
                            at += 1;
                        }
                        self.out.push(LiveItem {
                            src: at..at,
                            top: self.top,
                            path: path.clone(),
                            frames: frames.clone(),
                            kind: LeafKind::EmptyLine,
                        });
                        mark_not_first(frames);
                    } else {
                        self.children(item, path, frames, depth + 1);
                    }
                    path.pop();
                    frames.pop();
                }
            }
            BlockKind::FootnoteDef { label, marker } => {
                frames.push(Frame::Footnote {
                    label: label.clone(),
                    marker: self.abs(marker),
                    first: true,
                });
                self.children(b, path, frames, depth);
                frames.pop();
            }
            _ => {
                self.out.push(LiveItem {
                    src: self.abs(&b.span),
                    top: self.top,
                    path: path.clone(),
                    frames: frames.clone(),
                    kind: LeafKind::Block,
                });
                mark_not_first(frames);
            }
        }
    }

    fn children(
        &mut self,
        b: &Block,
        path: &mut Vec<usize>,
        frames: &mut Vec<Frame>,
        depth: usize,
    ) {
        if b.children.is_empty()
            && matches!(
                b.kind,
                BlockKind::BlockQuote { .. } | BlockKind::FootnoteDef { .. }
            )
        {
            let at = self.abs(&b.span).end;
            self.out.push(LiveItem {
                src: at..at,
                top: self.top,
                path: path.clone(),
                frames: frames.clone(),
                kind: LeafKind::EmptyLine,
            });
            mark_not_first(frames);
            return;
        }
        for (i, c) in b.children.iter().enumerate() {
            path.push(i);
            self.walk(c, path, frames, depth);
            path.pop();
        }
    }
}

/// After a leaf has been emitted, no enclosing frame is at its first leaf.
fn mark_not_first(frames: &mut [Frame]) {
    for f in frames {
        match f {
            Frame::Quote { first, .. }
            | Frame::Item { first, .. }
            | Frame::Footnote { first, .. } => *first = false,
        }
    }
}

/// Flattens the document into live items, in source order.
pub fn flatten(doc: &Doc) -> Vec<LiveItem> {
    let mut out = Vec::new();
    for (top, r) in doc.ast().iter().enumerate() {
        let mut f = Flatten {
            out: &mut out,
            top,
            delta: r.delta,
            text: doc.text(),
        };
        f.walk(r.block, &mut Vec::new(), &mut Vec::new(), 0);
    }
    out
}

/// Container frames enclosing `pos` (for a virtual empty line at the caret).
pub fn frames_at(doc: &Doc, pos: usize) -> Vec<Frame> {
    let ast = doc.ast();
    let Some(i) = ast.index_at(pos) else {
        return Vec::new();
    };
    let Some(r) = ast.get(i) else {
        return Vec::new();
    };
    let rel = r.rel(pos);
    let mut frames = Vec::new();
    let mut level = std::slice::from_ref(r.block);
    let mut depth = 0;
    while let Some(b) = level
        .iter()
        .find(|b| b.span.start <= rel && rel <= b.span.end)
    {
        match &b.kind {
            BlockKind::BlockQuote { .. } => frames.push(Frame::Quote {
                alert: None,
                first: false,
            }),
            BlockKind::ListItem { .. } => {
                frames.push(Frame::Item {
                    marker: Marker::Bullet(depth),
                    task: None,
                    first: false,
                    tight: true,
                    list: 0,
                });
                depth += 1;
            }
            BlockKind::List { .. } => {}
            BlockKind::FootnoteDef { label, marker } => frames.push(Frame::Footnote {
                label: label.clone(),
                marker: r.abs_span(marker),
                first: false,
            }),
            _ => break,
        }
        level = &b.children;
    }
    frames
}

/// Vertical gap above `cur` given the previous item (SPEC 7.1 spacing).
pub fn gap(doc: &Doc, prev: Option<&LiveItem>, cur: &LiveItem, style: &crate::Style) -> f32 {
    let Some(prev) = prev else { return 0.0 };
    let base = gap_base(doc, prev, cur, style);
    if !style.preserve_blank_lines || prev.src.end > cur.src.start {
        return base;
    }
    // MD-074: every blank line beyond the first adds a line of space.
    let t = doc.text();
    let between = t.slice(prev.src.end..cur.src.start);
    let blank = between
        .split('\n')
        .skip(1)
        .filter(|l| l.trim().is_empty())
        .count()
        .saturating_sub(2);
    base + blank as f32 * style.size * style.line_height
}

fn gap_base(doc: &Doc, prev: &LiveItem, cur: &LiveItem, style: &crate::Style) -> f32 {
    let em = style.size;
    let heading_size = |it: &LiveItem| -> Option<(f32, crate::style::HeadingStyle)> {
        let (b, _) = it.block(doc)?;
        match b.kind {
            BlockKind::Heading { level, .. } => {
                let hs = style.headings[(level as usize).clamp(1, 6) - 1];
                Some((hs.size * em, hs))
            }
            _ => None,
        }
    };
    if let Some((size, hs)) = heading_size(cur) {
        return hs.margin_top * size;
    }
    if let Some((size, hs)) = heading_size(prev) {
        return (hs.margin_bottom * size).max(style.paragraph_spacing * em * 0.5);
    }
    // Items of the same tight list sit close together.
    let tight_shared = cur.frames.iter().any(|f| match f {
        Frame::Item {
            tight: true, list, ..
        } => prev
            .frames
            .iter()
            .any(|g| matches!(g, Frame::Item { list: l, .. } if l == list)),
        _ => false,
    });
    if tight_shared {
        return 0.25 * em;
    }
    style.paragraph_spacing * em
}

/// Number of outer quote frames `cur` shares with `prev` (quote bars continue).
pub fn shared_quotes(prev: Option<&LiveItem>, cur: &LiveItem) -> usize {
    let Some(prev) = prev else { return 0 };
    let mut n = 0;
    let quotes = |it: &LiveItem| {
        it.frames
            .iter()
            .filter(|f| matches!(f, Frame::Quote { .. }))
            .count()
    };
    let (a, b) = (quotes(prev), quotes(cur));
    // Same top-level block and both inside quotes: assume continuity of the common depth.
    if prev.top == cur.top {
        n = a.min(b);
        if cur
            .frames
            .iter()
            .any(|f| matches!(f, Frame::Quote { first: true, .. }))
        {
            n = n.saturating_sub(1);
        }
    }
    n
}

thread_local! {
    static DETAILS: std::cell::RefCell<(std::collections::HashSet<u64>, u64)> = Default::default();
}

/// Key of a `<details>` element: its summary and position (MD-014).
pub fn details_key<'a>(summary: impl Iterator<Item = &'a str>, n: usize) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for s in summary {
        s.hash(&mut h);
    }
    n.hash(&mut h);
    h.finish()
}

/// Whether the reader flipped `<details>` `key` from its `open` attribute.
pub fn details_toggled(key: u64) -> bool {
    DETAILS.with(|d| d.borrow().0.contains(&key))
}

/// Folds or unfolds `<details>` `key` in the view (not in the file).
pub fn toggle_details(key: u64) {
    DETAILS.with(|d| {
        let mut d = d.borrow_mut();
        if !d.0.remove(&key) {
            d.0.insert(key);
        }
        d.1 += 1;
    });
}

/// Changes whenever a `<details>` is toggled (cache keys).
pub fn details_generation() -> u64 {
    DETAILS.with(|d| d.borrow().1)
}

/// Document-wide facts some items need: footnote numbers by order of first
/// reference (MD-010) and the headings shown by `[TOC]` (MD-013).
#[derive(Debug, Clone, Default, PartialEq, Hash)]
pub struct DocInfo {
    /// Footnote label → number.
    pub footnotes: Vec<(String, u32)>,
    /// `(level, text, anchor)` of every heading, when the document has a `[TOC]`.
    pub headings: Vec<(u8, String, String)>,
    /// Heading numbers (`1.2.`) by heading start, when the theme numbers them
    /// (MD-002, render only).
    pub heading_numbers: Vec<(usize, String)>,
}

impl DocInfo {
    /// The number of footnote `label`.
    pub fn footnote(&self, label: &str) -> Option<u32> {
        self.footnotes
            .iter()
            .find(|(l, _)| l == label)
            .map(|(_, n)| *n)
    }

    /// The number shown before the heading starting at `start`.
    pub fn heading_number(&self, start: usize) -> Option<&str> {
        self.heading_numbers
            .iter()
            .find(|(s, _)| *s == start)
            .map(|(_, n)| n.as_str())
    }

    /// Like [`Self::compute`], numbering headings when `numbers` is set.
    pub fn compute_with(doc: &Doc, items: &[LiveItem], numbers: bool) -> Self {
        let mut info = Self::compute(doc, items);
        if numbers {
            let blocks = doc.ast().to_blocks();
            let heads = inkra_markdown::walk::headings(doc.text(), &blocks);
            let top = heads.iter().map(|h| h.level).min().unwrap_or(1);
            let mut counters = [0u32; 6];
            for h in heads {
                let k = usize::from(h.level.saturating_sub(top)).min(5);
                counters[k] += 1;
                for c in counters.iter_mut().skip(k + 1) {
                    *c = 0;
                }
                let n: Vec<String> = counters[..=k].iter().map(|c| c.to_string()).collect();
                info.heading_numbers
                    .push((h.span.start, format!("{}.", n.join("."))));
            }
        }
        info
    }

    /// Computes the info for `doc` given its flattened items.
    pub fn compute(doc: &Doc, items: &[LiveItem]) -> Self {
        let mut info = Self::default();
        let has_toc = items.iter().any(|it| {
            it.block(doc)
                .is_some_and(|(b, _)| matches!(b.kind, BlockKind::Toc))
        });
        let has_notes = items.iter().any(|it| {
            it.frames
                .iter()
                .any(|f| matches!(f, Frame::Footnote { .. }))
        });
        if !has_toc && !has_notes {
            return info;
        }
        let ast = doc.ast();
        if has_notes {
            let mut next = 1;
            for i in 0..ast.len() {
                let Some(r) = ast.get(i) else { continue };
                r.block.walk(&mut |b| {
                    for inl in &b.inlines {
                        inl.walk(&mut |n| {
                            if let inkra_markdown::InlineKind::FootnoteRef { label } = &n.kind
                                && !info.footnotes.iter().any(|(l, _)| l == label)
                            {
                                info.footnotes.push((label.clone(), next));
                                next += 1;
                            }
                        });
                    }
                });
            }
        }
        if has_toc {
            let blocks = ast.to_blocks();
            info.headings = inkra_markdown::walk::headings(doc.text(), &blocks)
                .into_iter()
                .map(|h| (h.level, h.text, h.anchor))
                .collect();
        }
        info
    }
}

#[cfg(test)]
mod doc_info_tests {
    use super::*;

    #[test]
    fn toc_and_footnotes() {
        let doc = Doc::from_str(
            "# A\n\n[TOC]\n\n## B\n\nx[^n] y[^m]\n\n[^m]: m\n[^n]: n\n",
            Default::default(),
        );
        let items = flatten(&doc);
        let info = DocInfo::compute(&doc, &items);
        assert_eq!(info.footnote("n"), Some(1));
        assert_eq!(info.footnote("m"), Some(2));
        assert_eq!(info.headings.len(), 2, "{info:?}");
    }
}
