//! Table commands (ED-083) and in-table editing helpers (ED-081, ED-013, ED-014).

use serde_json::Value;

use crate::command::{
    Category, CommandRegistry, CommandResult, CommandSpec, FnCommand, Outcome, UndoGroup, spec,
};
use crate::editor::table::{self, Cell, Table};
use crate::state::{CommandContext, Effect};
use inkra_doc::{Selection, UndoPolicy};
use inkra_markdown::ast::Align;

fn edits(mut s: CommandSpec) -> CommandSpec {
    s.undo_group = UndoGroup::Own;
    s
}

fn current(cx: &CommandContext<'_>) -> Option<(Table, Cell)> {
    let e = &cx.app.editor;
    let t = table::table_at(&e.doc, e.sel.head)?;
    let c = t.cell_at(e.sel.head).unwrap_or(Cell { row: 0, col: 0 });
    Some((t, c))
}

fn pad(cx: &CommandContext<'_>) -> bool {
    cx.app.prefs.bool("markdown.tableAlign")
}

/// Applies a rewrite of the current table.
fn rewrite(
    cx: &mut CommandContext<'_>,
    f: impl FnOnce(&mut Vec<Vec<String>>, &mut Vec<Align>, &mut Cell) -> bool,
) -> CommandResult {
    let Some((t, mut cell)) = current(cx) else {
        return Ok(Outcome::Noop);
    };
    let mut cells = t.texts(&cx.app.editor.doc);
    let mut aligns = t.aligns.clone();
    if !f(&mut cells, &mut aligns, &mut cell) {
        return Ok(Outcome::Noop);
    }
    let p = pad(cx);
    let e = &mut cx.app.editor;
    let tx = table::rewrite(&e.doc, e.sel, &t, &cells, &aligns, cell, p);
    e.doc.break_undo_group();
    e.apply(tx, UndoPolicy::NewGroup);
    cx.effects.push(Effect::ScrollToCaret);
    Ok(Outcome::Done)
}

fn in_table(cx: &CommandContext<'_>) -> bool {
    table::in_table(&cx.app.editor.doc, cx.app.editor.sel.head)
}

/// Moves to the next (or previous) cell; Tab in the last cell adds a row (ED-014).
pub fn step_cell(cx: &mut CommandContext<'_>, forward: bool) -> CommandResult {
    let Some((t, cell)) = current(cx) else {
        return Ok(Outcome::Noop);
    };
    let cols = t.cols();
    let rows = t.rows.len();
    let idx = cell.row * cols + cell.col;
    let next = if forward {
        idx + 1
    } else {
        idx.wrapping_sub(1)
    };
    if forward && next >= rows * cols {
        return rewrite(cx, |cells, aligns, c| {
            cells.push(vec![String::new(); aligns.len()]);
            *c = Cell {
                row: cells.len() - 1,
                col: 0,
            };
            true
        });
    }
    if next == usize::MAX {
        return Ok(Outcome::Noop);
    }
    let target = Cell {
        row: next / cols,
        col: next % cols,
    };
    select_cell(cx, &t, target);
    Ok(Outcome::Done)
}

/// Selects the content of `target` (Tab selects the next cell's text, like
/// spreadsheets; an empty cell just gets the caret).
fn select_cell(cx: &mut CommandContext<'_>, t: &Table, target: Cell) {
    let e = &mut cx.app.editor;
    match t.rows.get(target.row).and_then(|r| r.get(target.col)) {
        Some(r) => e.sel = Selection::new(r.start, r.end),
        None => {
            // A missing trailing cell: caret at the end of the row.
            let end = t.row_spans[target.row].end;
            e.sel = Selection::caret(end);
        }
    }
    cx.effects.push(Effect::ScrollToCaret);
}

fn idx(p: &Value, key: &str) -> usize {
    p.get(key).and_then(Value::as_u64).unwrap_or(0) as usize
}

type Run = fn(&mut CommandContext<'_>, &Value) -> CommandResult;

/// Registers table commands.
pub fn register(r: &mut CommandRegistry) {
    use Category::Table as T;
    let cmds: &[(&str, &str, Run)] = &[
        ("table.insertRowAbove", "Insert a row above.", |cx, _| {
            rewrite(cx, |cells, a, c| {
                let row = c.row.max(1);
                cells.insert(row, vec![String::new(); a.len()]);
                *c = Cell { row, col: c.col };
                true
            })
        }),
        (
            "table.insertRowBelow",
            "Insert a row below (Ctrl+Enter).",
            |cx, _| {
                // ED-006: Ctrl+Enter at the very start of a table that opens the
                // document makes room for a paragraph above it.
                let e = &cx.app.editor;
                if let Some(t) = crate::editor::table::table_at(&e.doc, e.sel.head)
                    && t.span.start == e.doc.ast().start(0)
                    && t.rows
                        .first()
                        .and_then(|r| r.first())
                        .is_some_and(|c| e.sel.head <= c.start)
                {
                    let tx = crate::editor::format_ops::exit_block(
                        &e.doc,
                        inkra_doc::Selection::caret(t.span.start),
                    );
                    let tx = tx.with_selection_before(e.sel);
                    return crate::formatting::apply_tx(cx, tx);
                }
                rewrite(cx, |cells, a, c| {
                    cells.insert(c.row + 1, vec![String::new(); a.len()]);
                    // A new row starts at its first cell.
                    *c = Cell {
                        row: c.row + 1,
                        col: 0,
                    };
                    true
                })
            },
        ),
        (
            "table.insertColLeft",
            "Insert a column to the left.",
            |cx, _| {
                rewrite(cx, |cells, a, c| {
                    for row in cells.iter_mut() {
                        row.insert(c.col, String::new());
                    }
                    a.insert(c.col, Align::None);
                    true
                })
            },
        ),
        (
            "table.insertColRight",
            "Insert a column to the right.",
            |cx, _| {
                rewrite(cx, |cells, a, c| {
                    for row in cells.iter_mut() {
                        row.insert(c.col + 1, String::new());
                    }
                    a.insert(c.col + 1, Align::None);
                    c.col += 1;
                    true
                })
            },
        ),
        (
            "table.deleteRow",
            "Delete the row (the header row only goes with the table).",
            |cx, _| {
                rewrite(cx, |cells, _, c| {
                    if c.row == 0 || cells.len() <= 1 {
                        return false;
                    }
                    cells.remove(c.row);
                    c.row = c.row.min(cells.len() - 1);
                    true
                })
            },
        ),
        ("table.deleteCol", "Delete the column.", |cx, _| {
            rewrite(cx, |cells, a, c| {
                if a.len() <= 1 {
                    return false;
                }
                for row in cells.iter_mut() {
                    row.remove(c.col);
                }
                a.remove(c.col);
                c.col = c.col.min(a.len() - 1);
                true
            })
        }),
        ("table.moveRowUp", "Move the row up (Alt+Up).", |cx, _| {
            rewrite(cx, |cells, _, c| {
                if c.row <= 1 {
                    return false;
                }
                cells.swap(c.row, c.row - 1);
                c.row -= 1;
                true
            })
        }),
        (
            "table.moveRowDown",
            "Move the row down (Alt+Down).",
            |cx, _| {
                rewrite(cx, |cells, _, c| {
                    if c.row == 0 || c.row + 1 >= cells.len() {
                        return false;
                    }
                    cells.swap(c.row, c.row + 1);
                    c.row += 1;
                    true
                })
            },
        ),
        ("table.moveColLeft", "Move the column left.", |cx, _| {
            rewrite(cx, |cells, a, c| {
                if c.col == 0 {
                    return false;
                }
                for row in cells.iter_mut() {
                    row.swap(c.col, c.col - 1);
                }
                a.swap(c.col, c.col - 1);
                c.col -= 1;
                true
            })
        }),
        ("table.moveColRight", "Move the column right.", |cx, _| {
            rewrite(cx, |cells, a, c| {
                if c.col + 1 >= a.len() {
                    return false;
                }
                for row in cells.iter_mut() {
                    row.swap(c.col, c.col + 1);
                }
                a.swap(c.col, c.col + 1);
                c.col += 1;
                true
            })
        }),
        ("table.alignLeft", "Align the column left.", |cx, _| {
            rewrite(cx, |_, a, c| {
                a[c.col] = Align::Left;
                true
            })
        }),
        ("table.alignCenter", "Center the column.", |cx, _| {
            rewrite(cx, |_, a, c| {
                a[c.col] = Align::Center;
                true
            })
        }),
        ("table.alignRight", "Align the column right.", |cx, _| {
            rewrite(cx, |_, a, c| {
                a[c.col] = Align::Right;
                true
            })
        }),
        (
            "table.sortByColumnAsc",
            "Sort rows by this column, ascending (numbers compare numerically).",
            |cx, _| sort(cx, false),
        ),
        (
            "table.sortByColumnDesc",
            "Sort rows by this column, descending.",
            |cx, _| sort(cx, true),
        ),
        ("table.format", "Re-align the table source.", |cx, _| {
            rewrite(cx, |_, _, _| true)
        }),
    ];
    for (id, doc, run) in cmds {
        r.register(FnCommand::new(edits(spec(id, T, "ED-083", doc)), *run).enabled(in_table));
    }
    r.register(
        FnCommand::new(
            edits({
                let mut s = spec("table.moveColumnTo", T, "ED-084", "Move column `from` to index `to` (dragging a column grip).");
                s.params = Some(r#"{"type":"object","required":["from","to"],"properties":{"from":{"type":"integer"},"to":{"type":"integer"}}}"#);
                s
            }),
            |cx, p| {
                let (from, to) = (idx(p, "from"), idx(p, "to"));
                rewrite(cx, |cells, a, c| {
                    if from >= a.len() || from == to {
                        return false;
                    }
                    let to = to.min(a.len() - 1);
                    for row in cells.iter_mut() {
                        let v = row.remove(from);
                        row.insert(to, v);
                    }
                    let al = a.remove(from);
                    a.insert(to, al);
                    *c = Cell { row: c.row, col: to };
                    true
                })
            },
        )
        .enabled(in_table),
    );
    r.register(
        FnCommand::new(
            edits({
                let mut s = spec("table.moveRowTo", T, "ED-084", "Move body row `from` to index `to` (rows count from 1; the header stays).");
                s.params = Some(r#"{"type":"object","required":["from","to"],"properties":{"from":{"type":"integer"},"to":{"type":"integer"}}}"#);
                s
            }),
            |cx, p| {
                let (from, to) = (idx(p, "from"), idx(p, "to"));
                rewrite(cx, |cells, _, c| {
                    if from == 0 || from >= cells.len() || from == to {
                        return false;
                    }
                    let to = to.clamp(1, cells.len() - 1);
                    let r = cells.remove(from);
                    cells.insert(to, r);
                    *c = Cell { row: to, col: c.col };
                    true
                })
            },
        )
        .enabled(in_table),
    );
    r.register(
        FnCommand::new(
            edits(spec(
                "table.deleteTable",
                T,
                "ED-083",
                "Delete the whole table.",
            )),
            |cx, _| {
                let Some((t, _)) = current(cx) else {
                    return Ok(Outcome::Noop);
                };
                let e = &mut cx.app.editor;
                let tx = inkra_doc::Transaction::single(t.span.clone(), "")
                    .with_selection_before(e.sel)
                    .with_selection_after(Selection::caret(t.span.start));
                e.apply(tx, UndoPolicy::NewGroup);
                Ok(Outcome::Done)
            },
        )
        .enabled(in_table),
    );
    r.register(
        FnCommand::new(
            edits({
                let mut s = spec("table.resize", T, "ED-083", "Set the number of body rows and columns (dialog without params).");
                s.params = Some(r#"{"type":"object","properties":{"rows":{"type":"integer","minimum":1},"cols":{"type":"integer","minimum":1}}}"#);
                s
            }),
            |cx, p| match (p.get("rows").and_then(Value::as_u64), p.get("cols").and_then(Value::as_u64)) {
                (Some(rows), Some(cols)) => rewrite(cx, |cells, a, c| {
                    let (rows, cols) = (rows.clamp(1, 1000) as usize, cols.clamp(1, 64) as usize);
                    a.resize(cols, Align::None);
                    for row in cells.iter_mut() {
                        row.resize(cols, String::new());
                    }
                    cells.resize(rows + 1, vec![String::new(); cols]);
                    c.row = c.row.min(rows);
                    c.col = c.col.min(cols - 1);
                    true
                }),
                _ => {
                    cx.effects.push(Effect::ShowTableResize);
                    Ok(Outcome::Done)
                }
            },
        )
        .enabled(in_table),
    );
    for (id, doc, kind) in [
        ("table.copyAsMarkdown", "Copy the table as Markdown.", 0u8),
        ("table.copyAsHtml", "Copy the table as HTML.", 1),
        ("table.copyAsCsv", "Copy the table as CSV.", 2),
    ] {
        let run: fn(&mut CommandContext<'_>, &Value) -> CommandResult = match kind {
            0 => |cx, _| copy(cx, 0),
            1 => |cx, _| copy(cx, 1),
            _ => |cx, _| copy(cx, 2),
        };
        r.register(FnCommand::new(spec(id, T, "ED-083", doc), run).enabled(in_table));
    }
    r.register(
        FnCommand::new(
            spec(
                "table.nextCell",
                T,
                "ED-014",
                "Move to the next cell (adds a row after the last cell).",
            ),
            |cx, _| step_cell(cx, true),
        )
        .enabled(in_table),
    );
    r.register(
        FnCommand::new(
            spec("table.prevCell", T, "ED-014", "Move to the previous cell."),
            |cx, _| step_cell(cx, false),
        )
        .enabled(in_table),
    );
}

fn sort(cx: &mut CommandContext<'_>, desc: bool) -> CommandResult {
    rewrite(cx, |cells, _, c| {
        if cells.len() <= 2 {
            return false;
        }
        let col = c.col;
        let mut body: Vec<Vec<String>> = cells.drain(1..).collect();
        body.sort_by(|a, b| {
            let o = table::smart_cmp(
                a.get(col).map_or("", String::as_str),
                b.get(col).map_or("", String::as_str),
            );
            if desc { o.reverse() } else { o }
        });
        cells.extend(body);
        true
    })
}

fn copy(cx: &mut CommandContext<'_>, kind: u8) -> CommandResult {
    let Some((t, _)) = current(cx) else {
        return Ok(Outcome::Noop);
    };
    let doc = &cx.app.editor.doc;
    let cells = t.texts(doc);
    let md = table::render(&cells, &t.aligns, true, doc.newline(), "").text;
    let (text, html) = match kind {
        0 => (md, None),
        1 => {
            let html = inkra_markdown::html::to_html(&md, doc.options());
            (md, Some(html))
        }
        _ => (table::to_csv(&cells, doc.newline()), None),
    };
    cx.effects.push(Effect::SetClipboard { text, html });
    Ok(Outcome::Done)
}
