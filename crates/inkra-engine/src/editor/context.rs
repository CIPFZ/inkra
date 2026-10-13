//! Syntactic context at a source position (from the AST), used by editing
//! operations.

use std::ops::Range;

use inkra_doc::Doc;
use inkra_markdown::ast::{Block, BlockKind, ListKind};
use inkra_markdown::walk::path_at;

/// A block on the path to a position, with absolute ranges.
#[derive(Debug, Clone)]
pub struct Node<'a> {
    /// The block (stored coordinates).
    pub block: &'a Block,
    /// Offset delta to absolute coordinates.
    pub delta: isize,
}

impl Node<'_> {
    /// Absolute position of a stored offset.
    pub fn abs(&self, p: usize) -> usize {
        (p as isize + self.delta) as usize
    }
    /// Absolute range of a stored range.
    pub fn abs_range(&self, r: &Range<usize>) -> Range<usize> {
        self.abs(r.start)..self.abs(r.end)
    }
    /// Absolute span of the block.
    pub fn span(&self) -> Range<usize> {
        self.abs_range(&self.block.span)
    }
}

/// Blocks containing `pos`, outermost first.
pub fn path(doc: &Doc, pos: usize) -> Vec<Node<'_>> {
    let ast = doc.ast();
    let Some(i) = ast.index_at(pos) else {
        return Vec::new();
    };
    let Some(r) = ast.get(i) else {
        return Vec::new();
    };
    let rel = r.rel(pos);
    path_at(std::slice::from_ref(r.block), rel)
        .into_iter()
        .map(|b| Node {
            block: b,
            delta: r.delta,
        })
        .collect()
}

/// Information about the list item containing a position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemInfo {
    /// Marker range (absolute).
    pub marker: Range<usize>,
    /// Marker kind.
    pub kind: ListKind,
    /// Ordered number.
    pub number: Option<u64>,
    /// Task checkbox `[ ]` range and state.
    pub task: Option<(Range<usize>, bool)>,
    /// Item span (absolute).
    pub span: Range<usize>,
    /// Whether the list is tight.
    pub tight: bool,
    /// Depth of nesting (0 = outermost list).
    pub depth: usize,
    /// Index of this item in its list.
    pub index: usize,
    /// Span of the list.
    pub list_span: Range<usize>,
    /// Absolute marker starts of all items of the list.
    pub siblings: Vec<Range<usize>>,
    /// Whether the item has no content at all.
    pub empty: bool,
}

/// The innermost list item containing `pos`.
pub fn list_item(doc: &Doc, pos: usize) -> Option<ItemInfo> {
    let p = path(doc, pos);
    let mut depth = 0;
    let mut found = None;
    for (k, n) in p.iter().enumerate() {
        if let BlockKind::ListItem {
            marker,
            number,
            task,
            ..
        } = &n.block.kind
        {
            let list = &p[k - 1];
            let BlockKind::List { kind, tight, .. } = list.block.kind else {
                continue;
            };
            let index = list
                .block
                .children
                .iter()
                .position(|c| std::ptr::eq(c, n.block))
                .unwrap_or(0);
            let siblings = list
                .block
                .children
                .iter()
                .filter_map(|c| match &c.kind {
                    BlockKind::ListItem { marker, .. } => Some(list.abs_range(marker)),
                    _ => None,
                })
                .collect();
            found = Some(ItemInfo {
                marker: n.abs_range(marker),
                kind,
                number: *number,
                task: task.as_ref().map(|t| (n.abs_range(&t.span), t.checked)),
                span: n.span(),
                tight,
                depth,
                index,
                list_span: list.span(),
                siblings,
                empty: n.block.children.is_empty(),
            });
            depth += 1;
        }
    }
    found
}

/// Whether `pos` is inside a code block (fenced or indented), and the block span.
pub fn code_block(doc: &Doc, pos: usize) -> Option<Range<usize>> {
    if unclosed_block_before(doc, pos) {
        let ast = doc.ast();
        return ast.get(ast.len() - 1).map(|r| r.span());
    }
    path(doc, pos)
        .iter()
        .rev()
        .find_map(|n| matches!(n.block.kind, BlockKind::CodeBlock { .. }).then(|| n.span()))
}

/// The list item containing `pos`, or the one whose (empty) content `pos` sits
/// in: a caret after `- ` lies past the item's span, so the last non-blank
/// character of the line before the caret is probed too.
pub fn list_item_near(doc: &Doc, pos: usize) -> Option<ItemInfo> {
    if let Some(i) = list_item(doc, pos) {
        return Some(i);
    }
    let t = doc.text();
    let ls = t.line_to_byte(t.byte_to_line(pos));
    let before = t.slice(ls..pos);
    let probe = ls + before.trim_end().len();
    (probe > ls).then(|| list_item(doc, probe)).flatten()
}

/// Whether the list item whose marker starts at `marker_start` ends with a
/// nested list (so an indented next sibling joins it).
pub fn item_ends_with_list(doc: &Doc, marker_start: usize) -> bool {
    path(doc, marker_start)
        .iter()
        .rev()
        .find(|n| matches!(n.block.kind, BlockKind::ListItem { .. }))
        .and_then(|n| n.block.children.last())
        .is_some_and(|c| matches!(c.kind, BlockKind::List { .. }))
}

/// An unclosed fenced code block (or math block) that the document ends in and
/// that `pos` follows: such a block extends to the end of the document.
fn unclosed_block_before(doc: &Doc, pos: usize) -> bool {
    let ast = doc.ast();
    if pos < doc.text().len() {
        return false;
    }
    let Some(last) = ast.get(ast.len().wrapping_sub(1)) else {
        return false;
    };
    let b = last.block;
    let open_end = match &b.kind {
        BlockKind::CodeBlock { fence: Some(f) } if f.close.is_none() => last.abs(f.open.end),
        BlockKind::MathBlock { open, close: None } => last.abs(open.end),
        _ => return false,
    };
    pos > open_end
}

/// Whether `pos` is inside a code block's content lines (not on the fences).
pub fn in_code_content(doc: &Doc, pos: usize) -> bool {
    if unclosed_block_before(doc, pos) {
        return true;
    }
    path(doc, pos).iter().rev().any(|n| match &n.block.kind {
        BlockKind::CodeBlock { fence } => match fence {
            Some(f) => {
                let open = n.abs_range(&f.open);
                let after_open = pos > open.end;
                // An empty block's only caret position is the start of its closing fence.
                let empty = n.block.lines.is_empty();
                let before_close = f
                    .close
                    .as_ref()
                    .is_none_or(|c| pos < n.abs(c.start) || empty && pos == n.abs(c.start));
                after_open && before_close
            }
            None => true,
        },
        BlockKind::MathBlock { .. }
        | BlockKind::FrontMatter { .. }
        | BlockKind::HtmlBlock { .. } => true,
        _ => false,
    })
}

/// Number of enclosing block quotes.
pub fn quote_depth(doc: &Doc, pos: usize) -> usize {
    path(doc, pos)
        .iter()
        .filter(|n| matches!(n.block.kind, BlockKind::BlockQuote { .. }))
        .count()
}

/// Whether `pos` is inside inline code or inline math.
pub fn in_inline_code(doc: &Doc, pos: usize) -> bool {
    let p = path(doc, pos);
    let Some(leaf) = p.last() else { return false };
    let rel = (pos as isize - leaf.delta) as usize;
    let mut hit = false;
    for i in &leaf.block.inlines {
        i.walk(&mut |i| {
            if matches!(
                i.kind,
                inkra_markdown::InlineKind::Code { .. } | inkra_markdown::InlineKind::Math { .. }
            ) && i.span.start < rel
                && rel < i.span.end
            {
                hit = true;
            }
        });
    }
    hit
}

/// Whether `pos` is at the end of the opening fence line of a code block.
pub fn at_fence_open_end(doc: &Doc, pos: usize) -> bool {
    path(doc, pos).iter().rev().any(|n| match &n.block.kind {
        BlockKind::CodeBlock { fence: Some(f) } => n.abs(f.open.end) == pos,
        _ => false,
    })
}

/// The definition of footnote `label`: start offset and plain text (MD-010).
pub fn footnote_def(doc: &Doc, label: &str) -> Option<(usize, String)> {
    let ast = doc.ast();
    for i in 0..ast.len() {
        let r = ast.get(i)?;
        let mut found = None;
        r.block.walk(&mut |b| {
            if found.is_some() {
                return;
            }
            if let BlockKind::FootnoteDef { label: l, .. } = &b.kind
                && l == label
            {
                let mut text = String::new();
                b.walk(&mut |c| {
                    if c.kind.has_inlines() {
                        if !text.is_empty() {
                            text.push('\n');
                        }
                        inkra_markdown::walk::plain_text(
                            doc.text(),
                            &shifted(&c.inlines, r.delta),
                            &mut text,
                        );
                    }
                });
                found = Some((r.abs(b.span.start), text));
            }
        });
        if found.is_some() {
            return found;
        }
    }
    None
}

fn shifted(list: &[inkra_markdown::Inline], delta: isize) -> Vec<inkra_markdown::Inline> {
    list.iter()
        .map(|i| {
            let mut i = i.clone();
            i.shift(delta);
            i
        })
        .collect()
}

/// A `key: value` of the document's YAML front matter (MD-012), quotes removed.
pub fn front_matter_value(doc: &Doc, key: &str) -> Option<String> {
    let ast = doc.ast();
    let r = ast.get(0)?;
    if !matches!(r.block.kind, BlockKind::FrontMatter { .. }) {
        return None;
    }
    let text = doc.text().slice(r.span()).into_owned();
    text.lines().find_map(|l| {
        let (k, v) = l.split_once(':')?;
        (k.trim() == key)
            .then(|| v.trim().trim_matches(['"', '\'']).to_owned())
            .filter(|v| !v.is_empty())
    })
}
