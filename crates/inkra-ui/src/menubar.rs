//! egui menu bar (Windows/Linux) rendered from the engine's menu model (UI-001/002).

use inkra_engine::Engine;
use inkra_engine::keymap::Os;
use inkra_engine::menu::{Dynamic, MENU_BAR, MenuItem};
use inkra_i18n::I18n;
use serde_json::{Value, json};

/// A command activation requested from a menu.
#[derive(Debug, Clone, PartialEq)]
pub struct Invocation {
    /// Command id.
    pub id: String,
    /// Parameters.
    pub params: Value,
}

/// Runtime menu contents (UI-001 dynamic items).
#[derive(Debug, Clone, Default)]
pub struct MenuData {
    /// Recent files: `(label, path)`.
    pub recent: Vec<(String, String)>,
    /// Open windows: `(label, id, is_current)`.
    pub windows: Vec<(String, u64, bool)>,
    /// Installed themes: `(name, is_current)`.
    pub themes: Vec<(String, bool)>,
}

/// Label of a command leaf.
pub fn item_label(i18n: &I18n, id: &str, label: Option<&str>) -> String {
    match label {
        Some(key) => i18n.tr(key),
        None => i18n.tr(&inkra_engine::command::title_key(id)),
    }
}

/// Shows the menu bar and returns the activated command, if any.
pub fn show(
    ui: &mut egui::Ui,
    engine: &mut Engine,
    i18n: &I18n,
    os: Os,
    data: &MenuData,
) -> Option<Invocation> {
    let mut out = None;
    egui::MenuBar::new().ui(ui, |ui| {
        for menu in MENU_BAR {
            ui.menu_button(i18n.tr(menu.label), |ui| {
                items(ui, menu.items, engine, i18n, os, data, &mut out);
            });
        }
    });
    out
}

fn items(
    ui: &mut egui::Ui,
    list: &[MenuItem],
    engine: &mut Engine,
    i18n: &I18n,
    os: Os,
    data: &MenuData,
    out: &mut Option<Invocation>,
) {
    let mut last_was_separator = true;
    for item in list.iter().filter(|i| i.visible_on(os)) {
        match item {
            MenuItem::Separator => {
                if !last_was_separator {
                    ui.separator();
                }
                last_was_separator = true;
                continue;
            }
            MenuItem::Command {
                id, params, label, ..
            } => {
                let text = item_label(i18n, id, *label);
                let enabled = engine.is_enabled(id);
                let checked = engine.is_checked(id);
                let shortcut = if params.is_none() {
                    engine
                        .keymap
                        .chord_for(id)
                        .map(|c| c.label(os))
                        .unwrap_or_default()
                } else {
                    String::new()
                };
                let text = match checked {
                    Some(true) => format!("✔ {text}"),
                    Some(false) => format!("    {text}"),
                    None => text,
                };
                let button = egui::Button::new(text).shortcut_text(shortcut);
                if ui.add_enabled(enabled, button).clicked() {
                    let params = params
                        .and_then(|p| serde_json::from_str(p).ok())
                        .unwrap_or(Value::Null);
                    *out = Some(Invocation {
                        id: (*id).to_owned(),
                        params,
                    });
                    ui.close();
                }
            }
            MenuItem::Submenu {
                label,
                items: children,
            } => {
                ui.menu_button(i18n.tr(label), |ui| {
                    items(ui, children, engine, i18n, os, data, out)
                });
            }
            MenuItem::Dynamic(kind) => {
                let mut any = false;
                match kind {
                    Dynamic::RecentFiles => {
                        for (label, path) in &data.recent {
                            any = true;
                            if ui.button(label).on_hover_text(path).clicked() {
                                *out = Some(Invocation {
                                    id: "file.openRecent".into(),
                                    params: json!({ "path": path }),
                                });
                                ui.close();
                            }
                        }
                    }
                    Dynamic::Windows => {
                        for (label, id, current) in &data.windows {
                            any = true;
                            let text = if *current {
                                format!("✔ {label}")
                            } else {
                                format!("    {label}")
                            };
                            if ui.button(text).clicked() {
                                *out = Some(Invocation {
                                    id: "view.switchWindow".into(),
                                    params: json!({ "window": id }),
                                });
                                ui.close();
                            }
                        }
                    }
                    Dynamic::Themes => {
                        for (name, current) in &data.themes {
                            any = true;
                            let text = if *current {
                                format!("✔ {name}")
                            } else {
                                format!("    {name}")
                            };
                            if ui.button(text).clicked() {
                                *out = Some(Invocation {
                                    id: "theme.select".into(),
                                    params: json!({ "name": name }),
                                });
                                ui.close();
                            }
                        }
                    }
                }
                if !any {
                    let empty = match kind {
                        Dynamic::RecentFiles => "menu-recent-empty",
                        Dynamic::Themes => "menu-themes-empty",
                        Dynamic::Windows => "menu-windows-empty",
                    };
                    ui.add_enabled(false, egui::Button::new(i18n.tr(empty)));
                }
            }
        }
        last_was_separator = false;
    }
}
