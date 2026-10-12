//! Native macOS menu bar via `muda` (UI-001), built from the same engine menu model.

use std::collections::HashMap;

use inkra_engine::Engine;
use inkra_engine::keymap::{Key, KeyChord, Os};
use inkra_engine::menu::{MENU_BAR, MenuItem};
use inkra_i18n::I18n;
use muda::accelerator::Accelerator;
use muda::{CheckMenuItem, Menu, MenuEvent, PredefinedMenuItem, Submenu};
use serde_json::Value;

use crate::menubar::{Invocation, item_label};

enum Leaf {
    Plain(muda::MenuItem),
    Check(CheckMenuItem),
}

/// The installed native menu.
pub struct NativeMenu {
    _menu: Menu,
    leaves: Vec<(String, Leaf)>,
    actions: HashMap<String, Invocation>,
}

fn code(key: Key) -> Option<String> {
    Some(match key {
        Key::Char(c) if c.is_ascii_alphabetic() => format!("Key{}", c.to_ascii_uppercase()),
        Key::Char(c) if c.is_ascii_digit() => format!("Digit{c}"),
        Key::Char('`') => "Backquote".into(),
        Key::Char('-') => "Minus".into(),
        Key::Char('=') => "Equal".into(),
        Key::Char('[') => "BracketLeft".into(),
        Key::Char(']') => "BracketRight".into(),
        Key::Char('\\') => "Backslash".into(),
        Key::Char(';') => "Semicolon".into(),
        Key::Char('\'') => "Quote".into(),
        Key::Char(',') => "Comma".into(),
        Key::Char('.') => "Period".into(),
        Key::Char('/') => "Slash".into(),
        Key::Char(_) => return None,
        Key::F(n) => format!("F{n}"),
        Key::Enter => "Enter".into(),
        Key::Tab => "Tab".into(),
        Key::Backspace => "Backspace".into(),
        Key::Delete => "Delete".into(),
        Key::Escape => "Escape".into(),
        Key::Space => "Space".into(),
        Key::Home => "Home".into(),
        Key::End => "End".into(),
        Key::PageUp => "PageUp".into(),
        Key::PageDown => "PageDown".into(),
        Key::Up => "ArrowUp".into(),
        Key::Down => "ArrowDown".into(),
        Key::Left => "ArrowLeft".into(),
        Key::Right => "ArrowRight".into(),
        Key::Insert => "Insert".into(),
    })
}

fn accelerator(chord: KeyChord) -> Option<Accelerator> {
    let mut s = String::new();
    if chord.ctrl {
        s.push_str("CmdOrCtrl+");
    }
    if chord.mac_ctrl {
        s.push_str("Ctrl+");
    }
    if chord.alt {
        s.push_str("Alt+");
    }
    if chord.shift {
        s.push_str("Shift+");
    }
    s.push_str(&code(chord.key)?);
    s.parse().ok()
}

impl NativeMenu {
    /// Builds the menu and installs it as the application menu.
    pub fn install(engine: &mut Engine, i18n: &I18n) -> Self {
        let menu = Menu::new();
        let mut this = Self {
            _menu: Menu::new(),
            leaves: Vec::new(),
            actions: HashMap::new(),
        };

        let app_menu = Submenu::new("Inkra", true);
        let _ = app_menu.append(&PredefinedMenuItem::services(None));
        let _ = app_menu.append(&PredefinedMenuItem::separator());
        let _ = app_menu.append(&PredefinedMenuItem::hide(None));
        let _ = app_menu.append(&PredefinedMenuItem::hide_others(None));
        let _ = app_menu.append(&PredefinedMenuItem::show_all(None));
        let _ = menu.append(&app_menu);

        for top in MENU_BAR {
            let sub = Submenu::new(i18n.tr(top.label), true);
            this.fill(&sub, top.items, engine, i18n);
            let _ = menu.append(&sub);
        }
        menu.init_for_nsapp();
        this._menu = menu;
        this
    }

    fn fill(&mut self, parent: &Submenu, items: &[MenuItem], engine: &mut Engine, i18n: &I18n) {
        for item in items.iter().filter(|i| i.visible_on(Os::Mac)) {
            match item {
                MenuItem::Separator => {
                    let _ = parent.append(&PredefinedMenuItem::separator());
                }
                MenuItem::Command {
                    id, params, label, ..
                } => {
                    let key = format!("{id}#{}", self.actions.len());
                    let text = item_label(i18n, id, *label);
                    let accel = if params.is_none() {
                        engine.keymap.chord_for(id).and_then(accelerator)
                    } else {
                        None
                    };
                    let enabled = engine.is_enabled(id);
                    let leaf = match engine.is_checked(id) {
                        Some(checked) => {
                            let it =
                                CheckMenuItem::with_id(key.as_str(), text, enabled, checked, accel);
                            let _ = parent.append(&it);
                            Leaf::Check(it)
                        }
                        None => {
                            let it = muda::MenuItem::with_id(key.as_str(), text, enabled, accel);
                            let _ = parent.append(&it);
                            Leaf::Plain(it)
                        }
                    };
                    let params = params
                        .and_then(|p| serde_json::from_str(p).ok())
                        .unwrap_or(Value::Null);
                    self.actions.insert(
                        key,
                        Invocation {
                            id: (*id).to_owned(),
                            params,
                        },
                    );
                    self.leaves.push(((*id).to_owned(), leaf));
                }
                MenuItem::Submenu { label, items } => {
                    let sub = Submenu::new(i18n.tr(label), true);
                    self.fill(&sub, items, engine, i18n);
                    let _ = parent.append(&sub);
                }
                MenuItem::Dynamic(_) => {}
            }
        }
    }

    /// Refreshes enabled/checked states from the engine.
    pub fn refresh(&self, engine: &mut Engine) {
        for (id, leaf) in &self.leaves {
            let enabled = engine.is_enabled(id);
            match leaf {
                Leaf::Plain(it) => it.set_enabled(enabled),
                Leaf::Check(it) => {
                    it.set_enabled(enabled);
                    it.set_checked(engine.is_checked(id).unwrap_or(false));
                }
            }
        }
    }

    /// Drains activated menu items.
    pub fn poll(&self) -> Vec<Invocation> {
        let mut out = Vec::new();
        while let Ok(ev) = MenuEvent::receiver().try_recv() {
            if let Some(inv) = self.actions.get(ev.id().as_ref()) {
                out.push(inv.clone());
            }
        }
        out
    }
}
