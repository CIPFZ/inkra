//! The editor's right-click menu (ED-130): contents depend on what is under the
//! caret (link, task item, code block) plus the general editing entries.

use inkra_engine::Engine;
use inkra_engine::editor::{context, format_ops, ops};
use inkra_i18n::I18n;
use serde_json::{Value, json};

fn cmd_button(
    ui: &mut egui::Ui,
    engine: &mut Engine,
    i18n: &I18n,
    id: &str,
    out: &mut Vec<(String, Value)>,
) {
    let label = i18n.tr(&inkra_engine::command::title_key(id));
    let shortcut = engine
        .keymap
        .chord_for(id)
        .map(|c| c.label(inkra_engine::keymap::Os::current()))
        .unwrap_or_default();
    let enabled = engine.is_enabled(id);
    if ui
        .add_enabled(enabled, egui::Button::new(label).shortcut_text(shortcut))
        .clicked()
    {
        out.push((id.to_owned(), Value::Null));
        ui.close();
    }
}

fn custom(
    ui: &mut egui::Ui,
    label: String,
    id: &str,
    params: Value,
    out: &mut Vec<(String, Value)>,
) {
    if ui.button(label).clicked() {
        out.push((id.to_owned(), params));
        ui.close();
    }
}

/// Fills the context menu; returns commands to run.
pub fn show(ui: &mut egui::Ui, engine: &mut Engine, i18n: &I18n) -> Vec<(String, Value)> {
    let mut out = Vec::new();
    let e = &engine.state.editor;
    let pos = e.sel.head;
    let link = ops::link_at(&e.doc, pos);
    let task = context::list_item(&e.doc, pos).and_then(|i| i.task);
    let code = context::code_block(&e.doc, pos).filter(|_| context::in_code_content(&e.doc, pos));
    let code_text = code.as_ref().and_then(|_| {
        let p = context::path(&e.doc, pos);
        p.iter().rev().find_map(|n| match &n.block.kind {
            inkra_markdown::BlockKind::CodeBlock { .. } => {
                let (a, z) = (n.block.lines.first()?, n.block.lines.last()?);
                Some(
                    e.doc
                        .text()
                        .slice(n.abs(a.span.start)..n.abs(z.span.end))
                        .into_owned(),
                )
            }
            _ => None,
        })
    });
    let has_link = format_ops::link_ranges(&e.doc, pos).is_some();

    // Context-specific entries first.
    if let Some(url) = &link {
        custom(
            ui,
            i18n.tr("ctx-open-link"),
            "app.openUrl",
            json!({ "url": url }),
            &mut out,
        );
        custom(
            ui,
            i18n.tr("ctx-copy-link"),
            "app.setClipboard",
            json!({ "text": url }),
            &mut out,
        );
        if has_link {
            cmd_button(ui, engine, i18n, "format.removeLink", &mut out);
        }
        ui.separator();
    }
    if task.is_some() {
        cmd_button(ui, engine, i18n, "paragraph.taskStatus", &mut out);
        ui.separator();
    }
    if let Some(code) = code_text {
        custom(
            ui,
            i18n.tr("ctx-copy-code"),
            "app.setClipboard",
            json!({ "text": code }),
            &mut out,
        );
        ui.separator();
    }

    for id in ["edit.cut", "edit.copy", "edit.paste", "edit.pastePlain"] {
        cmd_button(ui, engine, i18n, id, &mut out);
    }
    ui.menu_button(i18n.tr("menu-edit-copyAs"), |ui| {
        for id in [
            "edit.copyAsMarkdown",
            "edit.copyAsHtml",
            "edit.copyAsPlain",
            "edit.copyAsHtmlCode",
        ] {
            cmd_button(ui, engine, i18n, id, &mut out);
        }
    });
    cmd_button(ui, engine, i18n, "edit.selectAll", &mut out);
    ui.separator();
    ui.menu_button(i18n.tr("ctx-insert"), |ui| {
        for id in [
            "format.image",
            "format.link",
            "paragraph.table",
            "paragraph.codeFences",
            "paragraph.mathBlock",
            "paragraph.hr",
            "paragraph.toc",
        ] {
            cmd_button(ui, engine, i18n, id, &mut out);
        }
    });
    ui.menu_button(i18n.tr("ctx-paragraph"), |ui| {
        for id in [
            "paragraph.heading1",
            "paragraph.heading2",
            "paragraph.heading3",
            "paragraph.heading4",
            "paragraph.heading5",
            "paragraph.heading6",
            "paragraph.normal",
            "paragraph.quote",
            "paragraph.codeFences",
        ] {
            cmd_button(ui, engine, i18n, id, &mut out);
        }
    });
    ui.menu_button(i18n.tr("ctx-list"), |ui| {
        for id in [
            "paragraph.unorderedList",
            "paragraph.orderedList",
            "paragraph.taskList",
        ] {
            cmd_button(ui, engine, i18n, id, &mut out);
        }
    });
    out
}
