//! Key chords and the default key bindings (UI-010).
//!
//! The engine is independent of any GUI toolkit, so chords use their own [`Key`]
//! type; the UI layer maps toolkit events onto it. `Ctrl` in a chord means the
//! platform primary modifier: `Cmd` (⌘) on macOS and `Ctrl` elsewhere (SPEC 0.3).

use std::fmt;
use std::str::FromStr;

use serde_json::Value;

/// Operating system family, used for platform-specific bindings and labels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Os {
    /// Microsoft Windows.
    Windows,
    /// Apple macOS.
    Mac,
    /// Linux and other Unix desktops.
    Linux,
}

impl Os {
    /// The OS this binary was compiled for.
    pub const fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::Mac
        } else if cfg!(windows) {
            Self::Windows
        } else {
            Self::Linux
        }
    }
}

/// A non-modifier key, named after its US-layout physical position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Key {
    /// Letter, digit or punctuation key, stored as its unshifted US-layout character
    /// (`a`..`z`, `0`..`9`, `` ` - = [ ] \ ; ' , . / ``).
    Char(char),
    /// Function key F1..F24.
    F(u8),
    /// Enter / Return.
    Enter,
    /// Tab.
    Tab,
    /// Backspace.
    Backspace,
    /// Forward delete.
    Delete,
    /// Escape.
    Escape,
    /// Space bar.
    Space,
    /// Home.
    Home,
    /// End.
    End,
    /// Page up.
    PageUp,
    /// Page down.
    PageDown,
    /// Arrow up.
    Up,
    /// Arrow down.
    Down,
    /// Arrow left.
    Left,
    /// Arrow right.
    Right,
    /// Insert.
    Insert,
}

impl Key {
    fn name(self) -> String {
        match self {
            Self::Char(c) => c.to_ascii_uppercase().to_string(),
            Self::F(n) => format!("F{n}"),
            Self::Enter => "Enter".into(),
            Self::Tab => "Tab".into(),
            Self::Backspace => "Backspace".into(),
            Self::Delete => "Delete".into(),
            Self::Escape => "Esc".into(),
            Self::Space => "Space".into(),
            Self::Home => "Home".into(),
            Self::End => "End".into(),
            Self::PageUp => "PageUp".into(),
            Self::PageDown => "PageDown".into(),
            Self::Up => "Up".into(),
            Self::Down => "Down".into(),
            Self::Left => "Left".into(),
            Self::Right => "Right".into(),
            Self::Insert => "Insert".into(),
        }
    }

    fn mac_symbol(self) -> String {
        match self {
            Self::Enter => "↩".into(),
            Self::Tab => "⇥".into(),
            Self::Backspace => "⌫".into(),
            Self::Delete => "⌦".into(),
            Self::Escape => "⎋".into(),
            Self::Up => "↑".into(),
            Self::Down => "↓".into(),
            Self::Left => "←".into(),
            Self::Right => "→".into(),
            Self::Home => "↖".into(),
            Self::End => "↘".into(),
            Self::PageUp => "⇞".into(),
            Self::PageDown => "⇟".into(),
            other => other.name(),
        }
    }
}

/// Modifier set plus a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct KeyChord {
    /// Primary modifier: `Cmd` on macOS, `Ctrl` elsewhere.
    pub ctrl: bool,
    /// Shift.
    pub shift: bool,
    /// Alt / Option.
    pub alt: bool,
    /// The physical `Control` key on macOS (written `MacCtrl` in chord strings).
    /// Always `false` on other platforms.
    pub mac_ctrl: bool,
    /// The key.
    pub key: Key,
}

impl KeyChord {
    /// A chord without modifiers.
    pub const fn key(key: Key) -> Self {
        Self {
            ctrl: false,
            shift: false,
            alt: false,
            mac_ctrl: false,
            key,
        }
    }

    /// Platform-appropriate label, e.g. `Ctrl+Shift+K` or `⇧⌘K`.
    pub fn label(&self, os: Os) -> String {
        if os == Os::Mac {
            let mut s = String::new();
            if self.mac_ctrl {
                s.push('⌃');
            }
            if self.alt {
                s.push('⌥');
            }
            if self.shift {
                s.push('⇧');
            }
            if self.ctrl {
                s.push('⌘');
            }
            s.push_str(&self.key.mac_symbol());
            s
        } else {
            let mut parts: Vec<String> = Vec::new();
            if self.ctrl {
                parts.push("Ctrl".into());
            }
            if self.shift {
                parts.push("Shift".into());
            }
            if self.alt {
                parts.push("Alt".into());
            }
            parts.push(self.key.name());
            parts.join("+")
        }
    }
}

impl fmt::Display for KeyChord {
    /// Canonical, platform-neutral form accepted by [`KeyChord::from_str`].
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.mac_ctrl {
            f.write_str("MacCtrl+")?;
        }
        if self.ctrl {
            f.write_str("Ctrl+")?;
        }
        if self.shift {
            f.write_str("Shift+")?;
        }
        if self.alt {
            f.write_str("Alt+")?;
        }
        f.write_str(&self.key.name())
    }
}

/// Error parsing a chord string.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid key chord `{0}`")]
pub struct ParseChordError(pub String);

impl FromStr for KeyChord {
    type Err = ParseChordError;

    /// Parses `Ctrl+Shift+K`, `Alt+Shift+5`, `F11`, `Ctrl+=`, `Ctrl++`… Modifier names
    /// are case-insensitive; `Cmd`/`Meta` are aliases of `Ctrl`, `Option` of `Alt`.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let err = || ParseChordError(s.to_owned());
        let s = s.trim();
        if s.is_empty() {
            return Err(err());
        }
        // A trailing "+" key ("Ctrl++") would otherwise produce an empty segment.
        let (mods_part, key_part) = if let Some(stripped) = s.strip_suffix("++") {
            (stripped, "+")
        } else {
            match s.rsplit_once('+') {
                Some((m, k)) => (m, k),
                None => ("", s),
            }
        };
        let mut chord = KeyChord::key(parse_key(key_part).ok_or_else(err)?);
        if !mods_part.is_empty() {
            for m in mods_part.split('+') {
                match m.trim().to_ascii_lowercase().as_str() {
                    "ctrl" | "cmd" | "command" | "meta" | "primary" => chord.ctrl = true,
                    "shift" => chord.shift = true,
                    "alt" | "option" | "opt" => chord.alt = true,
                    "macctrl" => chord.mac_ctrl = true,
                    _ => return Err(err()),
                }
            }
        }
        Ok(chord)
    }
}

fn parse_key(k: &str) -> Option<Key> {
    let lower = k.trim().to_ascii_lowercase();
    Some(match lower.as_str() {
        "enter" | "return" => Key::Enter,
        "tab" => Key::Tab,
        "backspace" => Key::Backspace,
        "delete" | "del" => Key::Delete,
        "esc" | "escape" => Key::Escape,
        "space" => Key::Space,
        "home" => Key::Home,
        "end" => Key::End,
        "pageup" => Key::PageUp,
        "pagedown" => Key::PageDown,
        "up" => Key::Up,
        "down" => Key::Down,
        "left" => Key::Left,
        "right" => Key::Right,
        "insert" => Key::Insert,
        "+" => Key::Char('='),
        _ => {
            if let Some(n) = lower.strip_prefix('f').and_then(|n| n.parse::<u8>().ok()) {
                if (1..=24).contains(&n) {
                    return Some(Key::F(n));
                }
                return None;
            }
            let mut chars = lower.chars();
            let c = chars.next()?;
            if chars.next().is_some() {
                return None;
            }
            match c {
                'a'..='z' | '0'..='9' => Key::Char(c),
                '`' | '-' | '=' | '[' | ']' | '\\' | ';' | '\'' | ',' | '.' | '/' => Key::Char(c),
                _ => return None,
            }
        }
    })
}

/// Context restriction of a binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum When {
    /// Always active.
    Always,
    /// Only when the caret is inside a table.
    InTable,
    /// Only when the caret is not inside a table.
    NotInTable,
}

/// One key binding.
#[derive(Debug, Clone, PartialEq)]
pub struct Binding {
    /// The chord.
    pub chord: KeyChord,
    /// Command id.
    pub command: String,
    /// Parameters passed to the command.
    pub params: Value,
    /// Context restriction.
    pub when: When,
}

/// Static default binding: chord string, command id, platform restriction.
struct DefaultBinding {
    chord: &'static str,
    command: &'static str,
    only: Option<Os>,
    not: Option<Os>,
    when: When,
}

const fn b(chord: &'static str, command: &'static str) -> DefaultBinding {
    DefaultBinding {
        chord,
        command,
        only: None,
        not: None,
        when: When::Always,
    }
}
const fn mac(chord: &'static str, command: &'static str) -> DefaultBinding {
    DefaultBinding {
        chord,
        command,
        only: Some(Os::Mac),
        not: None,
        when: When::Always,
    }
}
const fn not_mac(chord: &'static str, command: &'static str) -> DefaultBinding {
    DefaultBinding {
        chord,
        command,
        only: None,
        not: Some(Os::Mac),
        when: When::Always,
    }
}
const fn when(chord: &'static str, command: &'static str, when: When) -> DefaultBinding {
    DefaultBinding {
        chord,
        command,
        only: None,
        not: None,
        when,
    }
}

/// SPEC 10.2 default bindings. Order matters only for display (first binding of a
/// command is shown in menus).
const DEFAULTS: &[DefaultBinding] = &[
    b("Ctrl+N", "file.new"),
    b("Ctrl+Shift+N", "file.newWindow"),
    b("Ctrl+O", "file.open"),
    // On macOS Cmd+Shift+O is Quick Open (FS-030); Open Folder has no default there.
    not_mac("Ctrl+Shift+O", "file.openFolder"),
    b("Ctrl+P", "file.quickOpen"),
    mac("Ctrl+Shift+O", "file.quickOpen"),
    b("Ctrl+S", "file.save"),
    b("Ctrl+Shift+S", "file.saveAs"),
    b("Ctrl+W", "file.close"),
    b("Ctrl+,", "app.preferences"),
    b("Ctrl+Q", "app.quit"),
    b("Ctrl+Z", "edit.undo"),
    b("Ctrl+Shift+Z", "edit.redo"),
    b("Ctrl+Y", "edit.redo"),
    b("Ctrl+X", "edit.cut"),
    b("Ctrl+C", "edit.copy"),
    b("Ctrl+V", "edit.paste"),
    b("Ctrl+Shift+V", "edit.pastePlain"),
    b("Ctrl+A", "edit.selectAll"),
    b("Ctrl+F", "edit.find"),
    b("Ctrl+H", "edit.replace"),
    b("F3", "edit.findNext"),
    b("Shift+F3", "edit.findPrev"),
    b("Ctrl+Shift+F", "search.global"),
    b("Ctrl+1", "paragraph.heading1"),
    b("Ctrl+2", "paragraph.heading2"),
    b("Ctrl+3", "paragraph.heading3"),
    b("Ctrl+4", "paragraph.heading4"),
    b("Ctrl+5", "paragraph.heading5"),
    b("Ctrl+6", "paragraph.heading6"),
    b("Ctrl+0", "paragraph.normal"),
    b("Ctrl+=", "paragraph.headingUp"),
    b("Ctrl+-", "paragraph.headingDown"),
    b("Ctrl+T", "paragraph.table"),
    b("Ctrl+Shift+K", "paragraph.codeFences"),
    b("Ctrl+Shift+M", "paragraph.mathBlock"),
    b("Ctrl+Shift+Q", "paragraph.quote"),
    b("Ctrl+Shift+[", "paragraph.orderedList"),
    b("Ctrl+Shift+]", "paragraph.unorderedList"),
    b("Ctrl+]", "paragraph.indent"),
    b("Ctrl+[", "paragraph.outdent"),
    b("Ctrl+B", "format.bold"),
    b("Ctrl+I", "format.italic"),
    b("Ctrl+U", "format.underline"),
    b("Ctrl+Shift+`", "format.code"),
    b("Alt+Shift+5", "format.strike"),
    b("Ctrl+K", "format.link"),
    b("Ctrl+Shift+I", "format.image"),
    b("Ctrl+\\", "format.clear"),
    b("Ctrl+Shift+L", "view.toggleSidebar"),
    b("Ctrl+Shift+1", "view.outline"),
    b("Ctrl+Shift+2", "view.articles"),
    b("Ctrl+Shift+3", "view.fileTree"),
    b("Ctrl+/", "view.sourceMode"),
    b("F8", "view.focusMode"),
    b("F9", "view.typewriterMode"),
    not_mac("F11", "view.fullscreen"),
    mac("MacCtrl+Ctrl+F", "view.fullscreen"),
    b("Ctrl+Shift+0", "view.actualSize"),
    b("Ctrl+Shift+=", "view.zoomIn"),
    b("Ctrl+Shift+-", "view.zoomOut"),
    b("Ctrl+Home", "edit.jumpTop"),
    b("Ctrl+End", "edit.jumpBottom"),
    b("Ctrl+J", "edit.jumpSelection"),
    when("Alt+Up", "paragraph.moveUp", When::NotInTable),
    when("Alt+Down", "paragraph.moveDown", When::NotInTable),
    when("Alt+Up", "table.moveRowUp", When::InTable),
    when("Alt+Down", "table.moveRowDown", When::InTable),
    when("Ctrl+Enter", "table.insertRowBelow", When::InTable),
    b("Ctrl+D", "edit.selectWord"),
    b("Ctrl+L", "edit.selectLine"),
    b("Ctrl+E", "edit.selectStyleScope"),
    b("Ctrl+Shift+D", "edit.deleteWord"),
    b("Ctrl+Shift+P", "app.commandPalette"),
];

/// The active set of bindings.
#[derive(Debug, Clone, Default)]
pub struct Keymap {
    bindings: Vec<Binding>,
}

impl Keymap {
    /// SPEC 10.2 defaults for `os`.
    pub fn defaults(os: Os) -> Self {
        let bindings = DEFAULTS
            .iter()
            .filter(|d| d.only.is_none_or(|o| o == os) && d.not.is_none_or(|o| o != os))
            .map(|d| Binding {
                chord: d
                    .chord
                    .parse()
                    .unwrap_or_else(|e| panic!("bad default binding: {e}")),
                command: d.command.to_owned(),
                params: Value::Null,
                when: d.when,
            })
            .collect();
        Self { bindings }
    }

    /// All bindings.
    pub fn bindings(&self) -> &[Binding] {
        &self.bindings
    }

    /// First chord bound to `command` (used for menu shortcut labels).
    pub fn chord_for(&self, command: &str) -> Option<KeyChord> {
        self.bindings
            .iter()
            .find(|b| b.command == command)
            .map(|b| b.chord)
    }

    /// All bindings matching `chord` (several when they differ in [`When`]).
    pub fn lookup(&self, chord: &KeyChord) -> impl Iterator<Item = &Binding> {
        let chord = *chord;
        self.bindings.iter().filter(move |b| b.chord == chord)
    }

    /// Resolves `chord` in the given context.
    pub fn resolve(&self, chord: &KeyChord, in_table: bool) -> Option<&Binding> {
        self.lookup(chord).find(|b| match b.when {
            When::Always => true,
            When::InTable => in_table,
            When::NotInTable => !in_table,
        })
    }

    /// Removes every binding of `chord`.
    pub fn unbind_chord(&mut self, chord: &KeyChord) {
        self.bindings.retain(|b| b.chord != *chord);
    }

    /// Removes every binding of `command`.
    pub fn unbind_command(&mut self, command: &str) {
        self.bindings.retain(|b| b.command != command);
    }

    /// Applies the user's `keybindings.json` (UI-020): an array of
    /// `{"command": id, "key": "Ctrl+B" | null, "args": {…}?}`. An entry with
    /// `"key": null` removes the command's default bindings; an entry with a key
    /// adds a binding that wins over others on the same chord. Returns one
    /// message per entry that could not be used.
    pub fn apply_user(&mut self, json: &str) -> Vec<String> {
        let mut problems = Vec::new();
        let list: Vec<Value> = match serde_json::from_str(json) {
            Ok(Value::Array(a)) => a,
            Ok(_) => return vec!["keybindings.json must contain an array".into()],
            Err(e) => return vec![format!("keybindings.json: {e}")],
        };
        // Removals first, so that a removal and a new key for the same command
        // can come in any order.
        for v in &list {
            if v.get("key").is_some_and(Value::is_null)
                && let Some(c) = v.get("command").and_then(Value::as_str)
            {
                self.unbind_command(c);
            }
        }
        for (k, v) in list.iter().enumerate() {
            let Some(cmd) = v.get("command").and_then(Value::as_str) else {
                problems.push(format!("entry {}: missing \"command\"", k + 1));
                continue;
            };
            match v.get("key") {
                Some(Value::Null) => {}
                Some(Value::String(key)) => match key.parse::<KeyChord>() {
                    Ok(chord) => {
                        self.bind(chord, cmd, v.get("args").cloned().unwrap_or(Value::Null))
                    }
                    Err(e) => problems.push(format!("entry {} ({cmd}): {e}", k + 1)),
                },
                _ => problems.push(format!(
                    "entry {} ({cmd}): \"key\" must be a string or null",
                    k + 1
                )),
            }
        }
        problems
    }

    /// The user entries that turn `defaults` into `self`, for writing
    /// `keybindings.json` from the keymap editor: removals for commands whose
    /// bindings changed, then their current keys.
    pub fn user_entries(&self, defaults: &Keymap) -> Vec<Value> {
        let mut commands: Vec<&str> = self
            .bindings
            .iter()
            .chain(&defaults.bindings)
            .map(|b| b.command.as_str())
            .collect();
        commands.sort_unstable();
        commands.dedup();
        let mut out = Vec::new();
        for c in commands {
            let mine: Vec<&Binding> = self.bindings.iter().filter(|b| b.command == c).collect();
            let def: Vec<&Binding> = defaults
                .bindings
                .iter()
                .filter(|b| b.command == c)
                .collect();
            let same = mine.len() == def.len()
                && mine
                    .iter()
                    .all(|m| def.iter().any(|d| d.chord == m.chord && d.when == m.when));
            if same {
                continue;
            }
            if !def.is_empty() {
                out.push(serde_json::json!({ "command": c, "key": Value::Null }));
            }
            for m in mine {
                let mut e = serde_json::json!({ "command": c, "key": m.chord.to_string() });
                if !m.params.is_null() {
                    e["args"] = m.params.clone();
                }
                out.push(e);
            }
        }
        out
    }

    /// Puts a (default) binding back, with its context.
    pub fn restore(&mut self, b: Binding) {
        self.bindings
            .retain(|x| !(x.chord == b.chord && x.when == b.when));
        self.bindings.insert(0, b);
    }

    /// Adds a binding (taking precedence over existing ones for the same chord).
    pub fn bind(&mut self, chord: KeyChord, command: &str, params: Value) {
        self.bindings
            .retain(|b| !(b.chord == chord && b.when == When::Always));
        self.bindings.insert(
            0,
            Binding {
                chord,
                command: command.to_owned(),
                params,
                when: When::Always,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_keybindings() {
        let defaults = Keymap::defaults(Os::Windows);
        let mut km = defaults.clone();
        let problems = km.apply_user(
            r#"[{"command":"format.bold","key":null},{"command":"format.bold","key":"Ctrl+Alt+B"},{"command":"x","key":"Nope+?"},{"key":"Ctrl+1"}]"#,
        );
        assert_eq!(problems.len(), 2);
        assert_eq!(
            km.chord_for("format.bold")
                .map(|c| c.to_string())
                .as_deref(),
            Some("Ctrl+Alt+B")
        );
        assert!(km.resolve(&"Ctrl+B".parse().unwrap(), false).is_none());
        // Round trip through the entries written by the editor.
        let entries = km.user_entries(&defaults);
        let mut again = defaults.clone();
        assert!(
            again
                .apply_user(&serde_json::to_string(&entries).unwrap())
                .is_empty()
        );
        assert_eq!(again.chord_for("format.bold"), km.chord_for("format.bold"));
        assert!(defaults.user_entries(&defaults).is_empty());
    }

    #[test]
    fn parse_and_display() {
        let c: KeyChord = "Ctrl+Shift+K".parse().unwrap();
        assert!(c.ctrl && c.shift && !c.alt);
        assert_eq!(c.key, Key::Char('k'));
        assert_eq!(c.to_string(), "Ctrl+Shift+K");
        assert_eq!(c.label(Os::Mac), "⇧⌘K");
        assert_eq!(c.label(Os::Windows), "Ctrl+Shift+K");
        assert_eq!("cmd+,".parse::<KeyChord>().unwrap().key, Key::Char(','));
        assert_eq!("F11".parse::<KeyChord>().unwrap().key, Key::F(11));
        assert_eq!("Ctrl++".parse::<KeyChord>().unwrap().key, Key::Char('='));
        assert_eq!("Ctrl+\\".parse::<KeyChord>().unwrap().key, Key::Char('\\'));
        assert!("Ctrl+Hyper+K".parse::<KeyChord>().is_err());
        assert!("F99".parse::<KeyChord>().is_err());
        assert!("".parse::<KeyChord>().is_err());
    }

    #[test]
    fn defaults_parse_on_all_platforms() {
        for os in [Os::Windows, Os::Mac, Os::Linux] {
            let km = Keymap::defaults(os);
            assert!(km.bindings().len() > 60);
        }
    }

    #[test]
    fn no_conflicting_defaults() {
        for os in [Os::Windows, Os::Mac, Os::Linux] {
            let km = Keymap::defaults(os);
            let mut seen = std::collections::HashMap::new();
            for b in km.bindings() {
                if let Some(prev) = seen.insert((b.chord, b.when), &b.command) {
                    panic!("{os:?}: {} bound to both {prev} and {}", b.chord, b.command);
                }
            }
        }
    }

    #[test]
    fn contextual_resolution() {
        let km = Keymap::defaults(Os::Linux);
        let c: KeyChord = "Alt+Up".parse().unwrap();
        assert_eq!(km.resolve(&c, true).unwrap().command, "table.moveRowUp");
        assert_eq!(km.resolve(&c, false).unwrap().command, "paragraph.moveUp");
        let q: KeyChord = "Ctrl+Shift+O".parse().unwrap();
        assert_eq!(
            Keymap::defaults(Os::Mac)
                .resolve(&q, false)
                .unwrap()
                .command,
            "file.quickOpen"
        );
        assert_eq!(km.resolve(&q, false).unwrap().command, "file.openFolder");
    }
}
