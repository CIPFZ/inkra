//! GFM table editing (ED-080 … ED-085). Tables stay plain Markdown: every
//! structural command rewrites the table source with aligned pipes (ED-085).

use std::ops::Range;

use inkra_doc::{Doc, Selection, Transaction};
use inkra_markdown::ast::{Align, BlockKind};
use inkra_text::segment::display_width;

use super::context;

/// A table in the document (absolute ranges).
#[derive(Debug, Clone, PartialEq)]
pub struct Table {
    /// Whole table (first character of the header row to the end of the last row).
    pub span: Range<usize>,
    /// Container prefix of each line (`> `, list indentation).
    pub prefix: String,
    /// Column alignments.
    pub aligns: Vec<Align>,
    /// Rows (header first); cells are trimmed content ranges.
    pub rows: Vec<Vec<Range<usize>>>,
    /// Row line ranges (content, header first).
    pub row_spans: Vec<Range<usize>>,
    /// The delimiter row.
    pub delimiter: Range<usize>,
    /// Absolute positions of the unescaped `|` of every row.
    pub pipes: Vec<Vec<usize>>,
}

/// A caret position inside a table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    /// Row (0 = header).
    pub row: usize,
    /// Column.
    pub col: usize,
}

impl Table {
    /// Number of columns.
    pub fn cols(&self) -> usize {
        self.aligns.len().max(1)
    }

    /// Cell texts (source, escapes kept), padded to the column count.
    pub fn texts(&self, doc: &Doc) -> Vec<Vec<String>> {
        let t = doc.text();
        self.rows
            .iter()
            .map(|r| {
                let mut v: Vec<String> =
                    r.iter().map(|c| t.slice(c.clone()).into_owned()).collect();
                v.resize(self.cols(), String::new());
                v.truncate(self.cols());
                v
            })
            .collect()
    }

    /// The cell containing `pos`: the column is the number of separating pipes
    /// before `pos` in its row (a leading pipe does not count).
    pub fn cell_at(&self, pos: usize) -> Option<Cell> {
        for (r, span) in self.row_spans.iter().enumerate() {
            if pos < span.start.saturating_sub(self.prefix.len()) || pos > span.end {
                continue;
            }
            let pipes = &self.pipes[r];
            let leading = pipes
                .first()
                .is_some_and(|&p| self.rows[r].first().is_none_or(|c| p < c.start));
            let before = pipes.iter().filter(|&&p| p < pos).count();
            let col = before.saturating_sub(usize::from(leading));
            return Some(Cell {
                row: r,
                col: col.min(self.cols() - 1),
            });
        }
        None
    }
}

/// The table containing `pos`, if any.
pub fn table_at(doc: &Doc, pos: usize) -> Option<Table> {
    let path = context::path(doc, pos);
    let n = path
        .iter()
        .rev()
        .find(|n| matches!(n.block.kind, BlockKind::Table { .. }))?;
    let BlockKind::Table { aligns, delimiter } = &n.block.kind else {
        return None;
    };
    let t = doc.text();
    let mut rows = Vec::new();
    let mut row_spans = Vec::new();
    let mut pipes = Vec::new();
    for row in &n.block.children {
        let rs = n.abs_range(&row.span);
        let line = t.slice(rs.clone());
        let b = line.as_bytes();
        let mut ps = Vec::new();
        let mut i = 0;
        while i < b.len() {
            match b[i] {
                b'\\' => i += 2,
                b'|' => {
                    ps.push(rs.start + i);
                    i += 1;
                }
                _ => i += 1,
            }
        }
        pipes.push(ps);
        row_spans.push(rs);
        rows.push(row.children.iter().map(|c| n.abs_range(&c.span)).collect());
    }
    let span = n.span();
    let ls = t.line_to_byte(t.byte_to_line(span.start));
    let prefix = t.slice(ls..span.start).into_owned();
    // Only keep container syntax (`>` and spaces) as the prefix.
    let prefix = if prefix.chars().all(|c| c == '>' || c == ' ' || c == '\t') {
        prefix
    } else {
        String::new()
    };
    Some(Table {
        span: ls + prefix.len()..span.end,
        prefix,
        aligns: aligns.clone(),
        rows,
        row_spans,
        delimiter: n.abs_range(delimiter),
        pipes,
    })
}

/// Whether `pos` is inside a table.
pub fn in_table(doc: &Doc, pos: usize) -> bool {
    table_at(doc, pos).is_some()
}

/// Rendered table source plus the content start offset of every cell.
pub struct Rendered {
    /// Source (without the leading prefix of the first line).
    pub text: String,
    /// `starts[row][col]`: offset of the cell content in `text`.
    pub starts: Vec<Vec<usize>>,
    /// `ends[row][col]`: offset after the cell's padding (before ` |`).
    pub ends: Vec<Vec<usize>>,
}

fn dashes(align: Align, w: usize) -> String {
    let w = w.max(3);
    match align {
        Align::None => "-".repeat(w),
        Align::Left => format!(":{}", "-".repeat(w - 1)),
        Align::Right => format!("{}:", "-".repeat(w - 1)),
        Align::Center => format!(":{}:", "-".repeat(w.saturating_sub(2).max(1))),
    }
}

/// Renders `cells` as a GFM table. With `pad`, columns are padded to equal display
/// width (CJK counts 2) and the delimiter row is filled with `-` (ED-085).
pub fn render(
    cells: &[Vec<String>],
    aligns: &[Align],
    pad: bool,
    nl: &str,
    prefix: &str,
) -> Rendered {
    let cols = aligns.len().max(1);
    let mut widths = vec![3usize; cols];
    if pad {
        for row in cells {
            for (c, s) in row.iter().enumerate().take(cols) {
                widths[c] = widths[c].max(display_width(s));
            }
        }
    }
    let mut text = String::new();
    let mut starts = Vec::new();
    let mut ends = Vec::new();
    let line = |row: &[String], text: &mut String, ends: &mut Vec<Vec<usize>>| -> Vec<usize> {
        let mut st = Vec::new();
        let mut en = Vec::new();
        text.push('|');
        for (c, &cw) in widths.iter().enumerate() {
            let s = row.get(c).map(String::as_str).unwrap_or("");
            text.push(' ');
            st.push(text.len());
            let w = display_width(s);
            let fill = if pad { cw.saturating_sub(w) } else { 0 };
            match aligns.get(c).copied().unwrap_or_default() {
                Align::Right if pad => {
                    // Keep the cell start at the content for caret placement.
                    let at = text.len();
                    text.push_str(&" ".repeat(fill));
                    text.push_str(s);
                    *st.last_mut().unwrap_or(&mut 0) = at + fill;
                }
                Align::Center if pad => {
                    let left = fill / 2;
                    let at = text.len();
                    text.push_str(&" ".repeat(left));
                    text.push_str(s);
                    text.push_str(&" ".repeat(fill - left));
                    *st.last_mut().unwrap_or(&mut 0) = at + left;
                }
                _ => {
                    text.push_str(s);
                    text.push_str(&" ".repeat(fill));
                }
            }
            en.push(text.len());
            text.push_str(" |");
        }
        ends.push(en);
        st
    };
    for (r, row) in cells.iter().enumerate() {
        if r > 0 {
            text.push_str(nl);
            text.push_str(prefix);
        }
        starts.push(line(row, &mut text, &mut ends));
        if r == 0 {
            text.push_str(nl);
            text.push_str(prefix);
            text.push('|');
            for (c, &cw) in widths.iter().enumerate() {
                text.push(' ');
                text.push_str(&dashes(
                    aligns.get(c).copied().unwrap_or_default(),
                    if pad { cw } else { 3 },
                ));
                text.push_str(" |");
            }
        }
    }
    Rendered { text, starts, ends }
}

/// Replaces `table` with `cells`/`aligns` and puts the caret in `target`.
pub fn rewrite(
    doc: &Doc,
    sel: Selection,
    table: &Table,
    cells: &[Vec<String>],
    aligns: &[Align],
    target: Cell,
    pad: bool,
) -> Transaction {
    let r = render(cells, aligns, pad, doc.newline(), &table.prefix);
    let row = target.row.min(r.starts.len().saturating_sub(1));
    let col = target.col.min(aligns.len().saturating_sub(1));
    let cell_len = cells
        .get(row)
        .and_then(|v| v.get(col))
        .map_or(0, String::len);
    // Caret after the cell text; an empty cell's caret sits after its padding,
    // where the layout shows it (before ` |`).
    let pick = |v: &Vec<Vec<usize>>| v.get(row).and_then(|v| v.get(col)).copied();
    let caret_rel = if cell_len == 0 {
        pick(&r.ends).unwrap_or(0)
    } else {
        pick(&r.starts).unwrap_or(0) + cell_len
    };
    let caret = table.span.start + caret_rel;
    Transaction::single(table.span.clone(), &r.text)
        .with_selection_before(sel)
        .with_selection_after(Selection::caret(caret))
}

/// Escapes `|` typed into a cell (ED-081).
pub fn escape_cell_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_backslash = false;
    for c in s.chars() {
        if c == '|' && !prev_backslash {
            out.push('\\');
        }
        prev_backslash = c == '\\' && !prev_backslash;
        out.push(c);
    }
    out
}

fn cell_number(s: &str) -> Option<f64> {
    let t = s
        .trim()
        .trim_matches(|c: char| c == '*' || c == '`' || c == '_');
    let t = t.replace([',', '_'], "");
    let t = t.trim_end_matches('%');
    let t = t.trim_start_matches(['$', '¥', '€', '£']);
    t.parse().ok()
}

/// "Smart" comparison for sorting: numbers numerically, others case-insensitively
/// with natural ordering of embedded numbers.
pub fn smart_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    match (cell_number(a), cell_number(b)) {
        (Some(x), Some(y)) => x.total_cmp(&y),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => natural_cmp(&a.to_lowercase(), &b.to_lowercase()),
    }
}

/// Natural string order (`file2` < `file10`).
pub fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    inkra_text::sort::natural_cmp(a, b)
}

/// CSV of the table (RFC 4180 quoting).
pub fn to_csv(cells: &[Vec<String>], nl: &str) -> String {
    let q = |s: &str| {
        let s = s.replace("\\|", "|");
        if s.contains([',', '"', '\n', '\r']) {
            format!("\"{}\"", s.replace('"', "\"\""))
        } else {
            s
        }
    };
    cells
        .iter()
        .map(|r| r.iter().map(|c| q(c)).collect::<Vec<_>>().join(","))
        .collect::<Vec<_>>()
        .join(nl)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_aligned_with_cjk_width() {
        let cells = vec![
            vec!["名称".to_owned(), "n".to_owned()],
            vec!["a".to_owned(), "1234".to_owned()],
        ];
        let r = render(&cells, &[Align::Left, Align::Right], true, "\n", "");
        assert_eq!(r.text, "| 名称 |    n |\n| :--- | ---: |\n| a    | 1234 |");
        let r = render(&cells, &[Align::None, Align::Center], false, "\n", "> ");
        assert_eq!(r.text, "| 名称 | n |\n> | --- | :-: |\n> | a | 1234 |");
    }

    #[test]
    fn helpers() {
        assert_eq!(escape_cell_text("a|b\\|c"), "a\\|b\\|c");
        assert_eq!(smart_cmp("10", "9"), std::cmp::Ordering::Greater);
        assert_eq!(smart_cmp("file2", "file10"), std::cmp::Ordering::Less);
        assert_eq!(smart_cmp("$1,200", "300"), std::cmp::Ordering::Greater);
        assert_eq!(
            to_csv(&[vec!["a,b".into(), "x\\|y".into()]], "\n"),
            "\"a,b\",x|y"
        );
    }

    #[test]
    fn table_model() {
        let doc = Doc::from_str(
            "intro\n\n| a | b |\n|---|:-:|\n| 1 | 2 |\n",
            Default::default(),
        );
        let t = table_at(&doc, 10).unwrap();
        assert_eq!(t.cols(), 2);
        assert_eq!(t.texts(&doc), vec![vec!["a", "b"], vec!["1", "2"]]);
        assert_eq!(t.cell_at(t.rows[1][1].start), Some(Cell { row: 1, col: 1 }));
        assert!(!in_table(&doc, 2));
    }
}

/// What a key does inside a table.
#[derive(Debug, Clone, PartialEq)]
pub enum TableAction {
    /// Apply this transaction.
    Edit(Transaction),
    /// Move the selection.
    Select(Selection),
    /// Do nothing (the key is swallowed).
    Nothing,
}

/// Tab / Shift+Tab (ED-014): next/previous cell; Tab in the last cell adds a row.
pub fn tab(doc: &Doc, sel: Selection, back: bool, pad: bool) -> Option<TableAction> {
    let t = table_at(doc, sel.head)?;
    let cell = t.cell_at(sel.head)?;
    let cols = t.cols();
    let idx = cell.row * cols + cell.col;
    if !back && idx + 1 >= t.rows.len() * cols {
        let mut cells = t.texts(doc);
        cells.push(vec![String::new(); cols]);
        let target = Cell {
            row: cells.len() - 1,
            col: 0,
        };
        return Some(TableAction::Edit(rewrite(
            doc, sel, &t, &cells, &t.aligns, target, pad,
        )));
    }
    if back && idx == 0 {
        return Some(TableAction::Nothing);
    }
    let next = if back { idx - 1 } else { idx + 1 };
    let target = Cell {
        row: next / cols,
        col: next % cols,
    };
    Some(TableAction::Select(cell_selection(&t, target)))
}

fn cell_selection(t: &Table, c: Cell) -> Selection {
    match t.rows.get(c.row).and_then(|r| r.get(c.col)) {
        Some(r) => Selection::new(r.start, r.end),
        None => Selection::caret(t.row_spans.get(c.row).map_or(t.span.end, |r| r.end)),
    }
}

/// Enter in a cell (ED-013): never a line break; on the last row it adds a row,
/// elsewhere it moves to the same column of the next row. Shift+Enter inserts
/// `<br>`.
pub fn enter(doc: &Doc, sel: Selection, soft: bool, pad: bool) -> Option<TableAction> {
    let t = table_at(doc, sel.head)?;
    let cell = t.cell_at(sel.head)?;
    if soft {
        let r = sel.range();
        let end = r.start + 4;
        return Some(TableAction::Edit(
            Transaction::single(r, "<br>")
                .with_selection_before(sel)
                .with_selection_after(Selection::caret(end)),
        ));
    }
    if cell.row + 1 >= t.rows.len() {
        let mut cells = t.texts(doc);
        cells.push(vec![String::new(); t.cols()]);
        let target = Cell {
            row: cells.len() - 1,
            col: cell.col,
        };
        return Some(TableAction::Edit(rewrite(
            doc, sel, &t, &cells, &t.aligns, target, pad,
        )));
    }
    let target = Cell {
        row: cell.row + 1,
        col: cell.col,
    };
    let s = cell_selection(&t, target);
    Some(TableAction::Select(Selection::caret(s.end())))
}

/// Backspace/Delete inside a table: never removes pipes. A selection spanning
/// several cells clears their contents (ED-012).
pub fn delete(doc: &Doc, sel: Selection, forward: bool) -> Option<TableAction> {
    let t = table_at(doc, sel.head)?;
    if !sel.is_empty() {
        let r = sel.range();
        let touched: Vec<Range<usize>> = t
            .rows
            .iter()
            .flatten()
            .filter(|c| c.end >= r.start && c.start <= r.end)
            .cloned()
            .collect();
        if touched.len() <= 1 {
            // Within one cell: normal deletion, clamped to the cell.
            let c = touched.first()?.clone();
            let a = r.start.max(c.start);
            let b = r.end.min(c.end);
            return Some(TableAction::Edit(
                Transaction::single(a..b, "")
                    .with_selection_before(sel)
                    .with_selection_after(Selection::caret(a)),
            ));
        }
        let edits: Vec<inkra_doc::Edit> = touched
            .iter()
            .map(|c| inkra_doc::Edit {
                range: c.start.max(r.start).min(c.end)..c.end.min(r.end).max(c.start),
                text: String::new(),
            })
            .filter(|e| !e.range.is_empty())
            .collect();
        let caret = touched[0].start.max(r.start);
        return Some(TableAction::Edit(
            Transaction::many(edits)
                .with_selection_before(sel)
                .with_selection_after(Selection::caret(caret)),
        ));
    }
    let pos = sel.head;
    let cell = t.cell_at(pos)?;
    let range = t.rows.get(cell.row).and_then(|r| r.get(cell.col))?.clone();
    if (!forward && pos <= range.start) || (forward && pos >= range.end) {
        return Some(TableAction::Nothing);
    }
    None
}

/// The display column the caret is in, for vertical movement.
pub fn caret_cell(doc: &Doc, pos: usize) -> Option<(Table, Cell)> {
    let t = table_at(doc, pos)?;
    let c = t.cell_at(pos)?;
    Some((t, c))
}
