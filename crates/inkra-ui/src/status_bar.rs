//! Status bar: word count (FS-025), caret position and the source-mode toggle
//! (ED-120 `</>` button).

use inkra_engine::Engine;
use inkra_engine::editor::ViewMode;
use inkra_i18n::I18n;
use inkra_layout::Style;
use inkra_text::count::Counts;

/// Cached statistics (recomputed when the document changes).
#[derive(Debug, Default)]
pub struct StatusCache {
    version: Option<u64>,
    counts: Counts,
    computed_at: f64,
}

/// Draws the status bar; returns a command to run, if clicked.
pub fn show(
    ui: &mut egui::Ui,
    cache: &mut StatusCache,
    engine: &Engine,
    i18n: &I18n,
    style: &Style,
) -> Option<String> {
    let e = &engine.state.editor;
    if cache.version != Some(e.doc.version()) {
        // Counting is O(document); for large documents refresh at most every 300 ms.
        let now = ui.input(|i| i.time);
        let large = e.doc.text().len() > 64 * 1024;
        if !large || now - cache.computed_at > 0.3 || cache.version.is_none() {
            cache.counts = Counts::of(&e.doc.text().to_string());
            cache.version = Some(e.doc.version());
            cache.computed_at = now;
        } else {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(320));
        }
    }
    let mut cmd = None;
    ui.horizontal(|ui| {
        ui.style_mut().visuals.override_text_color = Some(style.fg_muted);
        let source = e.mode == ViewMode::Source;
        let label = egui::RichText::new("</>").monospace().color(if source {
            style.accent
        } else {
            style.fg_muted
        });
        if ui
            .add(egui::Button::new(label).frame(false))
            .on_hover_text(i18n.tr("cmd-view-sourceMode"))
            .clicked()
        {
            cmd = Some("view.sourceMode".to_owned());
        }
        let words = if e.sel.is_empty() {
            cache.counts.words
        } else {
            Counts::of(&e.doc.text().slice(e.sel.range())).words
        };
        ui.label(i18n.tr_args("status-words", &[("count", (words as i64).into())]));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let t = e.doc.text();
            let (line, col) = t.byte_to_line_col(e.sel.head);
            let col_chars = t.slice(t.line_to_byte(line)..e.sel.head).chars().count();
            let _ = col;
            ui.label(i18n.tr_args(
                "status-line-col",
                &[
                    ("line", ((line + 1) as i64).into()),
                    ("col", ((col_chars + 1) as i64).into()),
                ],
            ));
            ui.label(format!(
                "{} · {}",
                e.doc.encoding.label(),
                e.doc.line_ending.label()
            ));
        });
    });
    cmd
}
