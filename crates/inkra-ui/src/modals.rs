//! Modal dialogs, banners and toasts (UI-040, UI-041).

use std::time::Duration;

use inkra_engine::state::ExternalChange;
use serde_json::{Value, json};

use crate::{Confirm, ICON_PNG, Shell};

/// Save / Don't Save / Cancel when closing a modified window (FS-005).
pub(crate) fn confirm_close(shell: &mut Shell, ctx: &egui::Context, window: u64) {
    let Some(Confirm { id, quit }) = shell.confirm else {
        return;
    };
    if id != window {
        return;
    }
    let i18n = shell.i18n.clone();
    let name = shell
        .title_of(&shell.engine.state.editor)
        .trim_start_matches("● ")
        .to_owned();
    let mut choice = None;
    let resp = egui::Modal::new(egui::Id::new(("inkra-confirm-close", id))).show(ctx, |ui| {
        ui.set_max_width(420.0);
        ui.heading(i18n.tr_args("confirm-save-title", &[("name", name.clone().into())]));
        ui.label(i18n.tr("confirm-save-body"));
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button(i18n.tr("confirm-save")).clicked()
                || ui.input(|i| i.key_pressed(egui::Key::Enter))
            {
                choice = Some(0);
            }
            if ui.button(i18n.tr("confirm-dont-save")).clicked() {
                choice = Some(1);
            }
            if ui.button(i18n.tr("dialog-cancel")).clicked() {
                choice = Some(2);
            }
        });
    });
    if resp.should_close() && choice.is_none() {
        choice = Some(2);
    }
    let Some(c) = choice else { return };
    shell.confirm = None;
    match c {
        0 => {
            if shell.engine.state.editor.path.is_some() {
                shell.invoke(ctx, "file.save", &Value::Null);
                if !shell.engine.state.editor.doc.is_dirty() {
                    shell.close_window(ctx, id);
                    if quit {
                        shell.continue_quit();
                    }
                } else if quit {
                    shell.quitting = false;
                }
            } else {
                shell.apply_effects(
                    ctx,
                    Some(vec![inkra_engine::Effect::SaveAsDialog {
                        close_after: true,
                        quit,
                    }]),
                );
            }
        }
        1 => {
            // Discard: drop the changes and the recovery entry.
            shell.engine.state.editor.doc.mark_saved();
            shell.close_window(ctx, id);
            if quit {
                shell.continue_quit();
            }
        }
        _ => shell.quitting = false,
    }
}

/// Offers to restore documents from the recovery directory (FS-006).
pub(crate) fn recovery_offer(shell: &mut Shell, ctx: &egui::Context) {
    if shell.recovery_offer.is_empty() {
        return;
    }
    let i18n = shell.i18n.clone();
    let mut restore = None;
    let mut discard = None;
    let mut close = false;
    egui::Modal::new(egui::Id::new("inkra-recovery")).show(ctx, |ui| {
        ui.set_max_width(520.0);
        ui.heading(i18n.tr("recovery-title"));
        ui.label(i18n.tr("recovery-body"));
        ui.add_space(6.0);
        egui::Grid::new("recovery-grid")
            .striped(true)
            .show(ui, |ui| {
                for (k, m) in shell.recovery_offer.iter().enumerate() {
                    ui.label(&m.title);
                    ui.weak(
                        m.path
                            .as_ref()
                            .map(|p| p.display().to_string())
                            .unwrap_or_default(),
                    );
                    ui.weak(&m.time);
                    if ui.button(i18n.tr("recovery-restore")).clicked() {
                        restore = Some(k);
                    }
                    if ui.button(i18n.tr("recovery-discard")).clicked() {
                        discard = Some(k);
                    }
                    ui.end_row();
                }
            });
        ui.add_space(6.0);
        if ui.button(i18n.tr("dialog-close")).clicked() {
            close = true;
        }
    });
    let store = shell
        .engine
        .state
        .paths
        .as_ref()
        .map(|p| inkra_workspace::recovery::Recovery::new(p.recovery_dir()));
    if let Some(k) = restore {
        let m = shell.recovery_offer.remove(k);
        if let Some(text) = store.as_ref().and_then(|s| s.read(&m.id)) {
            let mut e = inkra_engine::editor::Editor::new(inkra_doc::Doc::from_str(
                &text,
                inkra_engine::markdown_options(&shell.engine.state.prefs),
            ));
            e.path = m.path.clone();
            e.doc.mark_dirty();
            if crate::inkra_engine_files_blank(&shell.engine.state) {
                shell.engine.state.editor = e;
            } else {
                let id = shell.engine.state.new_window(e);
                shell.apply_effects(ctx, Some(vec![inkra_engine::Effect::NewWindow(id)]));
            }
        }
        if let Some(s) = &store {
            s.remove(&m.id);
        }
    }
    if let Some(k) = discard {
        let m = shell.recovery_offer.remove(k);
        if let Some(s) = &store {
            s.remove(&m.id);
        }
    }
    if close {
        shell.recovery_offer.clear();
    }
}

/// "File changed on disk" banner (FS-007).
pub(crate) fn external_banner(shell: &mut Shell, ui: &mut egui::Ui) {
    let Some(kind) = shell.engine.state.meta.external else {
        return;
    };
    let i18n = shell.i18n.clone();
    let ctx = ui.ctx().clone();
    let mut cmd = None;
    egui::Frame::new()
        .fill(egui::Color32::from_rgb(0xff, 0xf5, 0xd6))
        .inner_margin(egui::Margin::symmetric(12, 6))
        .show(ui, |ui| {
            ui.horizontal(|ui| match kind {
                ExternalChange::Modified => {
                    ui.label(i18n.tr("external-modified"));
                    if ui.button(i18n.tr("cmd-file-reload")).clicked() {
                        cmd = Some("file.reload");
                    }
                    if ui.button(i18n.tr("cmd-file-keepLocal")).clicked() {
                        cmd = Some("file.keepLocal");
                    }
                }
                ExternalChange::Deleted => {
                    ui.label(i18n.tr("external-deleted"));
                    if ui.button(i18n.tr("dialog-ok")).clicked() {
                        cmd = Some("file.keepLocal");
                    }
                }
            });
        });
    if let Some(c) = cmd {
        if kind == ExternalChange::Deleted {
            shell.engine.state.meta.external = None;
            shell.engine.state.editor.doc.mark_dirty();
        } else {
            shell.invoke(&ctx, c, &Value::Null);
        }
    }
}

/// Insert-table dialog (ED-053).
pub(crate) fn table_dialog(shell: &mut Shell, ctx: &egui::Context) {
    let Some((mut rows, mut cols)) = shell.table_dialog else {
        return;
    };
    let mut done = None;
    let i18n = shell.i18n.clone();
    let resize = shell.table_dialog_resize;
    let resp = egui::Modal::new(egui::Id::new("inkra-table-dialog")).show(ctx, |ui| {
        ui.heading(i18n.tr(if resize {
            "table-resize-title"
        } else {
            "cmd-paragraph-table"
        }));
        egui::Grid::new("table-grid").show(ui, |ui| {
            ui.label(i18n.tr("table-rows"));
            ui.add(egui::DragValue::new(&mut rows).range(1..=200));
            ui.end_row();
            ui.label(i18n.tr("table-cols"));
            ui.add(egui::DragValue::new(&mut cols).range(1..=30));
            ui.end_row();
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui.button(i18n.tr("dialog-ok")).clicked()
                || ui.input(|i| i.key_pressed(egui::Key::Enter))
            {
                done = Some(true);
            }
            if ui.button(i18n.tr("dialog-cancel")).clicked() {
                done = Some(false);
            }
        });
    });
    shell.table_dialog = Some((rows, cols));
    if resp.should_close() {
        done = Some(false);
    }
    match done {
        Some(true) => {
            shell.table_dialog = None;
            let id = if resize {
                "table.resize"
            } else {
                "paragraph.table"
            };
            shell.invoke(ctx, id, &json!({ "rows": rows, "cols": cols }));
        }
        Some(false) => shell.table_dialog = None,
        None => {}
    }
}

fn icon(shell: &mut Shell, ctx: &egui::Context) -> Option<egui::TextureHandle> {
    if shell.icon.is_none() {
        match image::load_from_memory(ICON_PNG) {
            Ok(img) => {
                let rgba = img.to_rgba8();
                let size = [rgba.width() as usize, rgba.height() as usize];
                let color = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
                shell.icon = Some(ctx.load_texture("inkra-icon", color, Default::default()));
            }
            Err(e) => log::warn!("cannot decode icon: {e}"),
        }
    }
    shell.icon.clone()
}

/// About dialog (UI-052).
pub(crate) fn about_dialog(shell: &mut Shell, ctx: &egui::Context) {
    if !shell.about_open {
        return;
    }
    let icon = icon(shell, ctx);
    let i18n = shell.i18n.clone();
    let response = egui::Modal::new(egui::Id::new("inkra-about")).show(ctx, |ui| {
        ui.set_width(320.0);
        ui.vertical_centered(|ui| {
            if let Some(icon) = &icon {
                ui.add(egui::Image::new(icon).fit_to_exact_size(egui::vec2(128.0, 128.0)));
            }
            ui.heading(i18n.tr("app-name"));
            ui.label(i18n.tr("app-name-zh"));
            ui.label(i18n.tr_args(
                "about-version",
                &[("version", env!("CARGO_PKG_VERSION").into())],
            ));
            ui.weak(i18n.tr("app-tagline"));
            ui.weak(i18n.tr("about-license"));
            ui.add_space(8.0);
            ui.button(i18n.tr("dialog-ok")).clicked()
        })
        .inner
    });
    if response.inner || response.should_close() {
        shell.about_open = false;
    }
}

/// Damaged settings notice (PR-003).
pub(crate) fn prefs_problem_dialog(shell: &mut Shell, ctx: &egui::Context) {
    let Some(text) = shell.prefs_problem.clone() else {
        return;
    };
    let ok = shell.i18n.tr("dialog-ok");
    let response = egui::Modal::new(egui::Id::new("inkra-prefs-problem")).show(ctx, |ui| {
        ui.set_max_width(420.0);
        ui.label(text.as_str());
        ui.add_space(8.0);
        ui.vertical_centered(|ui| ui.button(ok).clicked()).inner
    });
    if response.inner || response.should_close() {
        shell.prefs_problem = None;
    }
}

/// Toasts (UI-041), bottom right.
pub(crate) fn toasts(shell: &mut Shell, ctx: &egui::Context) {
    let now = ctx.input(|i| i.time);
    shell.toasts.retain(|t| t.until > now);
    if shell.toasts.is_empty() {
        return;
    }
    egui::Area::new(egui::Id::new("inkra-toasts"))
        .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-16.0, -32.0))
        .show(ctx, |ui| {
            for t in &shell.toasts {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_max_width(360.0);
                    ui.label(&t.text);
                });
            }
        });
    let next = shell
        .toasts
        .iter()
        .map(|t| t.until)
        .fold(f64::INFINITY, f64::min);
    ctx.request_repaint_after(Duration::from_secs_f64((next - now).max(0.0)));
}

/// Page setup form state (EX-002).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PageSetupForm {
    paper: String,
    landscape: bool,
    width: f64,
    height: f64,
    margins: [f64; 4],
    header: String,
    footer: String,
    background: bool,
}

impl PageSetupForm {
    pub(crate) fn from_prefs(p: &inkra_engine::Preferences) -> Self {
        Self {
            paper: p.string("export.pdf.paper"),
            landscape: p.string("export.pdf.orientation") == "landscape",
            width: p.f64("export.pdf.customWidth"),
            height: p.f64("export.pdf.customHeight"),
            margins: [
                p.f64("export.pdf.marginTop"),
                p.f64("export.pdf.marginRight"),
                p.f64("export.pdf.marginBottom"),
                p.f64("export.pdf.marginLeft"),
            ],
            header: p.string("export.pdf.header"),
            footer: p.string("export.pdf.footer"),
            background: p.bool("export.pdf.background"),
        }
    }
}

/// The page setup dialog: paper, orientation, margins, header and footer.
pub(crate) fn page_setup(shell: &mut Shell, ctx: &egui::Context) {
    let Some(mut f) = shell.page_setup.take() else {
        return;
    };
    let i18n = shell.i18n.clone();
    let mut done = None;
    let resp = egui::Modal::new(egui::Id::new("inkra-page-setup")).show(ctx, |ui| {
        ui.heading(i18n.tr("cmd-file-pageSetup"));
        ui.add_space(4.0);
        egui::Grid::new("page-setup")
            .num_columns(2)
            .spacing([12.0, 6.0])
            .show(ui, |ui| {
                ui.label(i18n.tr("page-paper"));
                egui::ComboBox::from_id_salt("paper")
                    .selected_text(f.paper.clone())
                    .show_ui(ui, |ui| {
                        for p in ["A4", "A3", "A5", "B5", "Letter", "Legal", "Custom"] {
                            ui.selectable_value(&mut f.paper, p.to_owned(), p);
                        }
                    });
                ui.end_row();
                if f.paper == "Custom" {
                    ui.label(i18n.tr("page-size-mm"));
                    ui.horizontal(|ui| {
                        ui.add(egui::DragValue::new(&mut f.width).range(50.0..=1000.0));
                        ui.label("×");
                        ui.add(egui::DragValue::new(&mut f.height).range(50.0..=1000.0));
                    });
                    ui.end_row();
                }
                ui.label(i18n.tr("page-orientation"));
                ui.horizontal(|ui| {
                    ui.radio_value(&mut f.landscape, false, i18n.tr("page-portrait"));
                    ui.radio_value(&mut f.landscape, true, i18n.tr("page-landscape"));
                });
                ui.end_row();
                for (k, key) in [
                    "page-margin-top",
                    "page-margin-right",
                    "page-margin-bottom",
                    "page-margin-left",
                ]
                .iter()
                .enumerate()
                {
                    ui.label(i18n.tr(key));
                    ui.add(
                        egui::DragValue::new(&mut f.margins[k])
                            .range(0.0..=100.0)
                            .suffix(" mm"),
                    );
                    ui.end_row();
                }
                ui.label(i18n.tr("page-header"));
                ui.text_edit_singleline(&mut f.header);
                ui.end_row();
                ui.label(i18n.tr("page-footer"));
                ui.text_edit_singleline(&mut f.footer);
                ui.end_row();
                ui.label("");
                ui.checkbox(&mut f.background, i18n.tr("page-background"));
                ui.end_row();
            });
        ui.label(
            egui::RichText::new(i18n.tr("page-template-help"))
                .small()
                .weak(),
        );
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui.button(i18n.tr("dialog-ok")).clicked() {
                done = Some(true);
            }
            if ui.button(i18n.tr("dialog-cancel")).clicked() {
                done = Some(false);
            }
        });
    });
    if resp.should_close() && done.is_none() {
        done = Some(false);
    }
    match done {
        Some(true) => {
            let set = [
                ("export.pdf.paper", json!(f.paper)),
                (
                    "export.pdf.orientation",
                    json!(if f.landscape { "landscape" } else { "portrait" }),
                ),
                ("export.pdf.customWidth", json!(f.width)),
                ("export.pdf.customHeight", json!(f.height)),
                ("export.pdf.marginTop", json!(f.margins[0])),
                ("export.pdf.marginRight", json!(f.margins[1])),
                ("export.pdf.marginBottom", json!(f.margins[2])),
                ("export.pdf.marginLeft", json!(f.margins[3])),
                ("export.pdf.header", json!(f.header)),
                ("export.pdf.footer", json!(f.footer)),
                ("export.pdf.background", json!(f.background)),
            ];
            for (k, v) in set {
                shell.invoke(ctx, "app.setPreference", &json!({ "key": k, "value": v }));
            }
        }
        Some(false) => {}
        None => shell.page_setup = Some(f),
    }
}

/// Asks whether a dropped document should be imported (EX-021).
pub(crate) fn import_offer(shell: &mut Shell, ctx: &egui::Context) {
    let Some(path) = shell.import_offer.clone() else {
        return;
    };
    let i18n = shell.i18n.clone();
    let mut done = None;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let resp = egui::Modal::new(egui::Id::new("inkra-import-offer")).show(ctx, |ui| {
        ui.label(i18n.tr_args("import-offer", &[("name", name.into())]));
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui.button(i18n.tr("cmd-file-import")).clicked() {
                done = Some(true);
            }
            if ui.button(i18n.tr("dialog-cancel")).clicked() {
                done = Some(false);
            }
        });
    });
    if resp.should_close() && done.is_none() {
        done = Some(false);
    }
    if let Some(yes) = done {
        shell.import_offer = None;
        if yes {
            shell.invoke(ctx, "file.import", &json!({ "path": path }));
        }
    }
}

/// "A new version is available" (PR-020) with a link to the release page.
pub(crate) fn update_banner(shell: &mut Shell, ctx: &egui::Context) {
    let Some((version, url)) = shell.update_available.clone() else {
        return;
    };
    let i18n = shell.i18n.clone();
    let mut close = false;
    egui::Area::new(egui::Id::new("inkra-update"))
        .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-16.0, 36.0))
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.label(i18n.tr_args("update-available", &[("version", version.clone().into())]));
                ui.horizontal(|ui| {
                    if ui.button(i18n.tr("update-open")).clicked() {
                        crate::dialogs::open_external(&url);
                        close = true;
                    }
                    if ui.button(i18n.tr("update-later")).clicked() {
                        close = true;
                    }
                });
            });
        });
    if close {
        shell.update_available = None;
    }
}
