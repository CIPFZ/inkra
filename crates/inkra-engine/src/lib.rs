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
pub mod exporting;
pub mod files;
pub mod formatting;
pub mod keymap;
pub mod menu;
pub mod prefs;
pub mod sidebar;
pub mod state;
pub mod tables;
pub mod theme;
pub mod theming;
pub mod viewing;

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
        let mut state = AppState {
            prefs,
            ..AppState::default()
        };
        state.meta.id = 1;
        state.next_window = 1;
        state.meta.recovery_id = state::recovery_id(1);
        state.editor.doc.set_options(markdown_options(&state.prefs));
        state.editor.doc.line_ending = new_line_ending(&state.prefs);
        state.editor.prefs = edit_prefs(&state.prefs);
        inkra_workspace::tree::set_markdown_exts(
            &state.prefs.strings("general.markdownExtensions"),
        );
        let _ = state.reload_theme();
        viewing::view_from_prefs(&mut state.view, &state.prefs);
        Self {
            registry: builtin_registry(),
            keymap: Keymap::defaults(Os::current()),
            state,
            effects: Vec::new(),
        }
    }

    /// Runs `id` against window `window` (made active for the call).
    pub fn execute_in(&mut self, window: u64, id: &str, params: &Value) -> CommandResult {
        self.state.activate(window);
        self.execute(id, params)
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

    /// Loads the default keymap plus the user's `keybindings.json` (UI-020);
    /// returns problems with the file.
    pub fn load_keybindings(&mut self) -> Vec<String> {
        let mut km = Keymap::defaults(Os::current());
        let problems = match self
            .state
            .paths
            .as_ref()
            .map(inkra_workspace::Paths::keybindings_file)
        {
            Some(f) if f.is_file() => match std::fs::read_to_string(&f) {
                Ok(t) => km.apply_user(&t),
                Err(e) => vec![format!("{}: {e}", f.display())],
            },
            _ => Vec::new(),
        };
        self.keymap = km;
        problems
    }

    /// Writes the current keymap's differences from the defaults to
    /// `keybindings.json` (UI-020).
    pub fn save_keybindings(&self) -> std::io::Result<()> {
        let Some(f) = self
            .state
            .paths
            .as_ref()
            .map(inkra_workspace::Paths::keybindings_file)
        else {
            return Ok(());
        };
        let entries = self.keymap.user_entries(&Keymap::defaults(Os::current()));
        let text = serde_json::to_string_pretty(&entries).unwrap_or_else(|_| "[]".into());
        inkra_workspace::fs::atomic_write(&f, format!("{text}\n").as_bytes())
    }

    /// Takes the effects produced since the last call.
    pub fn take_effects(&mut self) -> Vec<Effect> {
        std::mem::take(&mut self.effects)
    }
}

/// Editing preferences (ED-100, ED-101, ED-102, PR-031, PR-032, PR-041).
pub fn edit_prefs(prefs: &Preferences) -> editor::ops::EditPrefs {
    let smart = prefs.bool("markdown.smartPunctuation");
    editor::ops::EditPrefs {
        pair_brackets: prefs.bool("editor.autoPairBrackets"),
        pair_markdown: prefs.bool("editor.autoPairMarkdown"),
        pair_dollar: prefs.bool("markdown.inlineMath"),
        tab_width: prefs.f64("editor.tabWidth").clamp(1.0, 16.0) as usize,
        use_spaces: prefs.bool("editor.useSpaces"),
        renumber: prefs.bool("markdown.renumberLists"),
        list_from_one: prefs.bool("markdown.listFromOne"),
        table_align: prefs.bool("markdown.tableAlign"),
        smart_quotes: if smart {
            match prefs.string("markdown.smartQuotes").as_str() {
                "curly" => Some(('“', '”', '‘', '’')),
                "corner" => Some(('「', '」', '『', '』')),
                "guillemets" => Some(('«', '»', '‹', '›')),
                _ => None,
            }
        } else {
            None
        },
        smart_dashes: smart && prefs.bool("markdown.smartDashes"),
        smart_ellipsis: smart && prefs.bool("markdown.smartEllipsis"),
    }
}

/// The line ending for new documents (PR-016).
pub fn new_line_ending(prefs: &Preferences) -> inkra_text::LineEnding {
    if prefs.string("general.defaultNewline") == "crlf" {
        inkra_text::LineEnding::Crlf
    } else {
        inkra_text::LineEnding::Lf
    }
}

/// Parse options from the Markdown preferences (SPEC 4.3).
pub fn markdown_options(prefs: &Preferences) -> inkra_markdown::ParseOptions {
    inkra_markdown::ParseOptions {
        inline_math: prefs.bool("markdown.inlineMath"),
        subscript: prefs.bool("markdown.subscript"),
        superscript: prefs.bool("markdown.superscript"),
        highlight: prefs.bool("markdown.highlight"),
        strict_atx: prefs.bool("markdown.strictMode"),
        latex_delimiters: prefs.bool("markdown.latexDelimiters"),
        loose_inline_math: prefs.bool("markdown.looseInlineMath"),
        ..Default::default()
    }
}

/// A registry with every built-in command (used by the app, CLI and `xtask commands`).
pub fn builtin_registry() -> CommandRegistry {
    let mut r = CommandRegistry::new();
    builtin::register_all(&mut r);
    editing::register(&mut r);
    formatting::register(&mut r);
    files::register(&mut r);
    theming::register(&mut r);
    viewing::register(&mut r);
    tables::register(&mut r);
    sidebar::register(&mut r);
    exporting::register(&mut r);
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
            !e.is_enabled("help.checkUpdates"),
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

#[cfg(test)]
mod table_command_tests {
    use super::*;

    fn engine(src: &str, caret: usize) -> Engine {
        let mut e = Engine::new(Preferences::new());
        e.state.editor = editor::Editor::from_text(src);
        e.state.editor.sel = inkra_doc::Selection::caret(caret);
        e
    }

    #[test]
    fn structural_commands_rewrite_aligned_tables() {
        let src = "| a | b |\n|---|---|\n| 2 | x |\n| 10 | y |\n| 1 | z |";
        let mut e = engine(src, 2);
        e.execute("table.insertColRight", &Value::Null).unwrap();
        assert_eq!(
            e.state.editor.text(),
            "| a   |     | b   |\n| --- | --- | --- |\n| 2   |     | x   |\n| 10  |     | y   |\n| 1   |     | z   |"
        );
        e.execute("table.deleteCol", &Value::Null).unwrap();
        eprintln!("after deleteCol:\n{}", e.state.editor.text());
        e.state.editor.sel = inkra_doc::Selection::caret(2);
        e.execute("table.sortByColumnAsc", &Value::Null).unwrap();
        let t = e.state.editor.text();
        let rows: Vec<&str> = t
            .lines()
            .skip(2)
            .map(|l| l.split('|').nth(1).unwrap().trim())
            .collect();
        assert_eq!(rows, ["1", "2", "10"], "numeric sort");
        e.execute(
            "table.moveRowTo",
            &serde_json::json!({ "from": 3, "to": 1 }),
        )
        .unwrap();
        let t = e.state.editor.text();
        assert!(t.lines().nth(2).unwrap().starts_with("| 10"), "{t}");
        e.execute(
            "table.moveRowTo",
            &serde_json::json!({ "from": 1, "to": 3 }),
        )
        .unwrap();
        e.execute(
            "table.moveColumnTo",
            &serde_json::json!({ "from": 0, "to": 1 }),
        )
        .unwrap();
        assert!(
            e.state.editor.text().starts_with("| b"),
            "{}",
            e.state.editor.text()
        );
        e.execute(
            "table.moveColumnTo",
            &serde_json::json!({ "from": 1, "to": 0 }),
        )
        .unwrap();
        e.state.editor.sel = inkra_doc::Selection::caret(2);
        e.execute("table.alignCenter", &Value::Null).unwrap();
        assert!(
            e.state
                .editor
                .text()
                .lines()
                .nth(1)
                .unwrap()
                .starts_with("| :-:")
        );
        e.execute("table.resize", &serde_json::json!({ "rows": 1, "cols": 3 }))
            .unwrap();
        assert_eq!(e.state.editor.text().lines().count(), 3);
        e.execute("table.copyAsCsv", &Value::Null).unwrap();
        let eff = e.take_effects();
        assert!(
            eff.iter()
                .any(|x| matches!(x, Effect::SetClipboard { text, .. } if text.starts_with("a,b"))),
            "{eff:?}"
        );
        e.execute("table.deleteTable", &Value::Null).unwrap();
        assert_eq!(e.state.editor.text(), "");
        // undo restores
        while e.state.editor.undo() {}
        assert_eq!(e.state.editor.text(), src);
    }
}
