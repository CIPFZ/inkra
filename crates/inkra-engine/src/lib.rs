//! # inkra-engine
//!
//! **Layer L5** (see SPEC 2.2).
//!
//! Application engine: the command system (all command ids), editor state machine,
//! preferences, theme loading, key bindings and the menu model. It has no GUI
//! dependency so that the CLI can drive it headlessly.

pub mod builtin;
pub mod command;
pub mod editing;
pub mod editor;
pub mod formatting;
pub mod keymap;
pub mod menu;
pub mod prefs;
pub mod state;

use serde_json::Value;

pub use command::{Category, Command, CommandError, CommandRegistry, CommandResult, Outcome};
pub use keymap::{Key, KeyChord, Keymap, Os};
pub use prefs::Preferences;
pub use state::{AppState, CommandContext, Effect, Notice};

/// The engine: commands, key bindings and application state.
#[derive(Debug)]
pub struct Engine {
    /// All commands.
    pub registry: CommandRegistry,
    /// Active key bindings.
    pub keymap: Keymap,
    /// Application state.
    pub state: AppState,
    effects: Vec<Effect>,
}

impl Engine {
    /// An engine with all built-in commands and the default key map.
    pub fn new(prefs: Preferences) -> Self {
        Self {
            registry: builtin_registry(),
            keymap: Keymap::defaults(Os::current()),
            state: AppState {
                prefs,
                ..AppState::default()
            },
            effects: Vec::new(),
        }
    }

    /// Runs `id` with `params`.
    pub fn execute(&mut self, id: &str, params: &Value) -> CommandResult {
        let mut cx = CommandContext::new(&mut self.state, &mut self.effects);
        self.registry.execute(id, &mut cx, params)
    }

    /// Whether `id` is registered and enabled.
    pub fn is_enabled(&mut self, id: &str) -> bool {
        let cx = CommandContext::new(&mut self.state, &mut self.effects);
        self.registry.is_enabled(id, &cx)
    }

    /// Toggle state of `id`.
    pub fn is_checked(&mut self, id: &str) -> Option<bool> {
        let cx = CommandContext::new(&mut self.state, &mut self.effects);
        self.registry.is_checked(id, &cx)
    }

    /// Takes the effects produced since the last call.
    pub fn take_effects(&mut self) -> Vec<Effect> {
        std::mem::take(&mut self.effects)
    }
}

/// A registry with every built-in command (used by the app, CLI and `xtask commands`).
pub fn builtin_registry() -> CommandRegistry {
    let mut r = CommandRegistry::new();
    builtin::register_all(&mut r);
    editing::register(&mut r);
    formatting::register(&mut r);
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_commands() {
        let mut e = Engine::new(Preferences::new());
        assert!(e.is_enabled("app.quit"));
        assert!(
            !e.is_enabled("file.save"),
            "unregistered commands are disabled"
        );
        assert_eq!(e.is_checked("view.toggleSidebar"), Some(false));
        e.execute("view.toggleSidebar", &Value::Null).unwrap();
        assert_eq!(e.is_checked("view.toggleSidebar"), Some(true));
        e.execute("app.about", &Value::Null).unwrap();
        e.execute("app.quit", &Value::Null).unwrap();
        assert_eq!(e.take_effects(), vec![Effect::ShowAbout, Effect::Quit]);
    }

    #[test]
    fn every_registered_command_has_i18n_title() {
        let i18n = inkra_i18n::I18n::new("en-US");
        for c in builtin_registry().iter() {
            assert!(
                i18n.has(&c.spec().title_key()),
                "missing title for {}",
                c.spec().id
            );
        }
    }

    #[test]
    fn every_menu_label_has_i18n() {
        for tag in ["en-US", "zh-CN"] {
            let i18n = inkra_i18n::I18n::new(tag);
            menu::for_each_label(|k| assert!(i18n.has(k), "{tag}: missing {k}"));
            menu::for_each_command(|item| {
                if let menu::MenuItem::Command {
                    id, label: None, ..
                } = item
                {
                    let k = command::title_key(id);
                    assert!(i18n.has(&k), "{tag}: missing {k}");
                }
            });
        }
    }
}
