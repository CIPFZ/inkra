//! Inline formatting (`format.*`, SPEC 3.3) and block (`paragraph.*`, SPEC 3.4)
//! commands.

use serde_json::Value;

use crate::command::{
    Category, CommandRegistry, CommandResult, CommandSpec, FnCommand, Outcome, UndoGroup, spec,
};
use crate::editor::format_ops::{self as f, InlineFormat, ListStyle};
use crate::editor::ops;
use crate::state::{CommandContext, Effect};
use inkra_doc::{Transaction, UndoPolicy};

fn edits(mut s: CommandSpec) -> CommandSpec {
    s.undo_group = UndoGroup::Own;
    s
}

fn params(mut s: CommandSpec, p: &'static str) -> CommandSpec {
    s.params = Some(p);
    s
}

fn apply(cx: &mut CommandContext<'_>, tx: Option<Transaction>) -> CommandResult {
    match tx {
        Some(t) if !t.edits.is_empty() => {
            let e = &mut cx.app.editor;
            e.doc.break_undo_group();
            e.apply_with_renumber(t, UndoPolicy::NewGroup);
            cx.effects.push(Effect::ScrollToCaret);
            Ok(Outcome::Done)
        }
        Some(t) => {
            if let Some(s) = t.selection_after {
                cx.app.editor.sel = s;
            }
            cx.effects.push(Effect::ScrollToCaret);
            Ok(Outcome::Noop)
        }
        None => Ok(Outcome::Noop),
    }
}

fn strong(cx: &CommandContext<'_>) -> String {
    let s = cx.app.prefs.string("markdown.strongMarker");
    if s == "__" { s } else { "**".to_owned() }
}

fn emph(cx: &CommandContext<'_>) -> String {
    let s = cx.app.prefs.string("markdown.emphasisMarker");
    if s == "_" { s } else { "*".to_owned() }
}

fn bullet(cx: &CommandContext<'_>) -> char {
    cx.app
        .prefs
        .string("markdown.bulletMarker")
        .chars()
        .next()
        .filter(|c| matches!(c, '-' | '*' | '+'))
        .unwrap_or('-')
}

fn inline(cx: &mut CommandContext<'_>, fmt: InlineFormat) -> CommandResult {
    let (st, em) = (strong(cx), emph(cx));
    let e = &cx.app.editor;
    let tx = f::toggle_inline(&e.doc, e.sel, fmt, &st, &em);
    apply(cx, Some(tx))
}

type Run = fn(&mut CommandContext<'_>, &Value) -> CommandResult;

/// Registers formatting and paragraph commands.
pub fn register(r: &mut CommandRegistry) {
    use Category::{Format, Paragraph};
    let inline_cmds: &[(&str, &str, &str, Run)] = &[
        (
            "format.bold",
            "ED-030",
            "Toggle **bold** on the selection (or insert `****`).",
            |cx, _| inline(cx, InlineFormat::Bold),
        ),
        ("format.italic", "ED-031", "Toggle *italic*.", |cx, _| {
            inline(cx, InlineFormat::Italic)
        }),
        (
            "format.underline",
            "ED-032",
            "Toggle <u>underline</u>.",
            |cx, _| inline(cx, InlineFormat::Underline),
        ),
        (
            "format.code",
            "ED-033",
            "Toggle `inline code` (double backticks when the text has one).",
            |cx, _| inline(cx, InlineFormat::Code),
        ),
        (
            "format.strike",
            "ED-034",
            "Toggle ~~strikethrough~~.",
            |cx, _| inline(cx, InlineFormat::Strike),
        ),
        (
            "format.highlight",
            "ED-035",
            "Toggle ==highlight== (needs MD-063).",
            |cx, _| inline(cx, InlineFormat::Highlight),
        ),
        (
            "format.superscript",
            "ED-036",
            "Toggle ^superscript^ (needs MD-062).",
            |cx, _| inline(cx, InlineFormat::Superscript),
        ),
        (
            "format.subscript",
            "ED-037",
            "Toggle ~subscript~ (needs MD-061).",
            |cx, _| inline(cx, InlineFormat::Subscript),
        ),
        (
            "format.comment",
            "ED-038",
            "Wrap the selection in an HTML comment.",
            |cx, _| inline(cx, InlineFormat::Comment),
        ),
        (
            "format.inlineMath",
            "ED-042",
            "Toggle $inline math$.",
            |cx, _| inline(cx, InlineFormat::Math),
        ),
    ];
    for (id, req, doc, run) in inline_cmds {
        r.register(FnCommand::new(edits(spec(id, Format, req, doc)), *run));
    }
    r.register(FnCommand::new(
        edits(params(
            spec(
                "format.link",
                Format,
                "ED-039",
                "Insert or make a link `[text](url)`; the URL defaults to the clipboard.",
            ),
            r#"{"type":"object","properties":{"url":{"type":"string"}}}"#,
        )),
        |cx, p| {
            let url = p
                .get("url")
                .and_then(Value::as_str)
                .filter(|u| !u.is_empty());
            let e = &cx.app.editor;
            let tx = f::insert_link(&e.doc, e.sel, url);
            apply(cx, Some(tx))
        },
    ));
    r.register(FnCommand::new(
        edits(params(
            spec(
                "format.image",
                Format,
                "ED-040",
                "Insert an image: `src` inserts directly, otherwise a file picker opens.",
            ),
            r#"{"type":"object","properties":{"src":{"type":"string"},"alt":{"type":"string"}}}"#,
        )),
        |cx, p| match p.get("src").and_then(Value::as_str) {
            Some(src) => {
                let alt = p.get("alt").and_then(Value::as_str).unwrap_or_default();
                let e = &cx.app.editor;
                let tx = f::insert_image(&e.doc, e.sel, src, alt);
                apply(cx, Some(tx))
            }
            None => {
                cx.effects.push(Effect::PickImage);
                Ok(Outcome::Done)
            }
        },
    ));
    r.register(
        FnCommand::new(
            edits(spec(
                "format.removeLink",
                Format,
                "ED-130",
                "Turn the link at the caret back into plain text.",
            )),
            |cx, _| {
                let e = &cx.app.editor;
                let tx = f::remove_link(&e.doc, e.sel);
                apply(cx, tx)
            },
        )
        .enabled(|cx| f::link_ranges(&cx.app.editor.doc, cx.app.editor.sel.head).is_some()),
    );
    r.register(FnCommand::new(
        edits(params(
            spec("format.insertImageFile", Format, "ED-092", "Insert an image file, copying it per the image preferences (`pasted`: a saved clipboard bitmap)."),
            r#"{"type":"object","required":["path"],"properties":{"path":{"type":"string"},"pasted":{"type":"boolean"}}}"#,
        )),
        |cx, p| {
            let src = std::path::PathBuf::from(p.get("path").and_then(Value::as_str).unwrap_or_default());
            let pasted = p.get("pasted").and_then(Value::as_bool).unwrap_or(false);
            let path = place_image(cx, &src, pasted).map_err(|e| crate::command::CommandError::Failed { id: "format.insertImageFile".into(), msg: e.to_string() })?;
            let prefer_rel = cx.app.prefs.bool("image.preferRelative");
            let md = inkra_workspace::assets::markdown_src(cx.app.editor.path.as_deref(), &path, prefer_rel);
            let alt = if pasted { String::new() } else { src.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default() };
            let e = &cx.app.editor;
            let tx = f::insert_image(&e.doc, e.sel, &md, &alt);
            apply(cx, Some(tx))
        },
    ));
    r.register(FnCommand::new(
        edits(spec(
            "format.imageLocal",
            Format,
            "ED-040",
            "Insert a local image file.",
        )),
        |cx, _| {
            cx.effects.push(Effect::PickImage);
            Ok(Outcome::Done)
        },
    ));

    for level in 1..=6u8 {
        let id: &'static str = [
            "paragraph.heading1",
            "paragraph.heading2",
            "paragraph.heading3",
            "paragraph.heading4",
            "paragraph.heading5",
            "paragraph.heading6",
        ][level as usize - 1];
        let run: fn(&mut CommandContext<'_>, &Value) -> CommandResult = match level {
            1 => |cx, _| heading(cx, 1),
            2 => |cx, _| heading(cx, 2),
            3 => |cx, _| heading(cx, 3),
            4 => |cx, _| heading(cx, 4),
            5 => |cx, _| heading(cx, 5),
            _ => |cx, _| heading(cx, 6),
        };
        r.register(
            FnCommand::new(
                edits(spec(
                    id,
                    Paragraph,
                    "ED-050",
                    "Make the block(s) a heading of this level (again: back to paragraph).",
                )),
                run,
            )
            .checked(match level {
                1 => |cx| Some(heading_level(cx) == Some(1)),
                2 => |cx| Some(heading_level(cx) == Some(2)),
                3 => |cx| Some(heading_level(cx) == Some(3)),
                4 => |cx| Some(heading_level(cx) == Some(4)),
                5 => |cx| Some(heading_level(cx) == Some(5)),
                _ => |cx| Some(heading_level(cx) == Some(6)),
            }),
        );
    }
    r.register(FnCommand::new(
        edits(spec(
            "paragraph.normal",
            Paragraph,
            "ED-051",
            "Make the block(s) plain paragraphs.",
        )),
        |cx, _| heading(cx, 0),
    ));
    r.register(FnCommand::new(
        edits(spec(
            "paragraph.headingUp",
            Paragraph,
            "ED-052",
            "Promote: H2→H1, paragraph→H6.",
        )),
        |cx, _| {
            let e = &cx.app.editor;
            let tx = f::shift_heading(&e.doc, e.sel, true);
            apply(cx, tx)
        },
    ));
    r.register(FnCommand::new(
        edits(spec(
            "paragraph.headingDown",
            Paragraph,
            "ED-052",
            "Demote: H1→H2, H6→paragraph.",
        )),
        |cx, _| {
            let e = &cx.app.editor;
            let tx = f::shift_heading(&e.doc, e.sel, false);
            apply(cx, tx)
        },
    ));
    r.register(FnCommand::new(
        edits(spec(
            "paragraph.quote",
            Paragraph,
            "ED-056",
            "Toggle a block quote.",
        )),
        |cx, _| {
            let e = &cx.app.editor;
            let tx = f::toggle_quote(&e.doc, e.sel);
            apply(cx, tx)
        },
    ));
    for (id, style, req, doc) in [
        (
            "paragraph.orderedList",
            ListStyle::Ordered,
            "ED-057",
            "Toggle an ordered list.",
        ),
        (
            "paragraph.unorderedList",
            ListStyle::Bullet,
            "ED-058",
            "Toggle a bullet list.",
        ),
        (
            "paragraph.taskList",
            ListStyle::Task,
            "ED-059",
            "Toggle a task list.",
        ),
    ] {
        let run: fn(&mut CommandContext<'_>, &Value) -> CommandResult = match style {
            ListStyle::Ordered => |cx, _| list(cx, ListStyle::Ordered),
            ListStyle::Bullet => |cx, _| list(cx, ListStyle::Bullet),
            ListStyle::Task => |cx, _| list(cx, ListStyle::Task),
        };
        r.register(FnCommand::new(edits(spec(id, Paragraph, req, doc)), run));
    }
    r.register(FnCommand::new(
        edits(params(
            spec(
                "paragraph.taskStatus",
                Paragraph,
                "ED-060",
                "Toggle the task checkbox at the caret (or at `at`).",
            ),
            r#"{"type":"object","properties":{"at":{"type":"integer"}}}"#,
        )),
        |cx, p| {
            let at = p.get("at").and_then(Value::as_u64).map(|v| v as usize);
            let e = &cx.app.editor;
            let tx = ops::toggle_task(&e.doc, e.sel, at);
            apply(cx, tx)
        },
    ));
    r.register(FnCommand::new(
        edits(params(
            spec(
                "paragraph.codeFences",
                Paragraph,
                "ED-054",
                "Turn the paragraph or selection into a fenced code block.",
            ),
            r#"{"type":"object","properties":{"lang":{"type":"string"}}}"#,
        )),
        |cx, p| {
            let lang = p
                .get("lang")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .unwrap_or_else(|| cx.app.prefs.string("markdown.codeDefaultLanguage"));
            let e = &cx.app.editor;
            let tx = f::code_fences(&e.doc, e.sel, &lang);
            apply(cx, tx)
        },
    ));
    r.register(FnCommand::new(
        edits(spec(
            "paragraph.hr",
            Paragraph,
            "ED-063",
            "Insert a horizontal rule after the block.",
        )),
        |cx, _| {
            let e = &cx.app.editor;
            let tx = f::insert_rule(&e.doc, e.sel);
            apply(cx, Some(tx))
        },
    ));
    r.register(FnCommand::new(
        edits(spec(
            "paragraph.insertAbove",
            Paragraph,
            "ED-070",
            "Insert an empty paragraph before the block.",
        )),
        |cx, _| {
            let e = &cx.app.editor;
            let tx = f::insert_paragraph(&e.doc, e.sel, true);
            apply(cx, Some(tx))
        },
    ));
    r.register(FnCommand::new(
        edits(spec(
            "paragraph.insertBelow",
            Paragraph,
            "ED-070",
            "Insert an empty paragraph after the block.",
        )),
        |cx, _| {
            let e = &cx.app.editor;
            let tx = f::insert_paragraph(&e.doc, e.sel, false);
            apply(cx, Some(tx))
        },
    ));
    r.register(FnCommand::new(
        edits(params(
            spec("paragraph.table", Paragraph, "ED-053", "Insert a table (`rows`, `cols`; without them a dialog asks)."),
            r#"{"type":"object","properties":{"rows":{"type":"integer","minimum":1},"cols":{"type":"integer","minimum":1}}}"#,
        )),
        |cx, p| match (p.get("rows").and_then(Value::as_u64), p.get("cols").and_then(Value::as_u64)) {
            (Some(rows), Some(cols)) => {
                let e = &cx.app.editor;
                let nl = e.doc.newline();
                let t = f::table_source(rows as usize, cols as usize, nl);
                let tx = f::insert_block_after(&e.doc, e.sel, &t, 2);
                apply(cx, Some(tx))
            }
            _ => {
                cx.effects.push(Effect::ShowTableDialog);
                Ok(Outcome::Done)
            }
        },
    ));
    r.register(FnCommand::new(
        edits(spec(
            "paragraph.mathBlock",
            Paragraph,
            "ED-055",
            "Insert a `$$` math block.",
        )),
        |cx, _| {
            let e = &cx.app.editor;
            let nl = e.doc.newline();
            let t = format!("$${nl}{nl}$$");
            let tx = f::insert_block_after(&e.doc, e.sel, &t, 2 + nl.len());
            apply(cx, Some(tx))
        },
    ));
    r.register(FnCommand::new(
        edits(spec(
            "paragraph.toc",
            Paragraph,
            "ED-064",
            "Insert `[TOC]`.",
        )),
        |cx, _| {
            let e = &cx.app.editor;
            let tx = f::insert_block_after(&e.doc, e.sel, "[TOC]", 5);
            apply(cx, Some(tx))
        },
    ));
    r.register(
        FnCommand::new(
            edits(spec(
                "paragraph.frontMatter",
                Paragraph,
                "ED-065",
                "Insert YAML front matter at the top.",
            )),
            |cx, _| {
                let e = &cx.app.editor;
                let nl = e.doc.newline();
                let t = format!("---{nl}{nl}---{nl}");
                let tx = Transaction::single(0..0, &t)
                    .with_selection_before(e.sel)
                    .with_selection_after(inkra_doc::Selection::caret(3 + nl.len()));
                apply(cx, Some(tx))
            },
        )
        .enabled(|cx| {
            !cx.app.editor.doc.ast().iter().next().is_some_and(|b| {
                matches!(b.block.kind, inkra_markdown::BlockKind::FrontMatter { .. })
            })
        }),
    );
    r.register(FnCommand::new(
        edits(spec(
            "paragraph.ensureTrailing",
            Paragraph,
            "ED-006",
            "Ensure an empty paragraph at the end and move there.",
        )),
        |cx, _| {
            let e = &cx.app.editor;
            let tx = f::ensure_trailing(&e.doc, e.sel);
            apply(cx, Some(tx))
        },
    ));
    r.register(FnCommand::new(
        edits(spec(
            "edit.exitBlock",
            Category::Edit,
            "ED-013",
            "Ctrl+Enter: leave a code/math block, or add a paragraph above the first block.",
        )),
        |cx, _| {
            let e = &cx.app.editor;
            let tx = f::exit_block(&e.doc, e.sel);
            apply(cx, Some(tx))
        },
    ));
    r.register(FnCommand::new(
        params(
            spec(
                "app.setClipboard",
                Category::App,
                "ED-016",
                "Put text on the clipboard.",
            ),
            r#"{"type":"object","required":["text"],"properties":{"text":{"type":"string"}}}"#,
        ),
        |cx, p| {
            let text = p
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            cx.effects.push(Effect::SetClipboard { text, html: None });
            Ok(Outcome::Done)
        },
    ));
    r.register(FnCommand::new(
        params(
            spec(
                "app.openUrl",
                Category::App,
                "MD-033",
                "Open a link (internal anchors scroll, files open, URLs go to the browser).",
            ),
            r#"{"type":"object","required":["url"],"properties":{"url":{"type":"string"}}}"#,
        ),
        |cx, p| {
            let url = p
                .get("url")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            cx.effects.push(Effect::OpenUrl(url));
            Ok(Outcome::Done)
        },
    ));
}

fn heading_level(cx: &CommandContext<'_>) -> Option<u8> {
    let e = &cx.app.editor;
    crate::editor::context::path(&e.doc, e.sel.head)
        .iter()
        .rev()
        .find_map(|n| match n.block.kind {
            inkra_markdown::BlockKind::Heading { level, .. } => Some(level),
            _ => None,
        })
}

fn heading(cx: &mut CommandContext<'_>, level: u8) -> CommandResult {
    let e = &cx.app.editor;
    let tx = f::set_heading(&e.doc, e.sel, level);
    apply(cx, tx)
}

fn list(cx: &mut CommandContext<'_>, style: ListStyle) -> CommandResult {
    let b = bullet(cx);
    let e = &cx.app.editor;
    let tx = f::toggle_list(&e.doc, e.sel, style, b);
    apply(cx, tx)
}

/// Where an inserted image ends up (ED-092): pasted bitmaps live in a temporary
/// file and are always moved to the target folder; local files only when
/// `image.applyToLocal` is set.
fn place_image(
    cx: &mut CommandContext<'_>,
    src: &std::path::Path,
    pasted: bool,
) -> std::io::Result<std::path::PathBuf> {
    use inkra_workspace::assets::{self, InsertAction};
    let prefs = &cx.app.prefs;
    let action = InsertAction::from_pref(
        &prefs.string("image.insertAction"),
        &prefs.string("image.copyFolder"),
    );
    if !pasted && !prefs.bool("image.applyToLocal") {
        return Ok(src.to_owned());
    }
    let fallback = cx
        .app
        .paths
        .as_ref()
        .map(|p| p.data.join("images"))
        .unwrap_or_else(std::env::temp_dir);
    let action = if pasted && action == InsertAction::None {
        InsertAction::CopyToAssets
    } else {
        action
    };
    match assets::target_dir(cx.app.editor.path.as_deref(), &action, &fallback) {
        Some(dir) => {
            let placed = assets::copy_in(&dir, src)?;
            if pasted && placed != src {
                let _ = std::fs::remove_file(src);
            }
            Ok(placed)
        }
        None => Ok(src.to_owned()),
    }
}
