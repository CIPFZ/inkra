//! Editing operations (ED-010..ED-016, ED-100, ED-101). Each function computes a
//! [`Transaction`] from the document and selection; nothing is applied here, so
//! every operation is a pure, unit-testable function.

use inkra_doc::{Doc, Edit, Selection, Transaction};
use inkra_markdown::ast::ListKind;
use inkra_text::segment;

use super::context::{self, ItemInfo};

/// Preferences that affect editing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditPrefs {
    /// Auto-pair brackets and quotes (PR-031, ED-100).
    pub pair_brackets: bool,
    /// Auto-pair Markdown symbols `*` `_` `~` `=` (PR-032 / MD-077).
    pub pair_markdown: bool,
    /// Pair `$` (only when inline math is enabled, MD-060).
    pub pair_dollar: bool,
    /// Tab width in code blocks (PR-041).
    pub tab_width: usize,
    /// Insert spaces instead of a tab in code blocks (PR-041).
    pub use_spaces: bool,
    /// Renumber ordered lists after edits (MD-076).
    pub renumber: bool,
    /// Ordered lists always start at 1 (ED-101); otherwise the start number is kept.
    pub list_from_one: bool,
}

impl Default for EditPrefs {
    fn default() -> Self {
        Self {
            pair_brackets: true,
            pair_markdown: false,
            pair_dollar: false,
            tab_width: 4,
            use_spaces: true,
            renumber: true,
            list_from_one: false,
        }
    }
}

/// Granularity of deletion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    /// One grapheme.
    Char,
    /// To the previous/next word boundary.
    Word,
    /// To the start/end of the source line.
    Line,
}

fn text(doc: &Doc) -> String {
    doc.text().to_string()
}

fn line_start(s: &str, pos: usize) -> usize {
    s[..pos].rfind(['\n', '\r']).map_or(0, |i| i + 1)
}

fn line_end(s: &str, pos: usize) -> usize {
    s[pos..].find(['\n', '\r']).map_or(s.len(), |i| pos + i)
}

fn closing_of(c: char) -> Option<char> {
    Some(match c {
        '(' => ')',
        '[' => ']',
        '{' => '}',
        '"' => '"',
        '\'' => '\'',
        '`' => '`',
        '*' => '*',
        '_' => '_',
        '~' => '~',
        '=' => '=',
        '$' => '$',
        _ => return None,
    })
}

fn is_markdown_symbol(c: char) -> bool {
    matches!(c, '*' | '_' | '~' | '=')
}

fn tx(edits: Vec<Edit>, before: Selection, after: Selection) -> Transaction {
    Transaction::many(edits)
        .with_selection_before(before)
        .with_selection_after(after)
}

fn replace(
    range: std::ops::Range<usize>,
    s: &str,
    before: Selection,
    after: Selection,
) -> Transaction {
    tx(
        vec![Edit {
            range,
            text: s.to_owned(),
        }],
        before,
        after,
    )
}

/// Text typed by the user (ED-010) with auto-pairing (ED-100).
pub fn type_text(doc: &Doc, sel: Selection, input: &str, p: &EditPrefs) -> Transaction {
    let s = text(doc);
    let pos = sel.head.min(s.len());
    let mut chars = input.chars();
    let single = match (chars.next(), chars.next()) {
        (Some(c), None) => Some(c),
        _ => None,
    };
    if let Some(c) = single {
        let in_code = context::in_code_content(doc, pos) || context::in_inline_code(doc, pos);
        let allowed = |c: char| {
            if is_markdown_symbol(c) {
                p.pair_markdown && !in_code
            } else if c == '$' {
                p.pair_dollar && !in_code
            } else if c == '`' {
                p.pair_brackets && !context::in_code_content(doc, pos)
            } else {
                p.pair_brackets
            }
        };
        // Wrap a selection.
        if !sel.is_empty() {
            if let Some(close) = closing_of(c).filter(|_| allowed(c)) {
                let r = sel.range();
                let inner = &s[r.clone()];
                let new = format!("{c}{inner}{close}");
                let a = r.start + c.len_utf8();
                return replace(r, &new, sel, Selection::new(a, a + inner.len()));
            }
        } else {
            let next = s[pos..].chars().next();
            let prev = s[..pos].chars().next_back();
            // Typing a closer that is already there: step over it.
            if matches!(
                c,
                ')' | ']' | '}' | '"' | '\'' | '`' | '$' | '*' | '_' | '~' | '='
            ) && next == Some(c)
                && allowed(if c == ')' {
                    '('
                } else if c == ']' {
                    '['
                } else if c == '}' {
                    '{'
                } else {
                    c
                })
                && pair_was_inserted(&s, pos, c)
            {
                return Transaction::default()
                    .with_selection_before(sel)
                    .with_selection_after(Selection::caret(pos + c.len_utf8()));
            }
            if let Some(close) = closing_of(c).filter(|_| allowed(c)) {
                let next_ok = next
                    .is_none_or(|n| n.is_whitespace() || ")]}>,.;:!?".contains(n) || n == close);
                let symmetric = c == close;
                let prev_word = prev.is_some_and(|ch| ch.is_alphanumeric());
                // Do not pair quotes inside words ("don't") or before word chars.
                if next_ok && !(symmetric && prev_word) {
                    let new = format!("{c}{close}");
                    let a = pos + c.len_utf8();
                    return replace(pos..pos, &new, sel, Selection::caret(a));
                }
            }
        }
    }
    let r = sel.range();
    let end = r.start + input.len();
    replace(r, input, sel, Selection::caret(end))
}

/// Whether the closer at `pos` likely belongs to a pair whose opener is on this
/// line before the caret (so typing it should step over).
fn pair_was_inserted(s: &str, pos: usize, close: char) -> bool {
    let open = match close {
        ')' => '(',
        ']' => '[',
        '}' => '{',
        c => c,
    };
    let ls = line_start(s, pos);
    let before = &s[ls..pos];
    if open == close {
        before.matches(open).count() % 2 == 1
    } else {
        before.matches(open).count() > before.matches(close).count()
    }
}

/// Backspace (ED-012 in source mode): deletes the selection, or backwards by `unit`.
/// An opener directly followed by its auto-inserted closer is deleted together.
pub fn delete_backward(
    doc: &Doc,
    sel: Selection,
    unit: Unit,
    p: &EditPrefs,
) -> Option<Transaction> {
    let s = text(doc);
    if !sel.is_empty() {
        let r = sel.range();
        return Some(replace(r.clone(), "", sel, Selection::caret(r.start)));
    }
    let pos = sel.head.min(s.len());
    if pos == 0 {
        return None;
    }
    let start = match unit {
        Unit::Char => {
            // CRLF counts as one unit.
            if s[..pos].ends_with("\r\n") {
                pos - 2
            } else {
                segment::prev_grapheme(&s, pos)
            }
        }
        Unit::Word => segment::prev_word(&s, pos),
        Unit::Line => {
            let ls = line_start(&s, pos);
            if ls == pos {
                segment::prev_grapheme(&s, pos)
            } else {
                ls
            }
        }
    };
    let mut end = pos;
    if unit == Unit::Char && p.pair_brackets {
        let prev = s[start..pos].chars().next();
        let next = s[pos..].chars().next();
        if let (Some(o), Some(n)) = (prev, next)
            && (closing_of(o) == Some(n) && !is_markdown_symbol(o)
                || p.pair_markdown && is_markdown_symbol(o) && o == n)
        {
            end = pos + n.len_utf8();
        }
    }
    Some(replace(start..end, "", sel, Selection::caret(start)))
}

/// Delete (forward) by `unit`.
pub fn delete_forward(doc: &Doc, sel: Selection, unit: Unit) -> Option<Transaction> {
    let s = text(doc);
    if !sel.is_empty() {
        let r = sel.range();
        return Some(replace(r.clone(), "", sel, Selection::caret(r.start)));
    }
    let pos = sel.head.min(s.len());
    if pos >= s.len() {
        return None;
    }
    let end = match unit {
        Unit::Char => {
            if s[pos..].starts_with("\r\n") {
                pos + 2
            } else {
                segment::next_grapheme(&s, pos)
            }
        }
        Unit::Word => segment::next_word(&s, pos),
        Unit::Line => {
            let le = line_end(&s, pos);
            if le == pos {
                segment::next_grapheme(&s, pos)
            } else {
                le
            }
        }
    };
    Some(replace(pos..end, "", sel, Selection::caret(pos)))
}

/// Kind of line break.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Break {
    /// Enter.
    Enter,
    /// Shift+Enter: soft line break.
    Soft,
}

fn next_marker(info: &ItemInfo, s: &str) -> String {
    let m = &s[info.marker.clone()];
    let base = match info.kind {
        ListKind::Bullet(_) => m.to_owned(),
        ListKind::Ordered(d) => format!("{}{d}", info.number.unwrap_or(0) + 1),
    };
    if info.task.is_some() {
        format!("{base} [ ]")
    } else {
        base
    }
}

/// Enter / Shift+Enter in source mode: inserts a line break and continues
/// list items, block quotes and code indentation (ED-013, ED-101).
pub fn newline(doc: &Doc, sel: Selection, kind: Break) -> Transaction {
    let s = text(doc);
    let nl = doc.newline();
    let r = sel.range();
    let pos = r.start;
    let ls = line_start(&s, pos);
    let line = &s[ls..line_end(&s, pos)];

    // Code: keep the current line's indentation.
    if context::in_code_content(doc, pos) && context::code_block(doc, pos).is_some() {
        let indent: String = line
            .chars()
            .take_while(|c| *c == ' ' || *c == '\t' || *c == '>')
            .collect();
        let ins = format!("{nl}{indent}");
        return replace(r.clone(), &ins, sel, Selection::caret(pos + ins.len()));
    }

    // The caret after `- ` lies past the (empty) item's span: probe the last
    // non-blank character of the line before the caret too.
    let probe = ls + s[ls..pos].trim_end().len();
    let item = context::list_item(doc, pos).or_else(|| {
        (probe > ls)
            .then(|| context::list_item(doc, probe))
            .flatten()
    });
    if let Some(info) = item {
        let marker_ls = line_start(&s, info.marker.start);
        let prefix = &s[marker_ls..info.marker.start];
        let after_marker_end = info.task.as_ref().map_or(info.marker.end, |t| t.0.end);
        let le = line_end(&s, after_marker_end);
        let gap: String = s[after_marker_end..le]
            .chars()
            .take_while(|c| *c == ' ')
            .take(4)
            .collect();
        let gap = if gap.is_empty() { " ".to_owned() } else { gap };
        let content_empty = s[after_marker_end..le].trim().is_empty()
            && marker_ls == ls
            && line_end(&s, info.span.end.min(s.len())) == le;
        let cont_prefix: String = prefix.to_owned() + &" ".repeat(info.marker.len() + gap.len());
        if kind == Break::Soft {
            let ins = format!("{nl}{cont_prefix}");
            return replace(r.clone(), &ins, sel, Selection::caret(pos + ins.len()));
        }
        if content_empty && pos >= after_marker_end {
            // Empty item: outdent when nested, otherwise end the list.
            if info.depth > 0 {
                let remove = prefix.len().min(indent_unit_before(&s, &info, doc));
                let a = marker_ls + prefix.len() - remove;
                let t = replace(
                    a..marker_ls + prefix.len(),
                    "",
                    sel,
                    Selection::caret(pos - remove),
                );
                return t;
            }
            // Leave a blank line so the following text is a new paragraph, not a
            // lazy continuation of the last item.
            let prev_blank = marker_ls == 0
                || s[line_start(&s, marker_ls - 1)..marker_ls]
                    .trim()
                    .is_empty();
            let ins = if prev_blank || !prefix.trim().is_empty() {
                String::new()
            } else {
                nl.to_owned()
            };
            let caret = info.marker.start + ins.len();
            return replace(info.marker.start..le, &ins, sel, Selection::caret(caret));
        }
        let marker = next_marker(&info, &s);
        let blank = if info.tight {
            String::new()
        } else {
            format!("{nl}{}", prefix.trim_end_matches([' ', '\t']))
        };
        let ins = format!("{blank}{nl}{prefix}{marker}{gap}");
        return replace(r.clone(), &ins, sel, Selection::caret(pos + ins.len()));
    }

    if kind == Break::Enter && context::quote_depth(doc, pos) > 0 {
        let prefix: String = line
            .chars()
            .take_while(|c| *c == '>' || *c == ' ')
            .collect();
        if !prefix.is_empty() {
            if line.trim_start_matches(['>', ' ']).is_empty() && pos == ls + line.len() {
                // Empty quote line: leave the quote.
                return replace(ls..ls + line.len(), "", sel, Selection::caret(ls));
            }
            let ins = format!("{nl}{prefix}");
            return replace(r.clone(), &ins, sel, Selection::caret(pos + ins.len()));
        }
    }

    replace(r.clone(), nl, sel, Selection::caret(pos + nl.len()))
}

fn indent_unit_before(s: &str, info: &ItemInfo, doc: &Doc) -> usize {
    // Distance between this item's marker column and its parent item's marker column.
    let marker_col = info.marker.start - line_start(s, info.marker.start);
    let parent = context::path(doc, info.marker.start);
    let mut cols = Vec::new();
    for n in &parent {
        if let inkra_markdown::BlockKind::ListItem { marker, .. } = &n.block.kind {
            let m = n.abs_range(marker);
            cols.push(m.start - line_start(s, m.start));
        }
    }
    // The last entry is this item; the one before it is the parent.
    if cols.len() >= 2 {
        marker_col.saturating_sub(cols[cols.len() - 2]).max(1)
    } else {
        marker_col
    }
}

/// Tab / Shift+Tab (ED-014): list items are indented/outdented; code lines are
/// indented with spaces or a tab; elsewhere four spaces are inserted.
pub fn tab(doc: &Doc, sel: Selection, outdent: bool, p: &EditPrefs) -> Option<Transaction> {
    let s = text(doc);
    let pos = sel.start();
    let unit = if p.use_spaces {
        " ".repeat(p.tab_width)
    } else {
        "\t".to_owned()
    };
    let ls0 = line_start(&s, pos);
    let probe = ls0 + s[ls0..pos].trim_end().len();
    let item = context::list_item(doc, pos).or_else(|| {
        (probe > ls0)
            .then(|| context::list_item(doc, probe))
            .flatten()
    });
    if let Some(info) = item {
        let item_lines = lines_in(&s, info.span.clone());
        if outdent {
            if info.depth == 0 {
                return None;
            }
            let n = indent_unit_before(&s, &info, doc);
            let mut edits = Vec::new();
            for ls in &item_lines {
                let lead = s[*ls..].bytes().take_while(|b| *b == b' ').count().min(n);
                if lead > 0 {
                    edits.push(Edit {
                        range: *ls..*ls + lead,
                        text: String::new(),
                    });
                }
            }
            let removed_before = |p: usize| {
                edits
                    .iter()
                    .filter(|e| e.range.end <= p)
                    .map(|e| e.range.len())
                    .sum::<usize>()
            };
            let after = Selection::new(
                sel.anchor - removed_before(sel.anchor),
                sel.head - removed_before(sel.head),
            );
            return Some(tx(edits, sel, after));
        }
        if info.index == 0 {
            return None; // the first item cannot be nested
        }
        // Indent to the previous sibling's content column.
        let prev = &info.siblings[info.index - 1];
        let prev_ls = line_start(&s, prev.start);
        let prev_content_col = {
            let after = prev.end;
            let gap = s[after..]
                .bytes()
                .take_while(|b| *b == b' ')
                .count()
                .clamp(1, 4);
            after - prev_ls + gap
        };
        let my_col = info.marker.start - line_start(&s, info.marker.start);
        let n = prev_content_col.saturating_sub(my_col).max(1);
        let pad = " ".repeat(n);
        let mut edits: Vec<Edit> = item_lines
            .iter()
            .map(|ls| Edit {
                range: *ls..*ls,
                text: pad.clone(),
            })
            .collect();
        let mut number_delta: isize = 0;
        // An ordered item that starts a new sub-list becomes number 1.
        if matches!(info.kind, ListKind::Ordered(_))
            && !context::item_ends_with_list(doc, prev.start)
        {
            let digits = info.marker.start..info.marker.end - 1;
            if &s[digits.clone()] != "1" {
                number_delta = 1 - digits.len() as isize;
                edits.push(Edit {
                    range: digits,
                    text: "1".to_owned(),
                });
            }
        }
        let marker_end = info.marker.end;
        let added_before = |p: usize| {
            let lines = item_lines.iter().filter(|ls| **ls <= p).count() * n;
            let num = if p >= marker_end { number_delta } else { 0 };
            (lines as isize + num) as usize
        };
        let after = Selection::new(
            sel.anchor + added_before(sel.anchor),
            sel.head + added_before(sel.head),
        );
        return Some(tx(edits, sel, after));
    }
    if context::in_code_content(doc, pos) && !sel.is_empty() && s[sel.range()].contains('\n')
        || outdent
    {
        let lines = lines_in(&s, sel.range());
        let mut edits = Vec::new();
        for ls in &lines {
            if outdent {
                let lead = s[*ls..]
                    .bytes()
                    .take_while(|b| *b == b' ')
                    .count()
                    .min(p.tab_width);
                let lead = if lead == 0 && s[*ls..].starts_with('\t') {
                    1
                } else {
                    lead
                };
                if lead > 0 {
                    edits.push(Edit {
                        range: *ls..*ls + lead,
                        text: String::new(),
                    });
                }
            } else {
                edits.push(Edit {
                    range: *ls..*ls,
                    text: unit.clone(),
                });
            }
        }
        if edits.is_empty() {
            return None;
        }
        let shift = |p: usize| -> usize {
            let mut q = p as isize;
            for e in &edits {
                if e.range.start < p {
                    q += e.text.len() as isize - e.range.len() as isize;
                }
            }
            q.max(0) as usize
        };
        let after = Selection::new(shift(sel.anchor), shift(sel.head));
        return Some(tx(edits, sel, after));
    }
    let r = sel.range();
    let ins = if context::in_code_content(doc, pos) {
        unit
    } else {
        "    ".to_owned()
    };
    Some(replace(
        r.clone(),
        &ins,
        sel,
        Selection::caret(r.start + ins.len()),
    ))
}

/// Line starts of all lines touched by `range`.
fn lines_in(s: &str, range: std::ops::Range<usize>) -> Vec<usize> {
    let mut out = vec![line_start(s, range.start.min(s.len()))];
    let end = range.end.min(s.len());
    for (i, b) in s.as_bytes()[range.start.min(s.len())..end]
        .iter()
        .enumerate()
    {
        if *b == b'\n' {
            let ls = range.start + i + 1;
            if ls < end || (ls == end && ls < s.len() && range.is_empty()) {
                out.push(ls);
            }
        }
    }
    out
}

/// Renumbers the ordered list containing `pos` so numbers are consecutive
/// (ED-101, MD-076). Returns `None` when nothing changes.
pub fn renumber_list(doc: &Doc, pos: usize, p: &EditPrefs) -> Option<Transaction> {
    if !p.renumber {
        return None;
    }
    let s = text(doc);
    let path = context::path(doc, pos);
    let mut edits = Vec::new();
    for n in path.iter().rev() {
        if let inkra_markdown::BlockKind::List {
            kind: ListKind::Ordered(_),
            start,
            ..
        } = n.block.kind
        {
            let first = if p.list_from_one { 1 } else { start };
            for (i, item) in n.block.children.iter().enumerate() {
                if let inkra_markdown::BlockKind::ListItem { marker, .. } = &item.kind {
                    let m = n.abs_range(marker);
                    let digits = &s[m.start..m.end - 1];
                    let want = (first + i as u64).to_string();
                    if digits != want {
                        edits.push(Edit {
                            range: m.start..m.end - 1,
                            text: want,
                        });
                    }
                }
            }
            break;
        }
    }
    if edits.is_empty() {
        return None;
    }
    let t = Transaction::many(edits);
    Some(t)
}

/// Word range at `pos` (double-click, `Ctrl+D`).
pub fn word_at(doc: &Doc, pos: usize) -> Selection {
    let s = text(doc);
    let r = segment::word_at(&s, pos);
    Selection::new(r.start, r.end)
}

/// Source line at `pos` (`Ctrl+L`), including its line break.
pub fn line_at(doc: &Doc, pos: usize) -> Selection {
    let s = text(doc);
    let a = line_start(&s, pos.min(s.len()));
    let mut b = line_end(&s, pos.min(s.len()));
    if s[b..].starts_with("\r\n") {
        b += 2;
    } else if b < s.len() {
        b += 1;
    }
    Selection::new(a, b)
}

/// Paragraph (block) at `pos` (triple click).
pub fn block_at(doc: &Doc, pos: usize) -> Selection {
    let p = context::path(doc, pos);
    match p.last() {
        Some(n) => {
            let r = n.span();
            Selection::new(r.start, r.end)
        }
        None => line_at(doc, pos),
    }
}

/// `Ctrl+A` (ED-011): inside a code block first selects the block content.
pub fn select_all(doc: &Doc, sel: Selection) -> Selection {
    let len = doc.text().len();
    if let Some(n) = context::path(doc, sel.head)
        .iter()
        .rev()
        .find(|n| matches!(n.block.kind, inkra_markdown::BlockKind::CodeBlock { .. }))
        && let (Some(first), Some(last)) = (n.block.lines.first(), n.block.lines.last())
    {
        let r = n.abs(first.span.start)..n.abs(last.span.end);
        let inner = Selection::new(r.start, r.end);
        if sel.range() != r && !(sel.start() <= r.start && sel.end() >= r.end) {
            return inner;
        }
    }
    Selection::new(0, len)
}

/// Deletes the current line (`edit.deleteLine`).
pub fn delete_line(doc: &Doc, sel: Selection) -> Transaction {
    let l = line_at(doc, sel.head);
    replace(l.range(), "", sel, Selection::caret(l.start()))
}

/// Deletes the word at the caret (`Ctrl+Shift+D`).
pub fn delete_word(doc: &Doc, sel: Selection) -> Transaction {
    let w = word_at(doc, sel.head);
    replace(w.range(), "", sel, Selection::caret(w.start()))
}

/// Caret position one grapheme (or word) to the left of `pos`.
pub fn prev_pos(doc: &Doc, pos: usize, word: bool) -> usize {
    let t = doc.text();
    let pos = t.snap(pos);
    if pos == 0 {
        return 0;
    }
    let line = t.byte_to_line(pos);
    let ls = t.line_to_byte(line);
    if pos == ls {
        // Move to the end of the previous line (before its terminator).
        return t.line_content_range(line.saturating_sub(1)).end;
    }
    let s = t.slice(ls..pos);
    let rel = if word {
        segment::prev_word(&s, s.len())
    } else {
        segment::prev_grapheme(&s, s.len())
    };
    ls + rel
}

/// Caret position one grapheme (or word) to the right of `pos`.
pub fn next_pos(doc: &Doc, pos: usize, word: bool) -> usize {
    let t = doc.text();
    let pos = t.snap(pos);
    let line = t.byte_to_line(pos);
    let content = t.line_content_range(line);
    if pos >= content.end {
        return if line + 1 < t.len_lines() {
            t.line_to_byte(line + 1)
        } else {
            t.len()
        };
    }
    let s = t.slice(pos..content.end);
    let rel = if word {
        segment::next_word(&s, 0)
    } else {
        segment::next_grapheme(&s, 0)
    };
    pos + rel.max(1).min(s.len())
}
