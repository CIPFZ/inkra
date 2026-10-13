//! The Preferences window (SPEC chapter 9): categories on the left, a form on
//! the right. Every control writes through `app.setPreference`, so changes
//! apply immediately and are saved.

use inkra_engine::Engine;
use inkra_engine::prefs::{PrefKind, spec};
use inkra_i18n::I18n;
use serde_json::{Value, json};

/// A page of the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    /// General.
    General,
    /// Appearance.
    Appearance,
    /// Editor.
    Editor,
    /// Images.
    Image,
    /// Markdown.
    Markdown,
    /// Files and sidebar.
    Files,
    /// Export.
    Export,
    /// Keyboard shortcuts.
    Keymap,
    /// Advanced.
    Advanced,
}

const PAGES: &[(Page, &str)] = &[
    (Page::General, "general"),
    (Page::Appearance, "appearance"),
    (Page::Editor, "editor"),
    (Page::Image, "image"),
    (Page::Markdown, "markdown"),
    (Page::Files, "files"),
    (Page::Export, "export"),
    (Page::Keymap, "keymap"),
    (Page::Advanced, "advanced"),
];

/// Keys shown on each page, in order (`None` = a separator).
fn keys(page: Page) -> &'static [Option<&'static str>] {
    match page {
        Page::General => &[
            Some("general.language"),
            Some("general.startup"),
            Some("general.openFileIn"),
            None,
            Some("general.autoSave"),
            Some("general.autoReload"),
            Some("general.trimTrailingOnSave"),
            Some("general.normalizeOnSave"),
            None,
            Some("general.defaultExtension"),
            Some("general.defaultEncoding"),
            Some("general.defaultNewline"),
            None,
            Some("general.checkUpdates"),
        ],
        Page::Appearance => &[
            Some("appearance.theme"),
            Some("appearance.themeAuto"),
            Some("appearance.lightTheme"),
            Some("appearance.darkTheme"),
            None,
            Some("view.statusBar"),
            Some("view.wordCount"),
            Some("view.menuBar"),
            Some("appearance.rememberWindowSize"),
            None,
            Some("sidebar.fontSize"),
            Some("sidebar.lineHeight"),
        ],
        Page::Editor => &[
            Some("editor.defaultMode"),
            None,
            Some("editor.autoPairBrackets"),
            Some("editor.autoPairMarkdown"),
            Some("markdown.smartPunctuation"),
            Some("markdown.smartQuotes"),
            Some("markdown.smartDashes"),
            Some("markdown.smartEllipsis"),
            None,
            Some("editor.spellcheck"),
            Some("editor.spellcheckDictionary"),
            Some("editor.emojiCompletion"),
            Some("editor.emojiAsShortcode"),
            Some("editor.linkCompletion"),
            None,
            Some("editor.firstLineIndent"),
            Some("editor.copyAs"),
            Some("editor.linkClick"),
            Some("editor.focusMode"),
            Some("editor.typewriterMode"),
            Some("editor.ctrlWheelZoom"),
            Some("editor.rememberZoom"),
            Some("editor.tabWidth"),
            Some("editor.useSpaces"),
            Some("editor.cursorBlink"),
            Some("editor.smoothScroll"),
        ],
        Page::Image => &[
            Some("image.insertAction"),
            Some("image.copyFolder"),
            Some("image.applyToLocal"),
            Some("image.preferRelative"),
            None,
            Some("image.maxWidthFollowsEditor"),
            Some("image.showTitle"),
            Some("image.loadRemote"),
        ],
        Page::Markdown => &[
            Some("markdown.inlineMath"),
            Some("markdown.subscript"),
            Some("markdown.superscript"),
            Some("markdown.highlight"),
            Some("markdown.diagrams"),
            Some("markdown.mermaidCli"),
            Some("markdown.strictMode"),
            None,
            Some("markdown.mathNumbering"),
            Some("markdown.latexDelimiters"),
            Some("markdown.looseInlineMath"),
            None,
            Some("markdown.codeLineNumbers"),
            Some("markdown.codeWrap"),
            Some("markdown.codeDefaultLanguage"),
            None,
            Some("markdown.preserveBlankLines"),
            Some("markdown.strongMarker"),
            Some("markdown.emphasisMarker"),
            Some("markdown.bulletMarker"),
            Some("markdown.renumberLists"),
            Some("markdown.listFromOne"),
            Some("markdown.tableAlign"),
            Some("markdown.tocDepth"),
        ],
        Page::Files => &[
            Some("general.markdownExtensions"),
            None,
            Some("sidebar.outlineDepth"),
            Some("sidebar.articlesRecursive"),
            Some("sidebar.articlesSort"),
            Some("sidebar.articlesSortDesc"),
            Some("sidebar.showHidden"),
            Some("sidebar.showAllFiles"),
            Some("sidebar.fileFilter"),
            Some("sidebar.revealCurrent"),
            Some("search.gitignore"),
        ],
        Page::Export => &[
            Some("export.theme"),
            Some("export.openAfter"),
            None,
            Some("export.pdf.paper"),
            Some("export.pdf.customWidth"),
            Some("export.pdf.customHeight"),
            Some("export.pdf.orientation"),
            Some("export.pdf.marginTop"),
            Some("export.pdf.marginRight"),
            Some("export.pdf.marginBottom"),
            Some("export.pdf.marginLeft"),
            Some("export.pdf.header"),
            Some("export.pdf.footer"),
            Some("export.pdf.background"),
            None,
            Some("export.html.images"),
            Some("export.html.math"),
            Some("export.html.highlight"),
            Some("export.html.softBreakAsBr"),
            Some("export.html.head"),
            Some("export.html.foot"),
            Some("export.html.css"),
            None,
            Some("export.image.scale"),
            Some("export.image.paged"),
            None,
            Some("export.pandocPath"),
            Some("export.pandocArgs"),
            Some("export.custom"),
        ],
        Page::Keymap | Page::Advanced => &[],
    }
}

/// Number ranges (min, max, step) of numeric keys.
fn range(key: &str) -> (f64, f64, f64) {
    match key {
        "sidebar.fontSize" => (9.0, 24.0, 1.0),
        "sidebar.lineHeight" => (1.0, 2.5, 0.1),
        "editor.tabWidth" => (1.0, 8.0, 1.0),
        "editor.zoom" => (0.5, 3.0, 0.1),
        "sidebar.outlineDepth" | "markdown.tocDepth" => (1.0, 6.0, 1.0),
        "export.image.scale" => (1.0, 3.0, 1.0),
        "export.pdf.customWidth" | "export.pdf.customHeight" => (50.0, 1000.0, 1.0),
        k if k.starts_with("export.pdf.margin") => (0.0, 100.0, 1.0),
        _ => (0.0, 10_000.0, 1.0),
    }
}

/// Keys whose effect needs a restart.
const RESTART: &[&str] = &["general.language", "appearance.rememberWindowSize"];

/// Keys with a multi-line editor.
const MULTILINE: &[&str] = &["export.html.head", "export.html.foot", "export.html.css"];

/// Keymap editor state.
#[derive(Debug, Default)]
struct KeymapEdit {
    filter: String,
    /// Recording a chord for this command.
    recording: Option<String>,
}

/// The window's state.
#[derive(Debug)]
pub struct PrefsWindow {
    /// Shown.
    pub open: bool,
    page: Page,
    filter: String,
    keymap: KeymapEdit,
    confirm_reset: bool,
    /// Text being edited for string-list keys, by key.
    lists: std::collections::HashMap<String, String>,
}

impl Default for PrefsWindow {
    fn default() -> Self {
        Self {
            open: false,
            page: Page::General,
            filter: String::new(),
            keymap: KeymapEdit::default(),
            confirm_reset: false,
            lists: Default::default(),
        }
    }
}

/// What the window asks the shell to do.
#[derive(Debug, Clone, PartialEq)]
pub enum Request {
    /// Run a command.
    Invoke(String, Value),
    /// The keymap changed: save `keybindings.json`.
    SaveKeymap,
}

fn label(i18n: &I18n, key: &str) -> String {
    let id = format!("pref-{}", key.replace('.', "-"));
    if i18n.has(&id) {
        i18n.tr(&id)
    } else {
        key.to_owned()
    }
}

fn value_label(i18n: &I18n, key: &str, v: &str) -> String {
    let id = format!("pref-{}--{v}", key.replace('.', "-"));
    if i18n.has(&id) {
        i18n.tr(&id)
    } else {
        v.to_owned()
    }
}

fn set(out: &mut Vec<Request>, key: &str, value: Value) {
    out.push(Request::Invoke(
        "app.setPreference".into(),
        json!({ "key": key, "value": value }),
    ));
}

impl PrefsWindow {
    /// Whether Escape may close the window (not while recording a shortcut).
    pub fn idle(&self) -> bool {
        self.keymap.recording.is_none()
    }

    /// Opens the window.
    pub fn show_page(&mut self, page: Page) {
        self.open = true;
        self.page = page;
    }

    /// Draws the window's content into `ui` (its own viewport or an embedded
    /// window). `themes` are the installed theme names.
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        engine: &mut Engine,
        i18n: &I18n,
        themes: &[String],
        os: inkra_engine::Os,
    ) -> Vec<Request> {
        let mut out = Vec::new();
        egui::Panel::left("prefs-pages")
            .resizable(false)
            .exact_size(170.0)
            .show(ui, |ui| {
                ui.add_space(8.0);
                ui.add(
                    egui::TextEdit::singleline(&mut self.filter).hint_text(i18n.tr("prefs-search")),
                );
                ui.add_space(6.0);
                for (p, name) in PAGES {
                    if ui
                        .selectable_label(
                            self.page == *p && self.filter.is_empty(),
                            i18n.tr(&format!("prefcat-{name}")),
                        )
                        .clicked()
                    {
                        self.page = *p;
                        self.filter.clear();
                    }
                }
            });
        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if !self.filter.trim().is_empty() {
                        let f = self.filter.trim().to_lowercase();
                        let hits: Vec<&str> = PAGES
                            .iter()
                            .flat_map(|(p, _)| keys(*p).iter().flatten().copied())
                            .filter(|k| {
                                k.to_lowercase().contains(&f)
                                    || label(i18n, k).to_lowercase().contains(&f)
                            })
                            .collect();
                        self.form(
                            ui,
                            engine,
                            i18n,
                            themes,
                            hits.into_iter().map(Some),
                            &mut out,
                        );
                        return;
                    }
                    ui.heading(i18n.tr(&format!(
                            "prefcat-{}",
                            PAGES
                                .iter()
                                .find(|(p, _)| *p == self.page)
                                .map_or("general", |p| p.1)
                        )));
                    ui.add_space(6.0);
                    match self.page {
                        Page::Keymap => self.keymap_page(ui, engine, i18n, os, &mut out),
                        Page::Advanced => self.advanced_page(ui, i18n, &mut out),
                        p => self.form(ui, engine, i18n, themes, keys(p).iter().copied(), &mut out),
                    }
                });
        });
        out
    }

    fn form<'k>(
        &mut self,
        ui: &mut egui::Ui,
        engine: &Engine,
        i18n: &I18n,
        themes: &[String],
        list: impl Iterator<Item = Option<&'k str>>,
        out: &mut Vec<Request>,
    ) {
        let prefs = &engine.state.prefs;
        egui::Grid::new("prefs-form")
            .num_columns(2)
            .spacing([16.0, 8.0])
            .striped(false)
            .show(ui, |ui| {
                for item in list {
                    let Some(key) = item else {
                        ui.separator();
                        ui.separator();
                        ui.end_row();
                        continue;
                    };
                    let Some(s) = spec(key) else { continue };
                    let text = label(i18n, key);
                    let help = s.doc;
                    match s.kind {
                        PrefKind::Bool => {
                            ui.label("");
                            let mut v = prefs.bool(key);
                            if ui.checkbox(&mut v, text).on_hover_text(help).changed() {
                                set(out, key, json!(v));
                            }
                        }
                        PrefKind::Number => {
                            ui.label(text).on_hover_text(help);
                            let (lo, hi, step) = range(key);
                            let mut v = prefs.f64(key);
                            let r = ui.add(
                                egui::DragValue::new(&mut v)
                                    .range(lo..=hi)
                                    .speed(step)
                                    .fixed_decimals(if step < 1.0 { 1 } else { 0 }),
                            );
                            if r.changed() {
                                set(out, key, json!(v));
                            }
                        }
                        PrefKind::Enum(values) => {
                            ui.label(text).on_hover_text(help);
                            let cur = prefs.string(key);
                            egui::ComboBox::from_id_salt(key)
                                .selected_text(value_label(i18n, key, &cur))
                                .show_ui(ui, |ui| {
                                    for v in values {
                                        if ui
                                            .selectable_label(cur == *v, value_label(i18n, key, v))
                                            .clicked()
                                            && cur != *v
                                        {
                                            set(out, key, json!(v));
                                        }
                                    }
                                });
                        }
                        PrefKind::String
                            if key.ends_with("Theme")
                                || key == "appearance.theme"
                                || key == "export.theme" =>
                        {
                            ui.label(text).on_hover_text(help);
                            let cur = prefs.string(key);
                            let none = key == "export.theme";
                            let shown = if cur.is_empty() && none {
                                i18n.tr("prefs-current-theme")
                            } else {
                                cur.clone()
                            };
                            egui::ComboBox::from_id_salt(key)
                                .selected_text(shown)
                                .show_ui(ui, |ui| {
                                    if none
                                        && ui
                                            .selectable_label(
                                                cur.is_empty(),
                                                i18n.tr("prefs-current-theme"),
                                            )
                                            .clicked()
                                    {
                                        set(out, key, json!(""));
                                    }
                                    for t in themes {
                                        if ui.selectable_label(cur == *t, t).clicked() && cur != *t
                                        {
                                            set(out, key, json!(t));
                                        }
                                    }
                                });
                        }
                        PrefKind::String if key == "general.language" => {
                            ui.label(text).on_hover_text(help);
                            let cur = prefs.string(key);
                            let name = |v: &str| match v {
                                "zh-CN" => "简体中文".to_owned(),
                                "en-US" => "English".to_owned(),
                                _ => i18n.tr("prefs-system"),
                            };
                            egui::ComboBox::from_id_salt(key)
                                .selected_text(name(&cur))
                                .show_ui(ui, |ui| {
                                    for v in ["system", "zh-CN", "en-US"] {
                                        if ui.selectable_label(cur == v, name(v)).clicked()
                                            && cur != v
                                        {
                                            set(out, key, json!(v));
                                        }
                                    }
                                });
                        }
                        PrefKind::String => {
                            ui.label(text).on_hover_text(help);
                            let mut v = prefs.string(key);
                            let r = if MULTILINE.contains(&key) {
                                ui.add(
                                    egui::TextEdit::multiline(&mut v)
                                        .desired_rows(3)
                                        .desired_width(320.0)
                                        .code_editor(),
                                )
                            } else {
                                ui.add(egui::TextEdit::singleline(&mut v).desired_width(320.0))
                            };
                            if r.changed() {
                                set(out, key, json!(v));
                            }
                        }
                        PrefKind::StringList => {
                            ui.label(text).on_hover_text(help);
                            let joined = prefs.strings(key).join("\n");
                            let buf = self
                                .lists
                                .entry(key.to_owned())
                                .or_insert_with(|| joined.clone());
                            let r = ui.add(
                                egui::TextEdit::multiline(buf)
                                    .desired_rows(3)
                                    .desired_width(320.0)
                                    .hint_text(i18n.tr("prefs-one-per-line")),
                            );
                            if r.lost_focus() || (r.changed() && !r.has_focus()) {
                                let items: Vec<String> = buf
                                    .lines()
                                    .map(str::trim)
                                    .filter(|l| !l.is_empty())
                                    .map(str::to_owned)
                                    .collect();
                                set(out, key, json!(items));
                            }
                            if !r.has_focus() && *buf != joined && !r.lost_focus() {
                                *buf = joined;
                            }
                        }
                    }
                    ui.end_row();
                    if RESTART.contains(&key) {
                        ui.label("");
                        ui.label(egui::RichText::new(i18n.tr("prefs-restart")).small().weak());
                        ui.end_row();
                    }
                }
            });
    }

    fn keymap_page(
        &mut self,
        ui: &mut egui::Ui,
        engine: &mut Engine,
        i18n: &I18n,
        os: inkra_engine::Os,
        out: &mut Vec<Request>,
    ) {
        // Recording: the next key press with a key becomes the chord.
        if let Some(cmd) = self.keymap.recording.clone() {
            let pressed = ui.input(|i| {
                i.events.iter().find_map(|e| match e {
                    egui::Event::Key {
                        key,
                        physical_key,
                        pressed: true,
                        modifiers,
                        ..
                    } => Some((*key, *physical_key, *modifiers)),
                    _ => None,
                })
            });
            if let Some((key, phys, mods)) = pressed {
                if key == egui::Key::Escape && !mods.any() {
                    self.keymap.recording = None;
                } else if let Some(chord) = crate::keys::chord(key, phys, mods, os) {
                    engine.keymap.bind(chord, &cmd, Value::Null);
                    self.keymap.recording = None;
                    out.push(Request::SaveKeymap);
                }
                ui.input_mut(|i| i.events.clear());
            }
        }
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.keymap.filter)
                    .hint_text(i18n.tr("prefs-search"))
                    .desired_width(260.0),
            );
            if ui.button(i18n.tr("keymap-reset-all")).clicked() {
                engine.keymap = inkra_engine::Keymap::defaults(os);
                out.push(Request::SaveKeymap);
            }
        });
        ui.add_space(6.0);
        let defaults = inkra_engine::Keymap::defaults(os);
        let f = self.keymap.filter.trim().to_lowercase();
        let mut rows: Vec<(String, String)> = engine
            .registry
            .iter()
            .map(|c| {
                (
                    c.spec().id.to_owned(),
                    i18n.tr(&inkra_engine::command::title_key(c.spec().id)),
                )
            })
            .filter(|(id, t)| {
                f.is_empty() || id.to_lowercase().contains(&f) || t.to_lowercase().contains(&f)
            })
            .collect();
        rows.sort_by(|a, b| a.0.cmp(&b.0));
        egui::Grid::new("keymap-grid")
            .num_columns(3)
            .striped(true)
            .spacing([12.0, 4.0])
            .show(ui, |ui| {
                ui.strong(i18n.tr("keymap-command"));
                ui.strong(i18n.tr("keymap-shortcut"));
                ui.label("");
                ui.end_row();
                for (id, title) in rows {
                    ui.horizontal(|ui| {
                        ui.label(&title);
                        ui.label(egui::RichText::new(&id).small().weak());
                    });
                    let chords: Vec<inkra_engine::KeyChord> = engine
                        .keymap
                        .bindings()
                        .iter()
                        .filter(|b| b.command == id)
                        .map(|b| b.chord)
                        .collect();
                    ui.horizontal(|ui| {
                        if self.keymap.recording.as_deref() == Some(id.as_str()) {
                            ui.label(egui::RichText::new(i18n.tr("keymap-record")).italics());
                        } else if chords.is_empty() {
                            ui.weak(i18n.tr("keymap-none"));
                        }
                        for c in &chords {
                            let text = c.label(os);
                            // Conflicts: the same chord bound to another command
                            // in the same context.
                            let others: Vec<String> = engine
                                .keymap
                                .lookup(c)
                                .filter(|b| b.command != id)
                                .map(|b| b.command.clone())
                                .collect();
                            let r = ui.label(egui::RichText::new(&text).monospace());
                            if !others.is_empty() {
                                r.on_hover_text(i18n.tr_args(
                                    "keymap-conflict",
                                    &[("command", others.join(", ").into())],
                                ));
                                ui.colored_label(egui::Color32::from_rgb(207, 34, 46), "⚠");
                            }
                        }
                    });
                    ui.horizontal(|ui| {
                        if ui
                            .small_button("＋")
                            .on_hover_text(i18n.tr("keymap-add"))
                            .clicked()
                        {
                            self.keymap.recording = Some(id.clone());
                        }
                        if !chords.is_empty() && ui.small_button(i18n.tr("keymap-remove")).clicked()
                        {
                            engine.keymap.unbind_command(&id);
                            out.push(Request::SaveKeymap);
                        }
                        let changed = {
                            let d: Vec<_> = defaults
                                .bindings()
                                .iter()
                                .filter(|b| b.command == id)
                                .map(|b| b.chord)
                                .collect();
                            d.len() != chords.len() || !d.iter().all(|c| chords.contains(c))
                        };
                        if changed && ui.small_button(i18n.tr("keymap-reset")).clicked() {
                            engine.keymap.unbind_command(&id);
                            for b in defaults.bindings().iter().filter(|b| b.command == id).rev() {
                                engine.keymap.restore(b.clone());
                            }
                            out.push(Request::SaveKeymap);
                        }
                    });
                    ui.end_row();
                }
            });
    }

    fn advanced_page(&mut self, ui: &mut egui::Ui, i18n: &I18n, out: &mut Vec<Request>) {
        let mut cmd = |ui: &mut egui::Ui, label: &str, id: &str, params: Value| {
            if ui.button(i18n.tr(label)).clicked() {
                out.push(Request::Invoke(id.into(), params));
            }
        };
        ui.horizontal(|ui| {
            cmd(ui, "prefs-export-settings", "app.exportSettings", json!({}));
            cmd(ui, "prefs-import-settings", "app.importSettings", json!({}));
        });
        ui.add_space(8.0);
        for (label, which) in [
            ("prefs-open-config", "config"),
            ("prefs-open-themes", "themes"),
            ("prefs-open-keybindings", "keybindings"),
            ("prefs-open-logs", "logs"),
        ] {
            cmd(ui, label, "app.openFolder", json!({ "which": which }));
        }
        ui.add_space(12.0);
        if self.confirm_reset {
            ui.label(i18n.tr("prefs-reset-confirm"));
            ui.horizontal(|ui| {
                if ui.button(i18n.tr("dialog-ok")).clicked() {
                    out.push(Request::Invoke("app.resetSettings".into(), json!({})));
                    self.confirm_reset = false;
                    self.lists.clear();
                }
                if ui.button(i18n.tr("dialog-cancel")).clicked() {
                    self.confirm_reset = false;
                }
            });
        } else if ui.button(i18n.tr("prefs-reset")).clicked() {
            self.confirm_reset = true;
        }
    }
}
