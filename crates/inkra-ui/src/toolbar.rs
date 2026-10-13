//! The optional formatting toolbar (FS-041): paragraph type, inline formats,
//! insertions. Buttons invoke commands; their state follows the caret.

use inkra_engine::Engine;
use inkra_i18n::I18n;
use serde_json::Value;

/// `(command, glyph)` of the buttons after the paragraph menu.
const BUTTONS: &[(&str, &str)] = &[
    ("format.bold", "B"),
    ("format.italic", "I"),
    ("format.underline", "U"),
    ("format.strike", "S"),
    ("format.code", "</>"),
    ("format.link", "🔗"),
    ("format.image", "🖼"),
    ("|", ""),
    ("paragraph.quote", "“"),
    ("paragraph.unorderedList", "•≡"),
    ("paragraph.orderedList", "1≡"),
    ("paragraph.taskList", "☑"),
    ("|", ""),
    ("paragraph.table", "▦"),
    ("paragraph.codeFences", "{ }"),
    ("paragraph.mathBlock", "∑"),
    ("paragraph.hr", "—"),
];

const PARAGRAPH: &[&str] = &[
    "paragraph.normal",
    "paragraph.heading1",
    "paragraph.heading2",
    "paragraph.heading3",
    "paragraph.heading4",
    "paragraph.heading5",
    "paragraph.heading6",
];

/// Draws the toolbar; returns the command to run.
pub fn show(ui: &mut egui::Ui, engine: &mut Engine, i18n: &I18n) -> Option<String> {
    let mut out = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        // The paragraph type at the caret.
        let current = PARAGRAPH
            .iter()
            .copied()
            .find(|id| engine.is_checked(id) == Some(true))
            .unwrap_or("paragraph.normal");
        let title = |id: &str| i18n.tr(&inkra_engine::command::title_key(id));
        egui::ComboBox::from_id_salt("toolbar-paragraph")
            .width(110.0)
            .selected_text(title(current))
            .show_ui(ui, |ui| {
                for id in PARAGRAPH {
                    if ui.selectable_label(*id == current, title(id)).clicked() && *id != current {
                        out = Some((*id).to_owned());
                    }
                }
            });
        ui.separator();
        for (id, glyph) in BUTTONS {
            if *id == "|" {
                ui.separator();
                continue;
            }
            let on = engine.is_checked(id) == Some(true);
            let mut text = egui::RichText::new(*glyph);
            text = match *id {
                "format.bold" => text.strong(),
                "format.italic" => text.italics(),
                "format.underline" => text.underline(),
                "format.strike" => text.strikethrough(),
                "format.code" => text.monospace(),
                _ => text,
            };
            let enabled = engine.is_enabled(id);
            let r = ui.add_enabled(
                enabled,
                egui::Button::new(text)
                    .selected(on)
                    .min_size(egui::vec2(26.0, 22.0)),
            );
            let tip = match engine.keymap.chord_for(id) {
                Some(c) => format!("{} ({})", title(id), c.label(inkra_engine::Os::current())),
                None => title(id),
            };
            if r.on_hover_text(tip).clicked() {
                out = Some((*id).to_owned());
            }
        }
    });
    let _ = Value::Null;
    out
}
