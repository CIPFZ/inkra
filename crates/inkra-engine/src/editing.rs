//! Editing commands (`edit.*`, ED-010..ED-016, ED-110/111, ED-120).

use serde_json::Value;

use crate::command::{
    Category, CommandRegistry, CommandResult, CommandSpec, FnCommand, Outcome, UndoGroup, spec,
};
use crate::editor::{Break, ClipboardContent, Unit, ViewMode, ops};
use crate::state::{CommandContext, Effect};
use inkra_doc::{Selection, UndoPolicy};
use inkra_text::LineEnding;

fn with_params(mut s: CommandSpec, params: &'static str) -> CommandSpec {
    s.params = Some(params);
    s
}

fn edits(mut s: CommandSpec) -> CommandSpec {
    s.undo_group = UndoGroup::Own;
    s
}

fn unit(params: &Value) -> Unit {
    match params.get("unit").and_then(Value::as_str) {
        Some("word") => Unit::Word,
        Some("line") => Unit::Line,
        _ => Unit::Char,
    }
}

fn done(cx: &mut CommandContext<'_>) -> CommandResult {
    cx.effects.push(Effect::ScrollToCaret);
    Ok(Outcome::Done)
}

/// Registers the editing commands.
pub fn register(r: &mut CommandRegistry) {
    use Category::Edit;
    r.register(FnCommand::new(
        edits(with_params(
            spec("edit.moveText", Edit, "ED-017", "Move (or with `copy`, copy) the text `from..to` to position `at` (drag and drop)."),
            r#"{"type":"object","required":["from","to","at"],"properties":{"from":{"type":"integer"},"to":{"type":"integer"},"at":{"type":"integer"},"copy":{"type":"boolean"}}}"#,
        )),
        |cx, p| {
            let n = |k: &str| p.get(k).and_then(Value::as_u64).map(|v| v as usize);
            let (Some(from), Some(to), Some(at)) = (n("from"), n("to"), n("at")) else {
                return Ok(Outcome::Done);
            };
            let e = &mut cx.app.editor;
            let len = e.doc.text().len();
            let (from, to, at) = (from.min(len), to.min(len), at.min(len));
            if from >= to || (from..=to).contains(&at) {
                return Ok(Outcome::Done);
            }
            let text = e.doc.text().slice(from..to).into_owned();
            let copy = p.get("copy").and_then(Value::as_bool).unwrap_or(false);
            let mut edits = vec![inkra_doc::Edit { range: at..at, text: text.clone() }];
            if !copy {
                edits.push(inkra_doc::Edit { range: from..to, text: String::new() });
            }
            edits.sort_by_key(|x| x.range.start);
            // The moved text stays selected at its new place.
            let start = if copy || at < from { at } else { at - (to - from) };
            let tx = inkra_doc::Transaction::many(edits).with_selection_before(e.sel).with_selection_after(Selection::new(start, start + text.len()));
            e.apply(tx, UndoPolicy::NewGroup);
            done(cx)
        },
    ));
    r.register(FnCommand::new(
        edits(with_params(
            spec("edit.insertFileLink", Edit, "ED-017", "Insert a link to a file at `at` (an image for picture files), relative to the document when possible."),
            r#"{"type":"object","required":["path"],"properties":{"path":{"type":"string"},"at":{"type":"integer"}}}"#,
        )),
        |cx, p| {
            let Some(path) = p.get("path").and_then(Value::as_str).map(std::path::PathBuf::from) else {
                return Ok(Outcome::Done);
            };
            let e = &mut cx.app.editor;
            let at = p.get("at").and_then(Value::as_u64).map_or(e.sel.head, |v| v as usize).min(e.doc.text().len());
            let rel = inkra_workspace::assets::markdown_src(e.path.as_deref(), &path, true);
            let name = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            let image = path.extension().and_then(|x| x.to_str()).is_some_and(|x| matches!(x.to_ascii_lowercase().as_str(), "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "bmp"));
            let text = if image { format!("![{name}]({rel})") } else { format!("[{name}]({rel})") };
            let tx = inkra_doc::Transaction::single(at..at, &text).with_selection_before(e.sel).with_selection_after(Selection::caret(at + text.len()));
            e.apply(tx, UndoPolicy::NewGroup);
            done(cx)
        },
    ));
    r.register(FnCommand::new(
        edits(with_params(
            spec("edit.replaceRange", Edit, "ED-103", "Replace a source range with text and put the caret after it (completions)."),
            r#"{"type":"object","required":["start","end","text"],"properties":{"start":{"type":"integer"},"end":{"type":"integer"},"text":{"type":"string"}}}"#,
        )),
        |cx, p| {
            let e = &mut cx.app.editor;
            let len = e.doc.text().len();
            let t = e.doc.text();
            let a = t.snap((p.get("start").and_then(Value::as_u64).unwrap_or(0) as usize).min(len));
            let b = t.snap((p.get("end").and_then(Value::as_u64).unwrap_or(0) as usize).min(len)).max(a);
            let text = p.get("text").and_then(Value::as_str).unwrap_or_default().to_owned();
            let end = a + text.len();
            let tx = inkra_doc::Transaction::single(a..b, &text).with_selection_before(e.sel).with_selection_after(Selection::caret(end));
            e.doc.break_undo_group();
            e.apply(tx, UndoPolicy::NewGroup);
            done(cx)
        },
    ));
    r.register(
        FnCommand::new(
            edits(spec(
                "edit.undo",
                Edit,
                "ED-015",
                "Undo the last change group.",
            )),
            |cx, _| {
                cx.app.editor.undo();
                done(cx)
            },
        )
        .enabled(|cx| cx.app.editor.doc.undo_stack().can_undo()),
    );
    r.register(
        FnCommand::new(
            edits(spec(
                "edit.redo",
                Edit,
                "ED-015",
                "Redo the last undone change group.",
            )),
            |cx, _| {
                cx.app.editor.redo();
                done(cx)
            },
        )
        .enabled(|cx| cx.app.editor.doc.undo_stack().can_redo()),
    );
    r.register(
        FnCommand::new(
            edits(spec(
                "edit.cut",
                Edit,
                "ED-016",
                "Cut the selection to the clipboard.",
            )),
            |cx, _| {
                if let Some((text, html)) = cx.app.editor.cut() {
                    cx.effects.push(Effect::SetClipboard {
                        text,
                        html: Some(html),
                    });
                }
                done(cx)
            },
        )
        .enabled(|cx| !cx.app.editor.sel.is_empty()),
    );
    r.register(
        FnCommand::new(
            spec(
                "edit.copy",
                Edit,
                "ED-016",
                "Copy the selection as Markdown (text/plain) and rendered HTML (text/html).",
            ),
            |cx, _| {
                if let Some((source, _)) =
                    crate::editor::clipboard::copy_payload(&cx.app.editor.doc, cx.app.editor.sel)
                {
                    // EX-006: the same generator as HTML export.
                    let html = crate::exporting::clipboard_html(cx.app, &source);
                    // PR-037: text/plain is the source or plain text.
                    let text = if cx.app.prefs.string("editor.copyAs") == "plain" {
                        crate::viewing::plain(&cx.app.editor.doc, cx.app.editor.sel)
                    } else {
                        source
                    };
                    cx.effects.push(Effect::SetClipboard {
                        text,
                        html: Some(html),
                    });
                }
                Ok(Outcome::Done)
            },
        )
        .enabled(|cx| !cx.app.editor.sel.is_empty()),
    );
    for (id, plain, doc) in [
        (
            "edit.paste",
            false,
            "Paste: images, files, HTML (converted to Markdown) or text (ED-016 rules).",
        ),
        (
            "edit.pastePlain",
            true,
            "Paste as plain text without conversion.",
        ),
    ] {
        r.register(FnCommand::new(
            edits(with_params(
                spec(id, Edit, "ED-016", doc),
                r#"{"type":"object","properties":{"text":{"type":"string"},"html":{"type":"string"},"files":{"type":"array","items":{"type":"string"}}}}"#,
            )),
            if plain { paste_plain } else { paste_rich },
        ));
    }
    r.register(FnCommand::new(
        spec(
            "edit.selectAll",
            Edit,
            "ED-011",
            "Select all (inside a code block: the block first).",
        ),
        |cx, _| {
            let e = &mut cx.app.editor;
            e.sel = ops::select_all(&e.doc, e.sel);
            Ok(Outcome::Done)
        },
    ));
    r.register(FnCommand::new(
        spec(
            "edit.selectWord",
            Edit,
            "ED-011",
            "Select the word at the caret.",
        ),
        |cx, _| {
            let e = &mut cx.app.editor;
            e.sel = ops::word_at(&e.doc, e.sel.head);
            Ok(Outcome::Done)
        },
    ));
    r.register(FnCommand::new(
        spec(
            "edit.selectLine",
            Edit,
            "ED-011",
            "Select the current line.",
        ),
        |cx, _| {
            let e = &mut cx.app.editor;
            e.sel = ops::line_at(&e.doc, e.sel.head);
            Ok(Outcome::Done)
        },
    ));
    r.register(FnCommand::new(
        edits(spec(
            "edit.deleteWord",
            Edit,
            "ED-012",
            "Delete the word at the caret.",
        )),
        |cx, _| {
            let e = &mut cx.app.editor;
            let tx = ops::delete_word(&e.doc, e.sel);
            e.apply(tx, UndoPolicy::NewGroup);
            done(cx)
        },
    ));
    r.register(FnCommand::new(
        edits(spec(
            "edit.deleteLine",
            Edit,
            "ED-012",
            "Delete the current line.",
        )),
        |cx, _| {
            let e = &mut cx.app.editor;
            let tx = ops::delete_line(&e.doc, e.sel);
            e.apply_with_renumber(tx, UndoPolicy::NewGroup);
            done(cx)
        },
    ));
    r.register(FnCommand::new(
        edits(with_params(
            spec(
                "edit.insertText",
                Edit,
                "ED-010",
                "Type text at the caret (with auto-pairing).",
            ),
            r#"{"type":"object","required":["text"],"properties":{"text":{"type":"string"}}}"#,
        )),
        |cx, p| {
            let text = p
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            cx.app.editor.type_text(&text);
            done(cx)
        },
    ));
    r.register(FnCommand::new(
        edits(with_params(
            spec(
                "edit.newline",
                Edit,
                "ED-013",
                "Insert a line break; continues lists, quotes and code indentation.",
            ),
            r#"{"type":"object","properties":{"soft":{"type":"boolean"}}}"#,
        )),
        |cx, p| {
            let soft = p.get("soft").and_then(Value::as_bool).unwrap_or(false);
            cx.app
                .editor
                .newline(if soft { Break::Soft } else { Break::Enter });
            done(cx)
        },
    ));
    for (id, fwd, doc) in [
        (
            "edit.deleteBackward",
            false,
            "Backspace by char, word or to line start.",
        ),
        (
            "edit.deleteForward",
            true,
            "Delete forward by char, word or to line end.",
        ),
    ] {
        r.register(FnCommand::new(
            edits(with_params(
                spec(id, Edit, "ED-012", doc),
                r#"{"type":"object","properties":{"unit":{"enum":["char","word","line"]}}}"#,
            )),
            if fwd {
                |cx, p| {
                    cx.app.editor.delete_forward(unit(p));
                    done(cx)
                }
            } else {
                |cx, p| {
                    cx.app.editor.delete_backward(unit(p));
                    done(cx)
                }
            },
        ));
    }
    r.register(FnCommand::new(
        edits(with_params(
            spec(
                "edit.tab",
                Edit,
                "ED-014",
                "Tab / Shift+Tab: indent or outdent list items and code, or insert spaces.",
            ),
            r#"{"type":"object","properties":{"outdent":{"type":"boolean"}}}"#,
        )),
        |cx, p| {
            let outdent = p.get("outdent").and_then(Value::as_bool).unwrap_or(false);
            cx.app.editor.tab(outdent);
            done(cx)
        },
    ));
    for (id, outdent, doc) in [
        ("paragraph.indent", false, "Indent the list item (nest it)."),
        ("paragraph.outdent", true, "Outdent the list item."),
    ] {
        r.register(FnCommand::new(
            edits(spec(id, Category::Paragraph, "ED-061", doc)),
            if outdent {
                |cx, _| {
                    cx.app.editor.tab(true);
                    done(cx)
                }
            } else {
                |cx, _| {
                    cx.app.editor.tab(false);
                    done(cx)
                }
            },
        ));
    }
    r.register(FnCommand::new(
        spec(
            "edit.jumpTop",
            Edit,
            "ED-020",
            "Move the caret to the start of the document.",
        ),
        |cx, _| {
            cx.app.editor.sel = Selection::caret(0);
            done(cx)
        },
    ));
    r.register(FnCommand::new(
        spec(
            "edit.jumpBottom",
            Edit,
            "ED-020",
            "Move the caret to the end of the document.",
        ),
        |cx, _| {
            let len = cx.app.editor.doc.text().len();
            cx.app.editor.sel = Selection::caret(len);
            done(cx)
        },
    ));
    r.register(FnCommand::new(
        spec(
            "edit.jumpSelection",
            Edit,
            "ED-020",
            "Scroll the selection into view.",
        ),
        |cx, _| done(cx),
    ));
    r.register(FnCommand::new(
        spec(
            "edit.find",
            Edit,
            "ED-110",
            "Open the find bar (pre-filled with the selection).",
        ),
        |cx, _| {
            open_find(cx, false);
            Ok(Outcome::Done)
        },
    ));
    r.register(FnCommand::new(
        spec(
            "edit.replace",
            Edit,
            "ED-111",
            "Open the find bar with the replace row.",
        ),
        |cx, _| {
            open_find(cx, true);
            Ok(Outcome::Done)
        },
    ));
    for (id, fwd, doc) in [
        ("edit.findNext", true, "Select the next match."),
        ("edit.findPrev", false, "Select the previous match."),
    ] {
        r.register(
            FnCommand::new(
                spec(id, Edit, "ED-110", doc),
                if fwd {
                    |cx, _| find_step(cx, true)
                } else {
                    |cx, _| find_step(cx, false)
                },
            )
            .enabled(|cx| !cx.app.editor.find.query.is_empty()),
        );
    }
    r.register(
        FnCommand::new(
            edits(spec(
                "edit.replaceCurrent",
                Edit,
                "ED-111",
                "Replace the current match.",
            )),
            |cx, _| {
                let e = &mut cx.app.editor;
                e.find.refresh(&e.doc, e.sel.start());
                if let Some(tx) = e.find.replace_current(&e.doc, e.sel) {
                    e.apply(tx, UndoPolicy::NewGroup);
                    e.find.refresh(&e.doc, e.sel.head);
                    if let Some(r) = e.find.step(e.sel.head.saturating_sub(1), true) {
                        e.sel = Selection::new(r.start, r.end);
                    }
                }
                done(cx)
            },
        )
        .enabled(|cx| !cx.app.editor.find.query.is_empty()),
    );
    r.register(
        FnCommand::new(
            edits(spec(
                "edit.replaceAll",
                Edit,
                "ED-111",
                "Replace all matches (one undo group).",
            )),
            |cx, _| {
                let e = &mut cx.app.editor;
                e.find.refresh(&e.doc, e.sel.start());
                if let Some(tx) = e.find.replace_all(&e.doc, e.sel) {
                    let mapped = tx.map_selection(e.sel);
                    e.apply(tx.with_selection_after(mapped), UndoPolicy::NewGroup);
                }
                done(cx)
            },
        )
        .enabled(|cx| !cx.app.editor.find.query.is_empty()),
    );
    r.register(
        FnCommand::new(
            spec(
                "view.sourceMode",
                Category::View,
                "ED-120",
                "Toggle between live rendering and source mode.",
            ),
            |cx, _| {
                let e = &mut cx.app.editor;
                e.mode = match e.mode {
                    ViewMode::Live => ViewMode::Source,
                    ViewMode::Source => ViewMode::Live,
                };
                done(cx)
            },
        )
        .checked(|cx| Some(cx.app.editor.mode == ViewMode::Source)),
    );
    r.register(FnCommand::new(
        edits(with_params(
            spec("edit.lineEnding", Edit, "ED-106", "Convert all line endings to LF or CRLF."),
            r#"{"type":"object","required":["style"],"properties":{"style":{"enum":["lf","crlf"]}}}"#,
        )),
        |cx, p| {
            let eol = match p.get("style").and_then(Value::as_str) {
                Some("crlf") => LineEnding::Crlf,
                _ => LineEnding::Lf,
            };
            let e = &mut cx.app.editor;
            e.doc.convert_line_endings(eol);
            e.sel = e.sel.clamp(e.doc.text().len());
            Ok(Outcome::Done)
        },
    ));
}

fn clipboard_from(p: &Value) -> Option<ClipboardContent> {
    if p.get("text").is_none() && p.get("html").is_none() && p.get("files").is_none() {
        return None;
    }
    Some(ClipboardContent {
        text: p.get("text").and_then(Value::as_str).map(str::to_owned),
        html: p.get("html").and_then(Value::as_str).map(str::to_owned),
        files: p
            .get("files")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_str).map(Into::into).collect())
            .unwrap_or_default(),
        from_inkra: p.get("fromInkra").and_then(Value::as_bool).unwrap_or(false),
    })
}

fn paste_rich(cx: &mut CommandContext<'_>, p: &Value) -> CommandResult {
    match clipboard_from(p) {
        Some(c) => {
            cx.app.editor.paste(&c, false);
            done(cx)
        }
        None => {
            cx.effects.push(Effect::RequestPaste { plain: false });
            Ok(Outcome::Done)
        }
    }
}

fn paste_plain(cx: &mut CommandContext<'_>, p: &Value) -> CommandResult {
    match clipboard_from(p) {
        Some(c) => {
            cx.app.editor.paste(&c, true);
            done(cx)
        }
        None => {
            cx.effects.push(Effect::RequestPaste { plain: true });
            Ok(Outcome::Done)
        }
    }
}

fn open_find(cx: &mut CommandContext<'_>, replace: bool) {
    let e = &mut cx.app.editor;
    e.find.open = true;
    e.find.replace_open |= replace;
    if !e.sel.is_empty() {
        let s = e.doc.text().slice(e.sel.range()).into_owned();
        if !s.contains('\n') {
            e.find.query = s;
        }
    }
    e.find.invalidate();
    cx.effects.push(Effect::FocusFind);
}

fn find_step(cx: &mut CommandContext<'_>, forward: bool) -> CommandResult {
    let e = &mut cx.app.editor;
    e.find.refresh(&e.doc, e.sel.start());
    let from = if forward {
        e.sel.end().saturating_sub(1).max(e.sel.start())
    } else {
        e.sel.start()
    };
    if let Some(r) = e.find.step(from, forward) {
        e.sel = Selection::new(r.start, r.end);
    }
    done(cx)
}
