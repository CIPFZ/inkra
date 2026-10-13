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
    let mut action = None;
    let response = egui::Modal::new(egui::Id::new("inkra-about")).show(ctx, |ui| {
        ui.set_width(320.0);
        ui.vertical_centered(|ui| {
            // UI-052: the 256 px icon.
            if let Some(icon) = &icon {
                ui.add(egui::Image::new(icon).fit_to_exact_size(egui::vec2(256.0, 256.0)));
            }
            ui.heading(i18n.tr("app-name"));
            ui.label(i18n.tr("app-name-zh"));
            ui.label(i18n.tr_args(
                "about-version",
                &[("version", env!("CARGO_PKG_VERSION").into())],
            ));
            ui.weak(i18n.tr("app-tagline"));
            ui.weak(i18n.tr("about-license"));
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui.link(i18n.tr("cmd-help-thirdParty")).clicked() {
                    action = Some("help.thirdParty");
                }
                if ui.link(i18n.tr("cmd-help-checkUpdates")).clicked() {
                    action = Some("help.checkUpdates");
                }
            });
            ui.add_space(8.0);
            ui.button(i18n.tr("dialog-ok")).clicked()
        })
        .inner
    });
    if response.inner || response.should_close() || action.is_some() {
        shell.about_open = false;
    }
    if let Some(id) = action {
        shell.invoke(ctx, id, &Value::Null);
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
    let mut run = None;
    egui::Area::new(egui::Id::new("inkra-toasts"))
        .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-16.0, -32.0))
        .show(ctx, |ui| {
            for t in &mut shell.toasts {
                let r = egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_max_width(360.0);
                    ui.label(&t.text);
                    if let Some((label, cmd, params)) = &t.action
                        && ui.button(label.as_str()).clicked()
                    {
                        run = Some((cmd.clone(), params.clone()));
                        t.until = now;
                    }
                });
                if let Some(f) = &t.open {
                    let resp = r
                        .response
                        .interact(egui::Sense::click())
                        .on_hover_cursor(egui::CursorIcon::PointingHand);
                    if resp.clicked() {
                        crate::dialogs::open_external(&f.to_string_lossy());
                    }
                }
            }
        });
    if let Some((cmd, params)) = run {
        shell.invoke(ctx, &cmd, &params);
    }
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

/// Renaming the current file (FS-022): the name without the folder.
pub(crate) fn rename_dialog(shell: &mut Shell, ctx: &egui::Context) {
    let Some(mut name) = shell.rename.take() else {
        return;
    };
    let i18n = shell.i18n.clone();
    let mut done = None;
    let resp = egui::Modal::new(egui::Id::new("inkra-rename")).show(ctx, |ui| {
        ui.heading(i18n.tr("cmd-file-rename"));
        let r = ui.add(egui::TextEdit::singleline(&mut name).desired_width(320.0));
        if !r.has_focus() && !r.lost_focus() {
            r.request_focus();
        }
        let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui.button(i18n.tr("dialog-ok")).clicked() || enter {
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
        Some(true) if !name.trim().is_empty() => {
            shell.invoke(ctx, "file.rename", &json!({ "name": name.trim() }))
        }
        Some(_) => {}
        None => shell.rename = Some(name),
    }
}

/// Confirms moving the current file to the recycle bin.
pub(crate) fn delete_confirm(shell: &mut Shell, ctx: &egui::Context) {
    let Some(path) = shell.delete_confirm.clone() else {
        return;
    };
    let i18n = shell.i18n.clone();
    let mut done = None;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let resp = egui::Modal::new(egui::Id::new("inkra-delete")).show(ctx, |ui| {
        ui.label(i18n.tr_args("delete-confirm", &[("name", name.into())]));
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui.button(i18n.tr("cmd-file-delete")).clicked() {
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
        shell.delete_confirm = None;
        if yes {
            shell.invoke(ctx, "file.delete", &json!({ "confirmed": true }));
        }
    }
}

fn format_time(t: Option<std::time::SystemTime>) -> String {
    let Some(t) = t else { return "—".into() };
    let u = inkra_workspace::time::Utc::from_system(t);
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02} UTC",
        u.year, u.month, u.day, u.hour, u.minute
    )
}

/// Document properties (FS-009): path, size, dates, encoding, line endings and
/// counts; encoding and line endings can be changed (applied on save).
pub(crate) fn properties_dialog(shell: &mut Shell, ctx: &egui::Context) {
    if !shell.properties_open {
        return;
    }
    let i18n = shell.i18n.clone();
    let e = &shell.engine.state.editor;
    let path = e.path.clone();
    let meta = path.as_ref().and_then(|p| std::fs::metadata(p).ok());
    let text = e.doc.text().to_string();
    let counts = inkra_text::count::Counts::of(&text);
    let enc = e.doc.encoding;
    let bom = e.doc.bom;
    let crlf = e.doc.line_ending == inkra_text::LineEnding::Crlf;
    let mut close = false;
    let mut cmds: Vec<(&str, Value)> = Vec::new();
    let resp = egui::Modal::new(egui::Id::new("inkra-properties")).show(ctx, |ui| {
        ui.heading(i18n.tr("cmd-file-properties"));
        ui.add_space(4.0);
        egui::Grid::new("props")
            .num_columns(2)
            .spacing([16.0, 6.0])
            .show(ui, |ui| {
                ui.label(i18n.tr("props-path"));
                ui.label(
                    path.as_ref()
                        .map_or_else(|| i18n.tr("props-unsaved"), |p| p.display().to_string()),
                );
                ui.end_row();
                ui.label(i18n.tr("props-size"));
                ui.label(
                    meta.as_ref()
                        .map_or_else(|| "—".to_owned(), |m| format!("{} B", m.len())),
                );
                ui.end_row();
                ui.label(i18n.tr("props-created"));
                ui.label(format_time(meta.as_ref().and_then(|m| m.created().ok())));
                ui.end_row();
                ui.label(i18n.tr("props-modified"));
                ui.label(format_time(meta.as_ref().and_then(|m| m.modified().ok())));
                ui.end_row();
                ui.label(i18n.tr("props-encoding"));
                egui::ComboBox::from_id_salt("props-enc")
                    .selected_text(format!("{}{}", enc.label(), if bom { " BOM" } else { "" }))
                    .show_ui(ui, |ui| {
                        for (label, key, b) in [
                            ("UTF-8", "utf-8", false),
                            ("UTF-8 BOM", "utf-8", true),
                            ("UTF-16 LE", "utf-16le", true),
                            ("UTF-16 BE", "utf-16be", true),
                            ("GBK", "gbk", false),
                            ("GB18030", "gb18030", false),
                        ] {
                            if ui
                                .selectable_label(
                                    enc.label() == label.trim_end_matches(" BOM") && bom == b,
                                    label,
                                )
                                .clicked()
                            {
                                cmds.push((
                                    "file.setEncoding",
                                    json!({ "encoding": key, "bom": b }),
                                ));
                            }
                        }
                    });
                ui.end_row();
                ui.label(i18n.tr("props-line-endings"));
                ui.horizontal(|ui| {
                    if ui.radio(!crlf, "LF").clicked() && crlf {
                        cmds.push(("edit.lineEnding", json!({ "style": "lf" })));
                    }
                    if ui.radio(crlf, "CRLF").clicked() && !crlf {
                        cmds.push(("edit.lineEnding", json!({ "style": "crlf" })));
                    }
                });
                ui.end_row();
                ui.label(i18n.tr("count-words"));
                ui.label(counts.words.to_string());
                ui.end_row();
                ui.label(i18n.tr("count-chars"));
                ui.label(counts.chars.to_string());
                ui.end_row();
                ui.label(i18n.tr("count-lines"));
                ui.label(counts.lines.to_string());
                ui.end_row();
            });
        ui.label(egui::RichText::new(i18n.tr("props-on-save")).small().weak());
        ui.add_space(6.0);
        if ui.button(i18n.tr("dialog-ok")).clicked() {
            close = true;
        }
    });
    for (id, p) in cmds {
        shell.invoke(ctx, id, &p);
    }
    if close || resp.should_close() {
        shell.properties_open = false;
    }
}

/// Image properties (ED-093): alt text, title, width.
pub(crate) fn image_dialog(shell: &mut Shell, ctx: &egui::Context) {
    let Some((img, mut alt, mut title, mut width)) = shell.image_dialog.take() else {
        return;
    };
    let i18n = shell.i18n.clone();
    let natural = inkra_layout::live::leaf::ImageSizes::size(&shell.win().editor.images, &img.src);
    let mut done = None;
    let resp = egui::Modal::new(egui::Id::new("inkra-image-dialog")).show(ctx, |ui| {
        ui.heading(i18n.tr("cmd-format-imageOps"));
        ui.label(egui::RichText::new(&img.src).small().weak());
        egui::Grid::new("image-dialog")
            .num_columns(2)
            .spacing([12.0, 6.0])
            .show(ui, |ui| {
                ui.label(i18n.tr("image-dialog-alt"));
                ui.add(egui::TextEdit::singleline(&mut alt).desired_width(280.0));
                ui.end_row();
                ui.label(i18n.tr("image-dialog-title"));
                ui.add(egui::TextEdit::singleline(&mut title).desired_width(280.0));
                ui.end_row();
                ui.label(i18n.tr("image-dialog-width"));
                ui.horizontal(|ui| {
                    let mut natural_size = width.is_none();
                    if ui
                        .checkbox(&mut natural_size, i18n.tr("image-dialog-natural"))
                        .changed()
                    {
                        width = if natural_size {
                            None
                        } else {
                            Some(natural.map_or(300, |n| n[0].round() as u32))
                        };
                    }
                    if let Some(w) = &mut width {
                        ui.add(egui::DragValue::new(w).range(1..=10_000).suffix(" px"));
                    }
                });
                ui.end_row();
            });
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
            shell.engine.state.editor.sel = inkra_doc::Selection::caret(img.range.start + 1);
            shell.invoke(
                ctx,
                "format.imageOps",
                &json!({ "alt": alt, "title": title, "width": width }),
            );
        }
        Some(false) => {}
        None => shell.image_dialog = Some((img, alt, title, width)),
    }
}

/// The welcome panel (WM-005, off by default): the 64 px icon at the top left
/// (UI-052), recent files and a few entry points. Returns the chosen command.
pub(crate) fn welcome(shell: &mut Shell, ui: &mut egui::Ui) -> Option<(String, Value)> {
    let i18n = shell.i18n.clone();
    let tex = icon(shell, ui.ctx());
    let recent: Vec<std::path::PathBuf> = shell
        .engine
        .state
        .session
        .recent
        .iter()
        .take(8)
        .cloned()
        .collect();
    let mut out = None;
    egui::Frame::new()
        .inner_margin(egui::Margin::same(32))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                if let Some(t) = &tex {
                    ui.add(egui::Image::new(t).fit_to_exact_size(egui::vec2(64.0, 64.0)));
                }
                ui.vertical(|ui| {
                    ui.heading("Inkra");
                    ui.label(egui::RichText::new("墨刻").weak());
                });
            });
            ui.add_space(16.0);
            for (id, key) in [
                ("file.new", "cmd-file-new"),
                ("file.open", "cmd-file-open"),
                ("file.openFolder", "cmd-file-openFolder"),
                ("help.quickStart", "cmd-help-quickStart"),
            ] {
                if ui.link(i18n.tr(key)).clicked() {
                    out = Some((id.to_owned(), Value::Null));
                }
            }
            if !recent.is_empty() {
                ui.add_space(16.0);
                ui.strong(i18n.tr("menu-file-openRecent"));
                for p in &recent {
                    let name = p
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    if ui
                        .link(name)
                        .on_hover_text(p.display().to_string())
                        .clicked()
                    {
                        out = Some(("file.openRecent".to_owned(), json!({ "path": p })));
                    }
                }
            }
            ui.add_space(16.0);
            if ui.small_button(i18n.tr("welcome-start-writing")).clicked() {
                out = Some((String::new(), Value::Null));
            }
        });
    out
}

/// Link text and address (UI-040).
pub(crate) fn link_dialog(shell: &mut Shell, ctx: &egui::Context) {
    let Some((mut text, mut url)) = shell.link_dialog.take() else {
        return;
    };
    let i18n = shell.i18n.clone();
    let mut done = None;
    let resp = egui::Modal::new(egui::Id::new("inkra-link-dialog")).show(ctx, |ui| {
        ui.heading(i18n.tr("cmd-format-editLink"));
        egui::Grid::new("link-dialog")
            .num_columns(2)
            .spacing([12.0, 6.0])
            .show(ui, |ui| {
                ui.label(i18n.tr("link-dialog-text"));
                ui.add(egui::TextEdit::singleline(&mut text).desired_width(320.0));
                ui.end_row();
                ui.label(i18n.tr("link-dialog-url"));
                let r = ui.add(
                    egui::TextEdit::singleline(&mut url)
                        .desired_width(320.0)
                        .hint_text("https://"),
                );
                if url.is_empty() && !r.has_focus() && !r.lost_focus() {
                    r.request_focus();
                }
                if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    done = Some(true);
                }
                ui.end_row();
            });
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
        Some(true) => shell.invoke(
            ctx,
            "format.editLink",
            &json!({ "text": text, "url": url.trim() }),
        ),
        Some(false) => {}
        None => shell.link_dialog = Some((text, url)),
    }
}
