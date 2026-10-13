//! "Normalize on save" (MD-092) and `inkra-cli fmt`: padded tables, renumbered
//! ordered lists, no trailing whitespace (hard breaks keep two spaces, code is
//! untouched) and exactly one final newline.

use inkra_doc::Doc;
use inkra_markdown::ast::{Block, BlockKind, ListKind};

use super::table;

/// What to normalize.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Normalize {
    /// Pad table columns (ED-085).
    pub tables: bool,
    /// Renumber ordered lists from their first number (MD-076).
    pub lists: bool,
    /// Remove trailing whitespace.
    pub trailing: bool,
    /// End with exactly one newline.
    pub final_newline: bool,
}

impl Normalize {
    /// Everything (MD-092).
    pub const ALL: Self = Self {
        tables: true,
        lists: true,
        trailing: true,
        final_newline: true,
    };
    /// Only trailing whitespace (PR-013 without normalization).
    pub const TRAILING: Self = Self {
        tables: false,
        lists: false,
        trailing: true,
        final_newline: false,
    };
}

/// Byte ranges of code and math block contents (left alone).
fn verbatim_ranges(doc: &Doc) -> Vec<std::ops::Range<usize>> {
    let ast = doc.ast();
    let mut out = Vec::new();
    for i in 0..ast.len() {
        let Some(r) = ast.get(i) else { continue };
        r.block.walk(&mut |b: &Block| {
            if matches!(
                b.kind,
                BlockKind::CodeBlock { .. }
                    | BlockKind::MathBlock { .. }
                    | BlockKind::HtmlBlock { .. }
                    | BlockKind::FrontMatter { .. }
            ) {
                out.push(r.abs_span(&b.span));
            }
        });
    }
    out
}

/// The normalized text of `doc`.
pub fn normalize(doc: &Doc, what: Normalize) -> String {
    let src = doc.text().to_string();
    let nl = doc.newline();
    let mut edits: Vec<(std::ops::Range<usize>, String)> = Vec::new();
    let ast = doc.ast();
    if what.tables || what.lists {
        for i in 0..ast.len() {
            let Some(r) = ast.get(i) else { continue };
            let mut tables = Vec::new();
            r.block.walk(&mut |b: &Block| match &b.kind {
                BlockKind::Table { .. } if what.tables => tables.push(r.abs_span(&b.span).start),
                BlockKind::List {
                    kind: ListKind::Ordered(_),
                    start,
                    ..
                } if what.lists => {
                    let mut n = *start;
                    for item in &b.children {
                        if let BlockKind::ListItem { marker, .. } = &item.kind {
                            let m = r.abs_span(marker);
                            let text = &src[m.clone()];
                            let delim = text.chars().last().unwrap_or('.');
                            let want = format!("{n}{delim}");
                            if text != want {
                                edits.push((m, want));
                            }
                            n += 1;
                        }
                    }
                }
                _ => {}
            });
            for at in tables {
                if let Some(t) = table::table_at(doc, at) {
                    let cells = t.texts(doc);
                    let rendered = table::render(&cells, &t.aligns, true, nl, &t.prefix);
                    if src.get(t.span.clone()) != Some(rendered.text.as_str()) {
                        edits.push((t.span.clone(), rendered.text));
                    }
                }
            }
        }
    }
    let mut out = apply(&src, edits);
    if what.trailing {
        out = trim_trailing(&Doc::from_str(&out, doc.options().clone()));
    }
    if what.final_newline {
        let trimmed = out.trim_end_matches(['\n', '\r']);
        out = if trimmed.is_empty() {
            String::new()
        } else {
            format!("{trimmed}{nl}")
        };
    }
    out
}

/// A transaction turning the document into `new` (one edit spanning the
/// changed middle), or `None` when nothing changes.
pub fn replace_with(
    doc: &Doc,
    sel: inkra_doc::Selection,
    new: &str,
) -> Option<inkra_doc::Transaction> {
    let old = doc.text().to_string();
    if old == new {
        return None;
    }
    let (a, b) = (old.as_bytes(), new.as_bytes());
    let mut pre = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    while !old.is_char_boundary(pre) || !new.is_char_boundary(pre) {
        pre -= 1;
    }
    let max_suf = (a.len() - pre).min(b.len() - pre);
    let mut suf = a
        .iter()
        .rev()
        .zip(b.iter().rev())
        .take(max_suf)
        .take_while(|(x, y)| x == y)
        .count();
    while !old.is_char_boundary(a.len() - suf) || !new.is_char_boundary(b.len() - suf) {
        suf -= 1;
    }
    let tx = inkra_doc::Transaction::single(pre..a.len() - suf, &new[pre..b.len() - suf])
        .with_selection_before(sel);
    let after = tx.map_selection(sel);
    Some(tx.with_selection_after(after))
}

fn apply(src: &str, mut edits: Vec<(std::ops::Range<usize>, String)>) -> String {
    edits.sort_by_key(|e| std::cmp::Reverse(e.0.start));
    let mut out = src.to_owned();
    let mut last = usize::MAX;
    for (r, t) in edits {
        // Overlapping edits (a list inside a table cannot happen) are skipped.
        if r.end > last {
            continue;
        }
        out.replace_range(r.clone(), &t);
        last = r.start;
    }
    out
}

/// Removes trailing spaces and tabs outside code; a hard break (two or more
/// spaces before more text in the same paragraph) keeps exactly two spaces.
pub fn trim_trailing(doc: &Doc) -> String {
    let src = doc.text().to_string();
    let skip = verbatim_ranges(doc);
    let mut out = String::with_capacity(src.len());
    let lines: Vec<&str> = src.split_inclusive('\n').collect();
    let mut pos = 0;
    for (k, line) in lines.iter().enumerate() {
        let start = pos;
        pos += line.len();
        let body_end = line.trim_end_matches(['\n', '\r']).len();
        let (body, ending) = line.split_at(body_end);
        if skip
            .iter()
            .any(|r| r.start < start + body_end.max(1) && start < r.end)
        {
            out.push_str(line);
            continue;
        }
        let trimmed = body.trim_end_matches([' ', '\t']);
        let spaces = body.len() - trimmed.len();
        let next_has_text = lines.get(k + 1).is_some_and(|n| !n.trim().is_empty());
        out.push_str(trimmed);
        if spaces >= 2
            && !trimmed.trim().is_empty()
            && next_has_text
            && body[trimmed.len()..].chars().all(|c| c == ' ')
        {
            out.push_str("  ");
        }
        out.push_str(ending);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_tables_lists_and_whitespace() {
        let src = "# T  \n\n|a|bb|\n|-|:-:|\n|ccc|d|\n\n3. x\n7. y\n1. z\n\nhard  \nbreak \n\n```\ncode   \n```\n\n\n";
        let doc = Doc::from_str(src, Default::default());
        let out = normalize(&doc, Normalize::ALL);
        assert_eq!(
            out,
            "# T\n\n| a   | bb  |\n| --- | :-: |\n| ccc |  d  |\n\n3. x\n4. y\n5. z\n\nhard  \nbreak\n\n```\ncode   \n```\n"
        );
        let doc = Doc::from_str("a \t\nb\n", Default::default());
        assert_eq!(normalize(&doc, Normalize::TRAILING), "a\nb\n");
        // Already normal text is unchanged.
        let doc = Doc::from_str(&out, Default::default());
        assert_eq!(normalize(&doc, Normalize::ALL), out);
    }
}
