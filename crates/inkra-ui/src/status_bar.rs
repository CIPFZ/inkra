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
    images: usize,
    computed_at: f64,
}

fn count_images(doc: &inkra_doc::Doc) -> usize {
    let mut n = 0;
    for b in doc.ast().to_blocks() {
        b.walk(&mut |b| {
            for i in &b.inlines {
                i.walk(&mut |i| {
                    if matches!(i.kind, inkra_markdown::InlineKind::Image { .. }) {
                        n += 1;
                    }
                });
            }
        });
    }
    n
}

/// Reading time in minutes (FS-025): 275 words per minute plus 12 s per image.
pub fn reading_minutes(words: usize, images: usize) -> f64 {
    words as f64 / 275.0 + images as f64 * 12.0 / 60.0
}

fn details(ui: &mut egui::Ui, i18n: &I18n, c: &Counts, images: usize, selection: Option<&Counts>) {
    let row = |ui: &mut egui::Ui, key: &str, v: String| {
        ui.label(i18n.tr(key));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.monospace(v)
        });
        ui.end_row();
    };
    let grid = |ui: &mut egui::Ui, id: &str, c: &Counts, read: bool| {
        egui::Grid::new(id)
            .num_columns(2)
            .spacing([24.0, 2.0])
            .show(ui, |ui| {
                row(ui, "count-words", c.words.to_string());
                row(ui, "count-chars", c.chars.to_string());
                row(ui, "count-chars-no-space", c.chars_no_space.to_string());
                row(ui, "count-lines", c.lines.to_string());
                row(ui, "count-paragraphs", c.paragraphs.to_string());
                if read {
                    let m = reading_minutes(c.words, images).ceil().max(1.0) as i64;
                    row(
                        ui,
                        "count-reading",
                        i18n.tr_args("count-minutes", &[("n", m.into())]),
                    );
                }
            });
    };
    if let Some(sel) = selection {
        ui.strong(i18n.tr("count-selection"));
        grid(ui, "count-sel", sel, false);
        ui.separator();
        ui.strong(i18n.tr("count-document"));
    }
    grid(ui, "count-doc", c, true);
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
            cache.images = count_images(&e.doc);
            cache.version = Some(e.doc.version());
            cache.computed_at = now;
        } else {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(320));
        }
    }
    let mut cmd = None;
    ui.horizontal(|ui| {
        ui.style_mut().visuals.override_text_color = Some(style.statusbar_fg);
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
        // PR-043: the word count can be hidden.
        if engine.state.prefs.bool("view.wordCount") {
            let sel = (!e.sel.is_empty()).then(|| Counts::of(&e.doc.text().slice(e.sel.range())));
            let words = sel.as_ref().map_or(cache.counts.words, |c| c.words);
            // Hover shows the details; a click pins them (FS-025).
            let r = ui.add(
                egui::Label::new(i18n.tr_args("status-words", &[("count", (words as i64).into())]))
                    .sense(egui::Sense::click()),
            );
            let pinned_id = ui.id().with("count-pinned");
            let mut pinned = ui
                .ctx()
                .data(|d| d.get_temp::<bool>(pinned_id).unwrap_or(false));
            if r.clicked() {
                pinned = !pinned;
                ui.ctx().data_mut(|d| d.insert_temp(pinned_id, pinned));
            }
            if pinned || r.hovered() {
                let counts = cache.counts;
                let images = cache.images;
                egui::Area::new(pinned_id.with("popup"))
                    .order(egui::Order::Foreground)
                    .fixed_pos(egui::pos2(r.rect.left(), r.rect.top() - 6.0))
                    .pivot(egui::Align2::LEFT_BOTTOM)
                    .show(ui.ctx(), |ui| {
                        egui::Frame::popup(ui.style())
                            .show(ui, |ui| details(ui, i18n, &counts, images, sel.as_ref()));
                    });
                if pinned && ui.input(|i| i.pointer.any_pressed()) && !r.hovered() {
                    ui.ctx().data_mut(|d| d.insert_temp(pinned_id, false));
                }
            }
        }
        // Mode indicators (WM-001, WM-002).
        if engine.state.view.focus {
            ui.label(egui::RichText::new("F").strong())
                .on_hover_text(i18n.tr("cmd-view-focusMode"));
        }
        if engine.state.view.typewriter {
            ui.label(egui::RichText::new("T").strong())
                .on_hover_text(i18n.tr("cmd-view-typewriterMode"));
        }
        if (engine.state.view.zoom - 1.0).abs() > 1e-3 {
            ui.label(format!("{:.0}%", engine.state.view.zoom * 100.0));
        }
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
