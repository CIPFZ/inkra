//! Find and replace bar (ED-110, ED-111).

use inkra_doc::Selection;
use inkra_engine::Engine;
use inkra_i18n::I18n;

fn toggle(ui: &mut egui::Ui, on: &mut bool, label: &str, tip: &str) -> bool {
    let r = ui
        .add(egui::Button::selectable(
            *on,
            egui::RichText::new(label).monospace(),
        ))
        .on_hover_text(tip);
    if r.clicked() {
        *on = !*on;
        true
    } else {
        false
    }
}

/// Draws the find bar at the top of `ui`; returns commands to run.
pub fn show(
    ui: &mut egui::Ui,
    engine: &mut Engine,
    i18n: &I18n,
    focus: &mut bool,
    editor_id: egui::Id,
) -> Vec<String> {
    let mut cmds = Vec::new();
    let frame = egui::Frame::new().inner_margin(egui::Margin::symmetric(10, 6));
    frame.show(ui, |ui| {
        let e = &mut engine.state.editor;
        ui.horizontal(|ui| {
            let before = (e.find.query.clone(), e.find.options);
            let edit = ui.add(
                egui::TextEdit::singleline(&mut e.find.query)
                    .hint_text(i18n.tr("find-placeholder"))
                    .desired_width(260.0)
                    .id(egui::Id::new("inkra-find-query")),
            );
            if *focus {
                edit.request_focus();
                *focus = false;
            }
            let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let shift = ui.input(|i| i.modifiers.shift);
            ui.label(e.find.counter());
            let mut o = e.find.options;
            toggle(ui, &mut o.case_sensitive, "Aa", &i18n.tr("find-case"));
            toggle(ui, &mut o.whole_word, "W", &i18n.tr("find-word"));
            toggle(ui, &mut o.regex, ".*", &i18n.tr("find-regex"));
            e.find.options = o;
            if ui.button("↑").on_hover_text(i18n.tr("find-prev")).clicked() {
                cmds.push("edit.findPrev".to_owned());
            }
            if ui.button("↓").on_hover_text(i18n.tr("find-next")).clicked() {
                cmds.push("edit.findNext".to_owned());
            }
            if ui
                .button("⇄")
                .on_hover_text(i18n.tr("find-toggle-replace"))
                .clicked()
            {
                e.find.replace_open = !e.find.replace_open;
            }
            if ui
                .button("✕")
                .on_hover_text(i18n.tr("find-close"))
                .clicked()
                || edit.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape))
            {
                e.find.open = false;
                ui.memory_mut(|m| m.request_focus(editor_id));
            }
            if (e.find.query.clone(), e.find.options) != before {
                e.find.invalidate();
                e.refresh_find(e.sel.start());
                if let Some(i) = e.find.current {
                    let r = e.find.matches[i].clone();
                    e.sel = Selection::new(r.start, r.end);
                    cmds.push("edit.jumpSelection".to_owned());
                }
            } else {
                e.refresh_find(e.sel.start());
            }
            if enter {
                cmds.push(
                    if shift {
                        "edit.findPrev"
                    } else {
                        "edit.findNext"
                    }
                    .to_owned(),
                );
                edit.request_focus();
            }
            if let Some(err) = &e.find.error {
                ui.colored_label(
                    egui::Color32::from_rgb(0xd7, 0x3a, 0x49),
                    err.lines().last().unwrap_or_default(),
                );
            }
        });
        if e.find.replace_open {
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut e.find.replacement)
                        .hint_text(i18n.tr("replace-placeholder"))
                        .desired_width(260.0),
                );
                if ui.button(i18n.tr("find-replace-one")).clicked() {
                    cmds.push("edit.replaceCurrent".to_owned());
                }
                if ui.button(i18n.tr("find-replace-all")).clicked() {
                    cmds.push("edit.replaceAll".to_owned());
                }
            });
        }
    });
    ui.separator();
    cmds
}
