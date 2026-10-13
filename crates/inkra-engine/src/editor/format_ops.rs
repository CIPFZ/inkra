//! Formatting and block conversion operations (SPEC 3.3, 3.4).

use inkra_doc::{Doc, Edit, Selection, Transaction};
use inkra_markdown::ast::{BlockKind, InlineKind, ListKind};

use super::context;

fn text(doc: &Doc) -> String {
    doc.text().to_string()
}

fn line_start(s: &str, pos: usize) -> usize {
    s[..pos.min(s.len())]
        .rfind(['\n', '\r'])
        .map_or(0, |i| i + 1)
}

fn line_end(s: &str, pos: usize) -> usize {
    s[pos.min(s.len())..]
        .find(['\n', '\r'])
        .map_or(s.len(), |i| pos + i)
}

fn next_line(s: &str, le: usize) -> usize {
    if s[le..].starts_with("\r\n") {
        le + 2
    } else {
        (le + 1).min(s.len())
    }
}

/// Inline formatting kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InlineFormat {
    /// `**x**`.
    Bold,
    /// `*x*`.
    Italic,
    /// `` `x` ``.
    Code,
    /// `~~x~~`.
    Strike,
    /// `<u>x</u>`.
    Underline,
    /// `==x==`.
    Highlight,
    /// `^x^`.
    Superscript,
    /// `~x~`.
    Subscript,
    /// `$x$`.
    Math,
    /// `<!-- x -->`.
    Comment,
}

fn markers(f: InlineFormat, content: &str, strong: &str, emph: &str) -> (String, String) {
    match f {
        InlineFormat::Bold => (strong.to_owned(), strong.to_owned()),
        InlineFormat::Italic => (emph.to_owned(), emph.to_owned()),
        InlineFormat::Code => {
            if content.contains('`') {
                ("`` ".to_owned(), " ``".to_owned())
            } else {
                ("`".to_owned(), "`".to_owned())
            }
        }
        InlineFormat::Strike => ("~~".into(), "~~".into()),
        InlineFormat::Underline => ("<u>".into(), "</u>".into()),
        InlineFormat::Highlight => ("==".into(), "==".into()),
        InlineFormat::Superscript => ("^".into(), "^".into()),
        InlineFormat::Subscript => ("~".into(), "~".into()),
        InlineFormat::Math => ("$".into(), "$".into()),
        InlineFormat::Comment => ("<!-- ".into(), " -->".into()),
    }
}

fn matches_kind(f: InlineFormat, k: &InlineKind) -> bool {
    match f {
        InlineFormat::Bold => matches!(k, InlineKind::Strong { .. }),
        InlineFormat::Italic => matches!(k, InlineKind::Emph { .. }),
        InlineFormat::Code => matches!(k, InlineKind::Code { .. }),
        InlineFormat::Strike => matches!(k, InlineKind::Strike { .. }),
        InlineFormat::Highlight => matches!(k, InlineKind::Highlight { .. }),
        InlineFormat::Superscript => matches!(k, InlineKind::Superscript { .. }),
        InlineFormat::Subscript => matches!(k, InlineKind::Subscript { .. }),
        InlineFormat::Math => matches!(k, InlineKind::Math { .. }),
        InlineFormat::Underline | InlineFormat::Comment => false,
    }
}

/// The innermost inline node of format `f` containing `pos`: `(open, close)`
/// absolute marker ranges.
/// Whether the caret is inside an element of format `f` (toolbar states).
pub fn is_inside(doc: &Doc, pos: usize, f: InlineFormat) -> bool {
    enclosing(doc, pos, f).is_some()
}

fn enclosing(
    doc: &Doc,
    pos: usize,
    f: InlineFormat,
) -> Option<(std::ops::Range<usize>, std::ops::Range<usize>)> {
    let p = context::path(doc, pos);
    let leaf = p.last()?;
    let rel = (pos as isize - leaf.delta) as usize;
    let mut found = None;
    let mut visit = |list: &[inkra_markdown::Inline]| {
        for i in list {
            i.walk(&mut |n| {
                if matches_kind(f, &n.kind) && n.span.start <= rel && rel <= n.span.end {
                    let (o, c) = match &n.kind {
                        InlineKind::Code { content, .. } | InlineKind::Math { content } => {
                            (n.span.start..content.start, content.end..n.span.end)
                        }
                        _ => n
                            .markers()
                            .unwrap_or((n.span.start..n.span.start, n.span.end..n.span.end)),
                    };
                    found = Some((leaf.abs_range(&o), leaf.abs_range(&c)));
                }
            });
        }
    };
    visit(&leaf.block.inlines);
    found
}

/// Toggles inline format `f` on the selection (ED-030…ED-042). Without a
/// selection, an empty pair is inserted with the caret between, or the markers of
/// the enclosing node are removed.
pub fn toggle_inline(
    doc: &Doc,
    sel: Selection,
    f: InlineFormat,
    strong: &str,
    emph: &str,
) -> Transaction {
    let s = text(doc);
    let r = sel.range();
    // Remove: the selection (or caret) is inside a node of this kind whose content
    // equals the selection, or there is no selection.
    if let Some((open, close)) = enclosing(doc, r.start, f) {
        let content = open.end..close.start;
        if sel.is_empty() || (r.start >= content.start && r.end <= content.end) {
            let edits = vec![
                Edit {
                    range: open.clone(),
                    text: String::new(),
                },
                Edit {
                    range: close.clone(),
                    text: String::new(),
                },
            ];
            let shift = |p: usize| {
                if p >= close.end {
                    p - open.len() - close.len()
                } else if p >= open.end {
                    p - open.len()
                } else {
                    p.min(open.start)
                }
            };
            let after = Selection::new(shift(sel.anchor), shift(sel.head));
            return Transaction::many(edits)
                .with_selection_before(sel)
                .with_selection_after(after);
        }
    }
    let inner = &s[r.clone()];
    // Selection already surrounded by the markers: unwrap.
    let (o, c) = markers(f, inner, strong, emph);
    if !sel.is_empty()
        && r.start >= o.len()
        && s[..r.start].ends_with(&o)
        && s[r.end..].starts_with(&c)
    {
        let edits = vec![
            Edit {
                range: r.start - o.len()..r.start,
                text: String::new(),
            },
            Edit {
                range: r.end..r.end + c.len(),
                text: String::new(),
            },
        ];
        return Transaction::many(edits)
            .with_selection_before(sel)
            .with_selection_after(Selection::new(r.start - o.len(), r.end - o.len()));
    }
    // Wrap (trailing whitespace stays outside the markers).
    let trimmed_end = r.start + inner.trim_end().len();
    let trimmed_start = r.start + (inner.len() - inner.trim_start().len());
    let (a, b) = if trimmed_start < trimmed_end {
        (trimmed_start, trimmed_end)
    } else {
        (r.start, r.end)
    };
    let edits = vec![
        Edit {
            range: a..a,
            text: o.clone(),
        },
        Edit {
            range: b..b,
            text: c.clone(),
        },
    ];
    let after = if sel.is_empty() {
        Selection::caret(a + o.len())
    } else {
        Selection::new(a + o.len(), b + o.len())
    };
    Transaction::many(edits)
        .with_selection_before(sel)
        .with_selection_after(after)
}

/// `[text](url)` (ED-039): with a selection and a URL, links the selection; else
/// places the caret inside `()`.
pub fn insert_link(doc: &Doc, sel: Selection, url: Option<&str>) -> Transaction {
    let s = text(doc);
    let r = sel.range();
    let label = &s[r.clone()];
    match url {
        Some(u) => {
            let t = format!("[{label}]({u})");
            let end = r.start + t.len();
            Transaction::single(r, &t)
                .with_selection_before(sel)
                .with_selection_after(Selection::caret(end))
        }
        None => {
            let t = format!("[{label}]()");
            let caret = r.start + t.len() - 1;
            Transaction::single(r, &t)
                .with_selection_before(sel)
                .with_selection_after(Selection::caret(caret))
        }
    }
}

/// `![alt](src)` (ED-040).
pub fn insert_image(doc: &Doc, sel: Selection, src: &str, alt: &str) -> Transaction {
    let r = sel.range();
    let _ = doc;
    let t = format!("![{alt}]({src})");
    let caret = if src.is_empty() {
        r.start + t.len() - 1
    } else {
        r.start + t.len()
    };
    Transaction::single(r, &t)
        .with_selection_before(sel)
        .with_selection_after(Selection::caret(caret))
}

/// A leaf block touched by the selection: content line starts and the kind.
struct LeafLines {
    /// Absolute start of the first content char of each line (after container
    /// prefixes).
    lines: Vec<usize>,
    kind: BlockKind,
    span: std::ops::Range<usize>,
}

fn leaves_in(doc: &Doc, range: std::ops::Range<usize>) -> Vec<LeafLines> {
    let ast = doc.ast();
    let mut out = Vec::new();
    let first = ast
        .first_starting_at_or_after(range.start)
        .saturating_sub(1);
    for i in first..ast.len() {
        let Some(r) = ast.get(i) else { break };
        if r.span().start > range.end {
            break;
        }
        r.block.walk(&mut |b| {
            let leaf = !matches!(
                b.kind,
                BlockKind::BlockQuote { .. }
                    | BlockKind::List { .. }
                    | BlockKind::ListItem { .. }
                    | BlockKind::FootnoteDef { .. }
                    | BlockKind::TableRow { .. }
                    | BlockKind::TableCell
            );
            let span = r.abs_span(&b.span);
            if leaf && span.end >= range.start && span.start <= range.end {
                let lines = if b.lines.is_empty() {
                    vec![span.start]
                } else {
                    b.lines.iter().map(|l| r.abs(l.span.start)).collect()
                };
                out.push(LeafLines {
                    lines,
                    kind: b.kind.clone(),
                    span,
                });
            }
        });
    }
    out
}

fn tx_from(edits: Vec<Edit>, sel: Selection) -> Transaction {
    let t = Transaction::many(edits).with_selection_before(sel);
    let mapped = t.clone().normalized(usize::MAX).map_selection(sel);
    t.with_selection_after(mapped)
}

/// Sets the heading level (1–6) of the blocks in the selection; `0` makes them
/// paragraphs (ED-050, ED-051). Applying the current level again toggles back to
/// a paragraph.
pub fn set_heading(doc: &Doc, sel: Selection, level: u8) -> Option<Transaction> {
    let s = text(doc);
    let leaves = leaves_in(doc, sel.range());
    let all_same = level > 0
        && leaves
            .iter()
            .all(|l| matches!(l.kind, BlockKind::Heading { level: lv, .. } if lv == level));
    let target = if all_same { 0 } else { level };
    let mut edits = Vec::new();
    for l in leaves {
        match &l.kind {
            BlockKind::Heading {
                marker,
                setext: false,
                ..
            } => {
                // `marker` is stored relative; recompute from the line text.
                let _ = marker;
                let ls = l.span.start;
                let hashes = s[ls..].bytes().take_while(|b| *b == b'#').count();
                let spaces = s[ls + hashes..]
                    .bytes()
                    .take_while(|b| *b == b' ' || *b == b'\t')
                    .count();
                let new = if target == 0 {
                    String::new()
                } else {
                    format!("{} ", "#".repeat(target as usize))
                };
                edits.push(Edit {
                    range: ls..ls + hashes + spaces,
                    text: new,
                });
            }
            BlockKind::Heading { setext: true, .. } => {
                // Convert to ATX: prefix and drop the underline line.
                let first = l.lines[0];
                if target > 0 {
                    edits.push(Edit {
                        range: first..first,
                        text: format!("{} ", "#".repeat(target as usize)),
                    });
                }
                let last_line_end = line_end(&s, *l.lines.last().unwrap_or(&first));
                edits.push(Edit {
                    range: last_line_end..l.span.end,
                    text: String::new(),
                });
            }
            BlockKind::Paragraph if target > 0 => {
                let first = l.lines[0];
                edits.push(Edit {
                    range: first..first,
                    text: format!("{} ", "#".repeat(target as usize)),
                });
                // Multi-line paragraphs: join lines so the whole paragraph is the heading.
                for &ls in &l.lines[1..] {
                    let prev_end = line_end(&s, line_start(&s, ls).saturating_sub(1));
                    edits.push(Edit {
                        range: prev_end..ls,
                        text: " ".to_owned(),
                    });
                }
            }
            _ => {}
        }
    }
    (!edits.is_empty()).then(|| tx_from(edits, sel))
}

/// Promotes (`up`) or demotes the heading level (ED-052).
pub fn shift_heading(doc: &Doc, sel: Selection, up: bool) -> Option<Transaction> {
    let l = leaves_in(doc, sel.range()).into_iter().next()?;
    let level = match l.kind {
        BlockKind::Heading { level, .. } => level,
        BlockKind::Paragraph => 7,
        _ => return None,
    };
    let new = if up {
        if level == 1 {
            return None;
        }
        level - 1
    } else {
        if level == 7 {
            return None;
        }
        if level == 6 { 0 } else { level + 1 }
    };
    let new = if new == 7 { 0 } else { new };
    if new == 0 {
        set_heading(doc, Selection::caret(l.span.start), 0).filter(|_| level != 7)
    } else {
        let s = Selection::caret(l.lines[0]);
        let mut t = set_heading(doc, s, new)?;
        t.selection_before = Some(sel);
        t.selection_after = Some(sel);
        Some(t)
    }
}

/// Toggles a block quote around the selected blocks (ED-056).
pub fn toggle_quote(doc: &Doc, sel: Selection) -> Option<Transaction> {
    let s = text(doc);
    let leaves = leaves_in(doc, sel.range());
    let start = leaves.first()?.span.start;
    let end = leaves.last()?.span.end;
    let first_line = line_start(&s, start);
    let quoted = context::quote_depth(doc, sel.start()) > 0;
    let mut edits = Vec::new();
    let mut ls = first_line;
    loop {
        let le = line_end(&s, ls);
        if quoted {
            let lead = s[ls..le].bytes().take_while(|b| *b == b' ').count();
            if s[ls + lead..].starts_with('>') {
                let n = if s[ls + lead + 1..].starts_with(' ') {
                    2
                } else {
                    1
                };
                edits.push(Edit {
                    range: ls + lead..ls + lead + n,
                    text: String::new(),
                });
            }
        } else {
            edits.push(Edit {
                range: ls..ls,
                text: if ls == le {
                    ">".to_owned()
                } else {
                    "> ".to_owned()
                },
            });
        }
        if le >= end || le >= s.len() {
            break;
        }
        ls = next_line(&s, le);
    }
    (!edits.is_empty()).then(|| tx_from(edits, sel))
}

/// List styles for [`toggle_list`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListStyle {
    /// `- `.
    Bullet,
    /// `1. `.
    Ordered,
    /// `- [ ] `.
    Task,
}

/// Toggles list markup on the selected blocks (ED-057..ED-059).
pub fn toggle_list(
    doc: &Doc,
    sel: Selection,
    style: ListStyle,
    bullet: char,
) -> Option<Transaction> {
    let s = text(doc);
    // Inside a list item: switch kind or remove.
    if let Some(info) = context::list_item(doc, sel.start()) {
        let same = match (style, info.kind) {
            (ListStyle::Bullet, ListKind::Bullet(_)) => info.task.is_none(),
            (ListStyle::Ordered, ListKind::Ordered(_)) => true,
            (ListStyle::Task, _) => info.task.is_some(),
            _ => false,
        };
        let mut edits = Vec::new();
        for (k, m) in info.siblings.iter().enumerate() {
            let after = s[m.end..].bytes().take_while(|b| *b == b' ').count().min(4);
            let task_len = if s[m.end + after..].starts_with("[ ] ")
                || s[m.end + after..].starts_with("[x] ")
                || s[m.end + after..].starts_with("[X] ")
            {
                4
            } else {
                0
            };
            let in_sel =
                m.start <= sel.end() && line_end(&s, m.start) >= line_start(&s, sel.start());
            if !in_sel && !same {
                continue;
            }
            if !in_sel {
                continue;
            }
            let replacement = if same {
                String::new()
            } else {
                match style {
                    ListStyle::Bullet => format!("{bullet} "),
                    ListStyle::Ordered => format!("{}. ", k + 1),
                    ListStyle::Task => format!("{bullet} [ ] "),
                }
            };
            edits.push(Edit {
                range: m.start..m.end + after + task_len,
                text: replacement,
            });
        }
        return (!edits.is_empty()).then(|| tx_from(edits, sel));
    }
    let leaves = leaves_in(doc, sel.range());
    let mut edits = Vec::new();
    for (k, l) in leaves
        .iter()
        .filter(|l| matches!(l.kind, BlockKind::Paragraph | BlockKind::Heading { .. }))
        .enumerate()
    {
        let n = k + 1;
        let marker = match style {
            ListStyle::Bullet => format!("{bullet} "),
            ListStyle::Ordered => format!("{n}. "),
            ListStyle::Task => format!("{bullet} [ ] "),
        };
        edits.push(Edit {
            range: l.lines[0]..l.lines[0],
            text: marker.clone(),
        });
        for &ls in &l.lines[1..] {
            edits.push(Edit {
                range: ls..ls,
                text: " ".repeat(marker.len()),
            });
        }
    }
    // Consecutive paragraphs separated by blank lines become one tight list.
    for w in leaves.windows(2) {
        let gap_start = line_end(&s, *w[0].lines.last().unwrap_or(&w[0].span.start));
        let gap_end = line_start(&s, w[1].lines[0]);
        if gap_end > gap_start + 1 && s[gap_start..gap_end].trim().is_empty() {
            let nl = doc.newline();
            edits.push(Edit {
                range: gap_start..gap_end,
                text: nl.to_owned(),
            });
        }
    }
    if edits.is_empty() {
        // Empty line: start a list.
        let marker = match style {
            ListStyle::Bullet => format!("{bullet} "),
            ListStyle::Ordered => "1. ".to_owned(),
            ListStyle::Task => format!("{bullet} [ ] "),
        };
        let p = sel.head;
        let end = p + marker.len();
        return Some(
            Transaction::single(p..p, &marker)
                .with_selection_before(sel)
                .with_selection_after(Selection::caret(end)),
        );
    }
    Some(tx_from(edits, sel))
}

/// Converts the current paragraph (or selection) into a fenced code block (ED-054).
pub fn code_fences(doc: &Doc, sel: Selection, lang: &str) -> Option<Transaction> {
    let s = text(doc);
    let nl = doc.newline();
    if context::code_block(doc, sel.head).is_some() {
        return None;
    }
    if !sel.is_empty() {
        let r = sel.range();
        let inner = &s[r.clone()];
        let before = if r.start > 0 && !s[..r.start].ends_with('\n') {
            nl
        } else {
            ""
        };
        let t = format!("{before}```{lang}{nl}{inner}{nl}```{nl}");
        let caret = r.start + before.len() + 3 + lang.len() + nl.len() + inner.len();
        return Some(
            Transaction::single(r, &t)
                .with_selection_before(sel)
                .with_selection_after(Selection::caret(caret)),
        );
    }
    let ls = line_start(&s, sel.head);
    let le = line_end(&s, sel.head);
    let leaf = leaves_in(doc, sel.head..sel.head).into_iter().next();
    let (a, b) = match &leaf {
        Some(l) if matches!(l.kind, BlockKind::Paragraph) => (l.lines[0], l.span.end),
        _ => (ls, le),
    };
    let inner = s[a..b].to_owned();
    let t = format!("```{lang}{nl}{inner}{nl}```");
    let caret = a + 3 + lang.len() + nl.len() + inner.len();
    Some(
        Transaction::single(a..b, &t)
            .with_selection_before(sel)
            .with_selection_after(Selection::caret(caret)),
    )
}

/// End of the top-level block containing `pos` (or `pos` itself on a blank line).
fn block_end(doc: &Doc, pos: usize) -> usize {
    let ast = doc.ast();
    match ast.index_at(pos) {
        Some(i) => ast.end(i),
        None => pos,
    }
}

/// Inserts `block` (Markdown source) as a new block after the current one; the
/// caret goes to `caret_in_block` within it.
pub fn insert_block_after(
    doc: &Doc,
    sel: Selection,
    block: &str,
    caret_in_block: usize,
) -> Transaction {
    let s = text(doc);
    let nl = doc.newline();
    let pos = sel.head;
    let line_blank = s[line_start(&s, pos)..line_end(&s, pos)].trim().is_empty();
    if line_blank {
        let ls = line_start(&s, pos);
        let le = line_end(&s, pos);
        let t = block.to_string();
        let caret = ls + caret_in_block;
        let pad = if le < s.len() {
            nl.to_owned()
        } else {
            String::new()
        };
        let _ = pad;
        return Transaction::single(ls..le, &t)
            .with_selection_before(sel)
            .with_selection_after(Selection::caret(caret));
    }
    let end = block_end(doc, pos).max(line_end(&s, pos));
    let t = format!("{nl}{nl}{block}");
    let caret = end + 2 * nl.len() + caret_in_block;
    Transaction::single(end..end, &t)
        .with_selection_before(sel)
        .with_selection_after(Selection::caret(caret))
}

/// A GFM table with `rows` body rows and `cols` columns (ED-053).
pub fn table_source(rows: usize, cols: usize, nl: &str) -> String {
    let cols = cols.clamp(1, 64);
    let header: Vec<String> = (1..=cols).map(|i| format!("Column {i}")).collect();
    let mut out = format!("| {} |{nl}", header.join(" | "));
    out.push_str(&format!("|{}{nl}", " --- |".repeat(cols)));
    for _ in 0..rows.max(1) {
        out.push_str(&format!(
            "|{}{nl}",
            "  |".repeat(cols).replace("  |", "   |")
        ));
    }
    out.trim_end_matches(['\n', '\r']).to_owned()
}

/// Inserts an empty paragraph before (`above`) or after the current block (ED-070).
pub fn insert_paragraph(doc: &Doc, sel: Selection, above: bool) -> Transaction {
    let s = text(doc);
    let nl = doc.newline();
    let ast = doc.ast();
    let pos = sel.head;
    match ast.index_at(pos) {
        Some(i) if above => {
            let start = line_start(&s, ast.start(i));
            let t = format!("{nl}{nl}");
            Transaction::single(start..start, &t)
                .with_selection_before(sel)
                .with_selection_after(Selection::caret(start))
        }
        Some(i) => {
            let end = ast.end(i);
            let t = format!("{nl}{nl}");
            Transaction::single(end..end, &t)
                .with_selection_before(sel)
                .with_selection_after(Selection::caret(end + 2 * nl.len()))
        }
        None => Transaction::default()
            .with_selection_before(sel)
            .with_selection_after(sel),
    }
}

/// Makes sure the document ends with an empty paragraph and puts the caret there
/// (ED-006 click below the last block).
pub fn ensure_trailing(doc: &Doc, sel: Selection) -> Transaction {
    let s = text(doc);
    let nl = doc.newline();
    let len = s.len();
    let trimmed = s.trim_end_matches(['\n', '\r', ' ', '\t']);
    if trimmed.is_empty() {
        return Transaction::default()
            .with_selection_before(sel)
            .with_selection_after(Selection::caret(len));
    }
    // Already a blank line at the end?
    let tail = &s[trimmed.len()..];
    let blank_lines = tail.matches('\n').count();
    if blank_lines >= 2 {
        return Transaction::default()
            .with_selection_before(sel)
            .with_selection_after(Selection::caret(len));
    }
    let add = if blank_lines == 1 {
        nl.to_owned()
    } else {
        format!("{nl}{nl}")
    };
    Transaction::single(len..len, &add)
        .with_selection_before(sel)
        .with_selection_after(Selection::caret(len + add.len()))
}

/// Ctrl+Enter (ED-006, ED-013): leaves a code or math block (creating a paragraph
/// after it if needed); at the start of the document's first block inserts a
/// paragraph above it.
pub fn exit_block(doc: &Doc, sel: Selection) -> Transaction {
    let s = text(doc);
    let nl = doc.newline();
    let ast = doc.ast();
    let pos = sel.head;
    let Some(i) = ast.index_at(pos) else {
        return Transaction::default()
            .with_selection_before(sel)
            .with_selection_after(sel);
    };
    // At the start of the first block (or of its first content line: a code or
    // math block) a paragraph goes above it.
    let first_line = ast
        .get(0)
        .and_then(|r| r.block.lines.first().map(|l| r.abs(l.span.start)));
    if i == 0
        && ((pos <= ast.start(0) + 1 && !context::in_code_content(doc, pos))
            || Some(pos) == first_line)
    {
        let t = format!("{nl}{nl}");
        return Transaction::single(0..0, &t)
            .with_selection_before(sel)
            .with_selection_after(Selection::caret(0));
    }
    let end = ast.end(i);
    // Is there a following block? Put the caret at its start.
    if i + 1 < ast.len() {
        let next = ast.start(i + 1);
        let gap = &s[end..next];
        if gap.matches('\n').count() >= 2 {
            // Blank line exists between: create an empty paragraph in it.
            let t = format!("{nl}{nl}");
            return Transaction::single(end..end, &t)
                .with_selection_before(sel)
                .with_selection_after(Selection::caret(end + 2 * nl.len()));
        }
        return Transaction::default()
            .with_selection_before(sel)
            .with_selection_after(Selection::caret(next));
    }
    let t = format!("{nl}{nl}");
    Transaction::single(end..end, &t)
        .with_selection_before(sel)
        .with_selection_after(Selection::caret(end + 2 * nl.len()))
}

/// `---` after the current block (ED-063).
pub fn insert_rule(doc: &Doc, sel: Selection) -> Transaction {
    let nl = doc.newline();
    let mut t = insert_block_after(doc, sel, &format!("---{nl}{nl}"), 3 + 2 * nl.len());
    if let Some(e) = t.edits.first_mut() {
        let _ = e;
    }
    t
}

/// Absolute ranges `(whole link, link text)` of the link at `pos`.
pub fn link_ranges(
    doc: &Doc,
    pos: usize,
) -> Option<(std::ops::Range<usize>, std::ops::Range<usize>)> {
    let p = context::path(doc, pos);
    let leaf = p.last()?;
    let rel = (pos as isize - leaf.delta) as usize;
    let mut found = None;
    for i in &leaf.block.inlines {
        i.walk(&mut |n| {
            if let InlineKind::Link { text, .. } = &n.kind
                && n.span.start <= rel
                && rel <= n.span.end
            {
                found = Some((leaf.abs_range(&n.span), leaf.abs_range(text)));
            }
        });
    }
    found
}

/// Replaces the link at the caret by its text (ED-130 "移除链接").
pub fn remove_link(doc: &Doc, sel: Selection) -> Option<Transaction> {
    let (whole, text) = link_ranges(doc, sel.head)?;
    let s = doc.text().slice(text.clone()).into_owned();
    let s = if text.is_empty() {
        doc.text()
            .slice(whole.clone())
            .trim_matches(['<', '>'])
            .to_owned()
    } else {
        s
    };
    let end = whole.start + s.len();
    Some(
        Transaction::single(whole, &s)
            .with_selection_before(sel)
            .with_selection_after(Selection::caret(end)),
    )
}

/// Turns the paragraph at the caret into a GitHub alert, or inserts an empty
/// one (MD-004). Inside a quote only the marker line is added.
pub fn insert_alert(doc: &Doc, sel: Selection, kind: &str) -> Transaction {
    let s = text(doc);
    let nl = doc.newline();
    let pos = sel.head;
    let ls = line_start(&s, pos);
    let le = line_end(&s, pos);
    let line = &s[ls..le];
    if line.trim().is_empty() {
        return insert_block_after(
            doc,
            sel,
            &format!("> [!{kind}]{nl}> "),
            kind.len() + 7 + nl.len(),
        );
    }
    // The paragraph's lines (lines up to the next blank one, both ways).
    let mut start = ls;
    while start > 0 {
        let pls = line_start(&s, start - 1);
        if s[pls..start].trim().is_empty() {
            break;
        }
        start = pls;
    }
    let mut end = le;
    while end < s.len() {
        let nle = line_end(&s, end + 1);
        if s[end..nle].trim().is_empty() {
            break;
        }
        end = nle;
    }
    let body = &s[start..end];
    let quoted = body.lines().all(|l| l.trim_start().starts_with('>'));
    let new_body: String = if quoted {
        body.to_owned()
    } else {
        body.split_inclusive('\n')
            .map(|l| format!("> {l}"))
            .collect()
    };
    let t = format!("> [!{kind}]{nl}{new_body}");
    let caret = start + t.len();
    Transaction::single(start..end, &t)
        .with_selection_before(sel)
        .with_selection_after(Selection::caret(caret))
}

/// Inserts `[^n]` at the caret (the first unused number) and `[^n]: ` at the end
/// of the document, putting the caret into the definition (MD-010).
pub fn insert_footnote(doc: &Doc, sel: Selection) -> Transaction {
    let s = text(doc);
    let nl = doc.newline();
    let mut n = 1;
    while s.contains(&format!("[^{n}]")) {
        n += 1;
    }
    let reference = format!("[^{n}]");
    let mut tail = String::new();
    if !s.ends_with('\n') && !s.is_empty() {
        tail.push_str(nl);
    }
    if !s.is_empty() {
        tail.push_str(nl);
    }
    let def = format!("[^{n}]: ");
    tail.push_str(&def);
    let r = sel.range();
    let end_pos = s.len() + reference.len() - (r.end - r.start) + tail.len();
    Transaction::many(vec![
        Edit {
            range: r.clone(),
            text: reference,
        },
        Edit {
            range: s.len()..s.len(),
            text: tail,
        },
    ])
    .with_selection_before(sel)
    .with_selection_after(Selection::caret(end_pos))
}

/// Removes inline markup in the selection (ED-041): emphasis, strong, strike,
/// highlight, code and link syntax; the text stays.
pub fn clear_format(doc: &Doc, sel: Selection) -> Option<Transaction> {
    if sel.is_empty() {
        return None;
    }
    let r = sel.range();
    let mut edits: Vec<Edit> = Vec::new();
    let ast = doc.ast();
    let first = ast.first_starting_at_or_after(r.start).saturating_sub(1);
    for bi in first..ast.len() {
        let Some(top) = ast.get(bi) else { break };
        if top.span().start > r.end {
            break;
        }
        let delta = top.delta;
        let abs = |x: &std::ops::Range<usize>| {
            (x.start as isize + delta) as usize..(x.end as isize + delta) as usize
        };
        top.block.walk(&mut |b| {
            for i in &b.inlines {
                i.walk(&mut |n| {
                    let span = abs(&n.span);
                    if span.end <= r.start || span.start >= r.end {
                        return;
                    }
                    match &n.kind {
                        InlineKind::Emph { open, close }
                        | InlineKind::Strong { open, close }
                        | InlineKind::Strike { open, close }
                        | InlineKind::Highlight { open, close }
                        | InlineKind::Superscript { open, close }
                        | InlineKind::Subscript { open, close } => {
                            edits.push(Edit {
                                range: abs(open),
                                text: String::new(),
                            });
                            edits.push(Edit {
                                range: abs(close),
                                text: String::new(),
                            });
                        }
                        InlineKind::Code { content, .. } => {
                            let c = abs(content);
                            edits.push(Edit {
                                range: span.start..c.start,
                                text: String::new(),
                            });
                            edits.push(Edit {
                                range: c.end..span.end,
                                text: String::new(),
                            });
                        }
                        InlineKind::Link { text, .. } => {
                            let t = abs(text);
                            if t.start > span.start {
                                edits.push(Edit {
                                    range: span.start..t.start,
                                    text: String::new(),
                                });
                                edits.push(Edit {
                                    range: t.end..span.end,
                                    text: String::new(),
                                });
                            }
                        }
                        _ => {}
                    }
                });
            }
        });
    }
    if edits.is_empty() {
        return None;
    }
    edits.sort_by_key(|e| e.range.start);
    edits.dedup_by(|a, b| a.range == b.range);
    let removed_before = |p: usize| -> usize {
        edits
            .iter()
            .filter(|e| e.range.end <= p)
            .map(|e| e.range.len())
            .sum()
    };
    let a = r.start - removed_before(r.start);
    let b = r.end - removed_before(r.end);
    Some(
        Transaction::many(edits.clone())
            .with_selection_before(sel)
            .with_selection_after(Selection::new(a, b.max(a))),
    )
}

/// Selects the innermost inline element around the caret (or the selection),
/// then its parent, then the block (ED-021 "选择样式范围").
pub fn style_scope(doc: &Doc, sel: Selection) -> Option<Selection> {
    let r = sel.range();
    let path = context::path(doc, r.start);
    let leaf = path.last()?;
    let delta = leaf.delta;
    let abs = |x: &std::ops::Range<usize>| {
        (x.start as isize + delta) as usize..(x.end as isize + delta) as usize
    };
    let mut best: Option<std::ops::Range<usize>> = None;
    for i in &leaf.block.inlines {
        i.walk(&mut |n| {
            if matches!(
                n.kind,
                InlineKind::Text | InlineKind::SoftBreak | InlineKind::HardBreak
            ) {
                return;
            }
            let s = abs(&n.span);
            let contains =
                s.start <= r.start && r.end <= s.end && (s.start < r.start || r.end < s.end);
            if contains && best.as_ref().is_none_or(|b| s.len() < b.len()) {
                best = Some(s);
            }
        });
    }
    let target = best.or_else(|| {
        let lines = &leaf.block.lines;
        let (a, b) = (lines.first()?.span.start, lines.last()?.span.end);
        let s = (a as isize + delta) as usize..(b as isize + delta) as usize;
        (s.start < r.start || r.end < s.end).then_some(s)
    })?;
    Some(Selection::new(target.start, target.end))
}

/// Swaps the top-level block (or list item) at the caret with its neighbour
/// (ED-018). The caret moves with the block.
pub fn move_block(doc: &Doc, sel: Selection, up: bool) -> Option<Transaction> {
    let s = text(doc);
    let pos = sel.head;
    // A list item swaps with a sibling item.
    if let Some(info) = context::list_item(doc, pos) {
        let k = info.index;
        let other = if up { k.checked_sub(1)? } else { k + 1 };
        let item_range = |i: usize| -> Option<std::ops::Range<usize>> {
            let start = line_start(&s, info.siblings.get(i)?.start);
            let end = match info.siblings.get(i + 1) {
                Some(n) => line_start(&s, n.start),
                None => {
                    let e = info.list_span.end;
                    if e < s.len() {
                        line_end(&s, e) + 1
                    } else {
                        s.len()
                    }
                }
            };
            Some(start..end.min(s.len()))
        };
        let (a, b) = if up {
            (item_range(other)?, item_range(k)?)
        } else {
            (item_range(k)?, item_range(other)?)
        };
        return Some(swap(&s, a, b, sel, up, doc.newline()));
    }
    let ast = doc.ast();
    let i = ast.index_at(pos)?;
    let j = if up { i.checked_sub(1)? } else { i + 1 };
    if j >= ast.len() {
        return None;
    }
    let range = |k: usize| {
        line_start(&s, ast.start(k))..line_end(&s, ast.end(k).saturating_sub(1).max(ast.start(k)))
    };
    let (a, b) = if up {
        (range(j), range(i))
    } else {
        (range(i), range(j))
    };
    Some(swap(&s, a, b, sel, up, doc.newline()))
}

/// Swaps adjacent ranges `a` < `b` (the text between them stays in place).
fn swap(
    s: &str,
    a: std::ops::Range<usize>,
    b: std::ops::Range<usize>,
    sel: Selection,
    up: bool,
    nl: &str,
) -> Transaction {
    let ta = &s[a.clone()];
    let tb = &s[b.clone()];
    let gap = &s[a.end..b.start];
    // Ranges that end without a newline (last item) get one when moved up.
    let (ta2, tb2) = if !tb.ends_with('\n') && ta.ends_with('\n') {
        (
            ta.trim_end_matches(['\n', '\r']).to_owned(),
            format!("{tb}{nl}"),
        )
    } else {
        (ta.to_owned(), tb.to_owned())
    };
    let new = format!("{tb2}{gap}{ta2}");
    // The moved block keeps the caret at the same offset inside it.
    let caret = if up {
        a.start + (sel.head - b.start)
    } else {
        a.start + tb2.len() + gap.len() + (sel.head - a.start)
    };
    Transaction::single(a.start..b.end, &new)
        .with_selection_before(sel)
        .with_selection_after(Selection::caret(caret.min(a.start + new.len())))
}

/// Changes the marker of every item of the list at the caret (ED-062):
/// `-`, `*`, `+` for bullets, `.` or `)` for ordered lists (renumbered from the
/// list's start).
pub fn set_list_marker(doc: &Doc, sel: Selection, marker: &str) -> Option<Transaction> {
    let s = text(doc);
    let info = context::list_item_near(doc, sel.head)?;
    let ordered = matches!(marker, "1." | "1)" | "." | ")");
    let delim = if marker.ends_with(')') { ')' } else { '.' };
    let start = match info.kind {
        ListKind::Ordered(_) => s[info.siblings[0].clone()]
            .trim_end_matches(['.', ')'])
            .parse::<u64>()
            .unwrap_or(1),
        ListKind::Bullet(_) => 1,
    };
    let mut edits = Vec::new();
    for (k, m) in info.siblings.iter().enumerate() {
        let new = if ordered {
            format!("{}{delim}", start + k as u64)
        } else {
            marker.chars().next().unwrap_or('-').to_string()
        };
        if s[m.clone()] != new {
            edits.push(Edit {
                range: m.clone(),
                text: new,
            });
        }
    }
    if edits.is_empty() {
        return None;
    }
    let tx = Transaction::many(edits).with_selection_before(sel);
    let after = tx.map_selection(sel);
    Some(tx.with_selection_after(after))
}
