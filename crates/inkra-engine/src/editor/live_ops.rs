//! Live-mode editing semantics (ED-012, ED-013): block-aware Enter, Backspace and
//! Delete. Everything still produces plain source transactions (AD-2).

use inkra_doc::{Doc, Edit, Selection, Transaction};
use inkra_markdown::ast::BlockKind;

use super::context::{self, Node};
use super::ops::{self, Break, EditPrefs, Unit};

fn text(doc: &Doc) -> String {
    doc.text().to_string()
}

fn line_start(s: &str, pos: usize) -> usize {
    s[..pos].rfind(['\n', '\r']).map_or(0, |i| i + 1)
}

fn line_end(s: &str, pos: usize) -> usize {
    s[pos..].find(['\n', '\r']).map_or(s.len(), |i| pos + i)
}

fn replace(
    range: std::ops::Range<usize>,
    s: &str,
    before: Selection,
    after: Selection,
) -> Transaction {
    Transaction::single(range, s)
        .with_selection_before(before)
        .with_selection_after(after)
}

/// The innermost leaf block containing `pos` and its content start/end.
struct Leaf<'a> {
    node: Node<'a>,
    content: std::ops::Range<usize>,
}

fn leaf(doc: &Doc, pos: usize) -> Option<Leaf<'_>> {
    let p = context::path(doc, pos);
    let n = p.into_iter().last()?;
    if matches!(
        n.block.kind,
        BlockKind::BlockQuote { .. }
            | BlockKind::List { .. }
            | BlockKind::ListItem { .. }
            | BlockKind::FootnoteDef { .. }
    ) {
        return None;
    }
    let content = match (n.block.lines.first(), n.block.lines.last()) {
        (Some(a), Some(z)) => n.abs(a.span.start)..n.abs(z.span.end),
        _ => n.span(),
    };
    Some(Leaf { node: n, content })
}

/// Quote prefix (`> > `) of the line containing `pos`.
fn quote_prefix(s: &str, pos: usize) -> String {
    let ls = line_start(s, pos);
    let mut p = String::new();
    for c in s[ls..].chars() {
        match c {
            '>' | ' ' if p.len() < 64 => p.push(c),
            _ => break,
        }
    }
    let p = p.trim_end().to_owned();
    if p.is_empty() { p } else { p + " " }
}

/// Syntax that Enter turns into a block (ED-004): an opening code fence or `$$`
/// gets its closing line; `|a|b|` becomes a table with a delimiter row.
fn expand_on_enter(doc: &Doc, s: &str, sel: Selection) -> Option<Transaction> {
    if !sel.is_empty() {
        return None;
    }
    let nl = doc.newline();
    let pos = sel.head;
    let ls = line_start(s, pos);
    let le = line_end(s, pos);
    if pos != le {
        return None;
    }
    let line = &s[ls..le];
    let prefix_len = line.len() - line.trim_start_matches(['>', ' ']).len();
    let body = &line[prefix_len..];
    let prefix = &line[..prefix_len];
    let in_code = context::in_code_content(doc, ls.saturating_sub(1)) && ls > 0;
    // Opening fence: ``` or ~~~ with optional info, and no closing fence below.
    let fence_char = body.chars().next().filter(|c| *c == '`' || *c == '~');
    if let Some(c) = fence_char {
        let n = body.chars().take_while(|x| *x == c).count();
        if n >= 3 && !(c == '`' && body[n..].contains('`')) && !in_code {
            if let Some(cb) = context::code_block(doc, pos) {
                // Only when this fence is the opener of an unclosed block.
                let closed = doc.ast().get(doc.ast().index_at(pos)?).is_some_and(|r| {
                    let mut closed = false;
                    r.block.walk(&mut |b| {
                        if let BlockKind::CodeBlock { fence: Some(f) } = &b.kind
                            && r.abs(f.open.start) <= pos
                            && pos <= r.abs(f.open.end)
                            && f.close.is_some()
                        {
                            closed = true;
                        }
                    });
                    closed
                });
                let _ = cb;
                if closed {
                    return None;
                }
            }
            let fence = &body[..n];
            let ins = format!("{nl}{prefix}{nl}{prefix}{fence}");
            let caret = pos + nl.len() + prefix.len();
            return Some(replace(pos..pos, &ins, sel, Selection::caret(caret)));
        }
    }
    if body.trim_end() == "$$" && !in_code {
        let ins = format!("{nl}{prefix}{nl}{prefix}$$");
        let caret = pos + nl.len() + prefix.len();
        // Only if this `$$` opens (no closing `$$` already follows).
        let rest = &s[le..];
        if !rest
            .lines()
            .skip(1)
            .any(|l| l.trim_start_matches(['>', ' ']).trim_end() == "$$")
        {
            return Some(replace(pos..pos, &ins, sel, Selection::caret(caret)));
        }
    }
    // `|a|b|` → table (single-line paragraph starting with a pipe).
    let t = body.trim();
    if t.starts_with('|') && t.len() > 1 && !in_code {
        let leaf_is_para = context::path(doc, pos).last().is_some_and(|n| {
            matches!(n.block.kind, BlockKind::Paragraph) && n.block.lines.len() == 1
        });
        if leaf_is_para {
            let cells = inkra_markdown::scan::table_cells(t.as_bytes()).len().max(1);
            let delim = format!("|{}", " --- |".repeat(cells));
            let row = format!("|{}", "   |".repeat(cells));
            let ins = format!("{nl}{prefix}{delim}{nl}{prefix}{row}");
            let caret = pos + nl.len() + prefix.len() + delim.len() + nl.len() + prefix.len() + 2;
            let mut tx = replace(pos..pos, &ins, sel, Selection::caret(caret));
            if !t.ends_with('|') {
                tx.edits.insert(
                    0,
                    Edit {
                        range: pos..pos,
                        text: " |".to_owned(),
                    },
                );
                tx = Transaction::many(vec![Edit {
                    range: pos..pos,
                    text: format!(" |{ins}"),
                }])
                .with_selection_before(sel)
                .with_selection_after(Selection::caret(caret + 2));
            }
            return Some(tx);
        }
    }
    None
}

/// Enter / Shift+Enter in live mode.
pub fn newline(doc: &Doc, sel: Selection, kind: Break) -> Transaction {
    let s = text(doc);
    let nl = doc.newline();
    let pos = sel.start();
    if kind == Break::Enter
        && let Some(t) = expand_on_enter(doc, &s, sel)
    {
        return t;
    }
    // At the end of an opening fence: a new first code line.
    if context::at_fence_open_end(doc, pos) && sel.is_empty() {
        return replace(pos..pos, nl, sel, Selection::caret(pos + nl.len()));
    }
    // Code, math, front matter, HTML: plain line breaks (code keeps indentation).
    if context::in_code_content(doc, pos) || context::list_item(doc, pos).is_some() {
        return ops::newline(doc, sel, kind);
    }
    if let Some(l) = leaf(doc, pos)
        && matches!(
            l.node.block.kind,
            BlockKind::Table { .. } | BlockKind::TableRow { .. } | BlockKind::TableCell
        )
    {
        // Tables never get line breaks from Enter (ED-013); rows are added by
        // table commands.
        return Transaction::default()
            .with_selection_before(sel)
            .with_selection_after(sel);
    }
    let quote = context::quote_depth(doc, pos) > 0;
    let prefix = if quote {
        quote_prefix(&s, pos)
    } else {
        String::new()
    };
    let r = sel.range();
    if kind == Break::Soft {
        let ins = format!("{nl}{prefix}");
        return replace(r.clone(), &ins, sel, Selection::caret(r.start + ins.len()));
    }
    if quote {
        let ls = line_start(&s, pos);
        let le = line_end(&s, pos);
        let line_rest = s[ls..le].trim_start_matches(['>', ' ']);
        if line_rest.trim().is_empty() {
            // Empty quote paragraph: leave the quote (drop this line's prefix and a
            // preceding empty `>` line).
            let mut start = ls;
            if ls > 0 {
                let pls = line_start(&s, ls - 1);
                if s[pls..ls]
                    .trim_end_matches(['\n', '\r'])
                    .trim_start_matches(['>', ' '])
                    .is_empty()
                {
                    start = pls;
                }
            }
            let ins = if start < ls {
                nl.to_owned()
            } else {
                String::new()
            };
            let t = ins;
            return replace(start..le, &t, sel, Selection::caret(start + t.len()));
        }
        let bare = prefix.trim_end();
        let ins = format!("{nl}{bare}{nl}{prefix}");
        return replace(r.clone(), &ins, sel, Selection::caret(r.start + ins.len()));
    }
    let ins = format!("{nl}{nl}");
    replace(r.clone(), &ins, sel, Selection::caret(r.start + ins.len()))
}

/// Backspace in live mode.
pub fn delete_backward(
    doc: &Doc,
    sel: Selection,
    unit: Unit,
    prefs: &EditPrefs,
) -> Option<Transaction> {
    if !sel.is_empty() || unit != Unit::Char {
        return ops::delete_backward(doc, sel, unit, prefs);
    }
    let s = text(doc);
    let pos = sel.head;
    // List item: at the content start of its first paragraph.
    if let Some(info) = context::list_item(doc, pos) {
        let content_start = info.task.as_ref().map_or(info.marker.end, |t| t.0.end);
        let content_start = content_start
            + s[content_start..]
                .bytes()
                .take_while(|b| *b == b' ')
                .count()
                .min(4);
        if pos == content_start || (info.empty && pos >= info.marker.start && pos <= content_start)
        {
            if info.depth > 0 {
                return ops::tab(doc, sel, true, prefs);
            }
            // Becomes a paragraph after the list (or splits the list).
            let ls = line_start(&s, info.marker.start);
            let indent = &s[ls..info.marker.start];
            let quote = quote_prefix(&s, info.marker.start);
            let keep = if quote.is_empty() {
                String::new()
            } else {
                quote.clone()
            };
            let _ = indent;
            let nl = doc.newline();
            let mut edits = vec![];
            let sep = if info.index > 0 {
                format!("{nl}{}", keep.trim_end())
            } else {
                String::new()
            };
            edits.push(Edit {
                range: ls..content_start,
                text: format!("{sep}{keep}"),
            });
            // A following sibling must not continue as a lazy line of this paragraph.
            if info.index + 1 < info.siblings.len() {
                let next = &info.siblings[info.index + 1];
                let nls = line_start(&s, next.start);
                edits.push(Edit {
                    range: nls..nls,
                    text: format!("{}{nl}", keep.trim_end()),
                });
            }
            let caret = ls + sep.len() + keep.len();
            return Some(
                Transaction::many(edits)
                    .with_selection_before(sel)
                    .with_selection_after(Selection::caret(caret)),
            );
        }
    }
    let Some(l) = leaf(doc, pos) else {
        return ops::delete_backward(doc, sel, unit, prefs);
    };
    match &l.node.block.kind {
        BlockKind::Heading { marker, setext, .. } if pos == l.content.start => {
            if *setext {
                // Remove the underline line.
                let m = l.node.abs_range(marker);
                let ls = line_start(&s, m.start);
                return Some(replace(
                    ls.saturating_sub(1).max(l.content.end)..m.end,
                    "",
                    sel,
                    Selection::caret(pos),
                ));
            }
            let m = l.node.abs_range(marker);
            return Some(replace(
                m.start..l.content.start,
                "",
                sel,
                Selection::caret(m.start),
            ));
        }
        BlockKind::CodeBlock { .. }
            if l.node.block.lines.iter().all(|x| x.span.is_empty())
                && context::in_code_content(doc, pos) =>
        {
            // Empty code block: delete it.
            let span = l.node.span();
            return Some(replace(span.clone(), "", sel, Selection::caret(span.start)));
        }
        BlockKind::Paragraph if pos == l.content.start => {
            // First paragraph of a quote: unquote one level.
            if context::quote_depth(doc, pos) > 0 {
                let ls = line_start(&s, pos);
                let prefix_end = pos;
                let quote = quote_prefix(&s, pos);
                let lvl_removed = quote
                    .trim_end()
                    .strip_suffix('>')
                    .map(|p| p.trim_end().to_owned())
                    .unwrap_or_default();
                let new_prefix = if lvl_removed.is_empty() {
                    String::new()
                } else {
                    lvl_removed + " "
                };
                // Rewrite the prefix of every line of the paragraph.
                let mut edits = Vec::new();
                let mut line = ls;
                loop {
                    let le = line_end(&s, line);
                    let pe = line
                        + s[line..le]
                            .chars()
                            .take_while(|c| *c == '>' || *c == ' ')
                            .map(char::len_utf8)
                            .sum::<usize>();
                    edits.push(Edit {
                        range: line..pe,
                        text: new_prefix.clone(),
                    });
                    if le >= l.content.end || le >= s.len() {
                        break;
                    }
                    line = le + if s[le..].starts_with("\r\n") { 2 } else { 1 };
                }
                // Separate from quote content that follows.
                let after = line_end(&s, l.content.end);
                let nl = doc.newline();
                if after < s.len() {
                    let next = after + if s[after..].starts_with("\r\n") { 2 } else { 1 };
                    if next < s.len() && s[next..].trim_start().starts_with('>') {
                        edits.push(Edit {
                            range: after..after,
                            text: format!("{nl}{}", new_prefix.trim_end()),
                        });
                    }
                }
                let caret = ls + new_prefix.len();
                let _ = prefix_end;
                return Some(
                    Transaction::many(edits)
                        .with_selection_before(sel)
                        .with_selection_after(Selection::caret(caret)),
                );
            }
            return merge_with_previous(doc, &s, sel, l.content.start);
        }
        BlockKind::Heading { .. } | BlockKind::Paragraph if pos <= l.content.start => {
            return merge_with_previous(doc, &s, sel, l.content.start);
        }
        _ => {}
    }
    if pos <= l.content.start && !context::in_code_content(doc, pos) {
        return merge_with_previous(doc, &s, sel, pos);
    }
    ops::delete_backward(doc, sel, unit, prefs)
}

/// Joins the leaf starting at `start` with the previous leaf (ED-012).
fn merge_with_previous(doc: &Doc, s: &str, sel: Selection, start: usize) -> Option<Transaction> {
    if start == 0 {
        return None;
    }
    // Find the previous non-blank position.
    let before = s[..start].trim_end_matches([' ', '\t', '\n', '\r', '>']);
    if before.is_empty() {
        // Only blank space before: delete it.
        return Some(replace(0..start, "", sel, Selection::caret(0)));
    }
    let prev_end = before.len();
    let prev = leaf(doc, prev_end.saturating_sub(1));
    match prev.as_ref().map(|l| &l.node.block.kind) {
        Some(
            BlockKind::CodeBlock { .. }
            | BlockKind::Table { .. }
            | BlockKind::TableCell
            | BlockKind::TableRow { .. }
            | BlockKind::MathBlock { .. }
            | BlockKind::HtmlBlock { .. }
            | BlockKind::FrontMatter { .. },
        ) => {
            // Move into the end of the previous block without merging.
            let l = prev.unwrap_or_else(|| unreachable!());
            let end = if matches!(
                l.node.block.kind,
                BlockKind::CodeBlock { .. } | BlockKind::MathBlock { .. }
            ) {
                l.content.end
            } else {
                l.node.span().end
            };
            Some(
                Transaction::default()
                    .with_selection_before(sel)
                    .with_selection_after(Selection::caret(end)),
            )
        }
        Some(BlockKind::ThematicBreak) => {
            let l = prev.unwrap_or_else(|| unreachable!());
            let span = l.node.span();
            Some(replace(
                span.start..start,
                "",
                sel,
                Selection::caret(span.start),
            ))
        }
        _ => {
            // Delete the gap (blank lines and container prefixes) between the blocks.
            Some(replace(
                prev_end..start,
                "",
                sel,
                Selection::caret(prev_end),
            ))
        }
    }
}

/// Delete (forward) in live mode.
pub fn delete_forward(doc: &Doc, sel: Selection, unit: Unit) -> Option<Transaction> {
    if !sel.is_empty() || unit != Unit::Char {
        return ops::delete_forward(doc, sel, unit);
    }
    let s = text(doc);
    let pos = sel.head;
    if let Some(l) = leaf(doc, pos) {
        let mergeable = matches!(
            l.node.block.kind,
            BlockKind::Paragraph | BlockKind::Heading { .. }
        );
        if mergeable && pos == l.content.end && !context::in_code_content(doc, pos) {
            let after = &s[pos..];
            let skip = after.len() - after.trim_start_matches([' ', '\t', '\n', '\r', '>']).len();
            if skip == 0 {
                return ops::delete_forward(doc, sel, unit);
            }
            let next = pos + skip;
            if next >= s.len() {
                return Some(replace(pos..s.len(), "", sel, Selection::caret(pos)));
            }
            // Drop the next block's leading syntax (heading marker, list marker).
            let mut end = next;
            if let Some(nl) = leaf(doc, next) {
                if matches!(
                    nl.node.block.kind,
                    BlockKind::Heading { .. } | BlockKind::Paragraph
                ) {
                    end = nl.content.start;
                }
                if matches!(
                    nl.node.block.kind,
                    BlockKind::CodeBlock { .. }
                        | BlockKind::Table { .. }
                        | BlockKind::ThematicBreak
                        | BlockKind::MathBlock { .. }
                ) {
                    return Some(
                        Transaction::default()
                            .with_selection_before(sel)
                            .with_selection_after(Selection::caret(pos)),
                    );
                }
            }
            if let Some(info) = context::list_item(doc, next) {
                end = info.task.as_ref().map_or(info.marker.end, |t| t.0.end);
                end += s[end..].bytes().take_while(|b| *b == b' ').count().min(4);
            }
            return Some(replace(pos..end, "", sel, Selection::caret(pos)));
        }
    }
    ops::delete_forward(doc, sel, unit)
}
