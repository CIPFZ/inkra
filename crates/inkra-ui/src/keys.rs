//! Mapping egui keyboard events onto engine [`KeyChord`]s.

use inkra_engine::keymap::{Key, KeyChord, Os};

/// Maps an egui key to the engine key at the same US-layout position.
pub fn map_key(key: egui::Key) -> Option<Key> {
    use egui::Key as E;
    Some(match key {
        E::ArrowDown => Key::Down,
        E::ArrowUp => Key::Up,
        E::ArrowLeft => Key::Left,
        E::ArrowRight => Key::Right,
        E::Escape => Key::Escape,
        E::Tab => Key::Tab,
        E::Backspace => Key::Backspace,
        E::Enter => Key::Enter,
        E::Space => Key::Space,
        E::Insert => Key::Insert,
        E::Delete => Key::Delete,
        E::Home => Key::Home,
        E::End => Key::End,
        E::PageUp => Key::PageUp,
        E::PageDown => Key::PageDown,
        E::Colon | E::Semicolon => Key::Char(';'),
        E::Comma => Key::Char(','),
        E::Backslash | E::Pipe => Key::Char('\\'),
        E::Slash | E::Questionmark => Key::Char('/'),
        E::Exclamationmark => Key::Char('1'),
        E::OpenBracket | E::OpenCurlyBracket => Key::Char('['),
        E::CloseBracket | E::CloseCurlyBracket => Key::Char(']'),
        E::Backtick => Key::Char('`'),
        E::Minus => Key::Char('-'),
        E::Period => Key::Char('.'),
        E::Plus | E::Equals => Key::Char('='),
        E::Quote => Key::Char('\''),
        E::Num0 => Key::Char('0'),
        E::Num1 => Key::Char('1'),
        E::Num2 => Key::Char('2'),
        E::Num3 => Key::Char('3'),
        E::Num4 => Key::Char('4'),
        E::Num5 => Key::Char('5'),
        E::Num6 => Key::Char('6'),
        E::Num7 => Key::Char('7'),
        E::Num8 => Key::Char('8'),
        E::Num9 => Key::Char('9'),
        E::F1 => Key::F(1),
        E::F2 => Key::F(2),
        E::F3 => Key::F(3),
        E::F4 => Key::F(4),
        E::F5 => Key::F(5),
        E::F6 => Key::F(6),
        E::F7 => Key::F(7),
        E::F8 => Key::F(8),
        E::F9 => Key::F(9),
        E::F10 => Key::F(10),
        E::F11 => Key::F(11),
        E::F12 => Key::F(12),
        other => {
            let name = other.name();
            let mut chars = name.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) if c.is_ascii_alphabetic() => Key::Char(c.to_ascii_lowercase()),
                _ => return None,
            }
        }
    })
}

/// Builds the chord for a key event.
///
/// The physical key is preferred so that `Ctrl+Shift+[` and `Ctrl+=` work regardless
/// of what the shifted character is on the active layout.
pub fn chord(
    key: egui::Key,
    physical_key: Option<egui::Key>,
    modifiers: egui::Modifiers,
    os: Os,
) -> Option<KeyChord> {
    let k = physical_key.and_then(map_key).or_else(|| map_key(key))?;
    Some(KeyChord {
        ctrl: modifiers.command,
        shift: modifiers.shift,
        alt: modifiers.alt,
        mac_ctrl: os == Os::Mac && modifiers.ctrl,
        key: k,
    })
}

/// The engine OS for an egui OS value.
pub fn os_of(os: egui::os::OperatingSystem) -> Os {
    match os {
        egui::os::OperatingSystem::Mac | egui::os::OperatingSystem::IOS => Os::Mac,
        egui::os::OperatingSystem::Windows => Os::Windows,
        _ => Os::Linux,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_letters_and_punctuation() {
        assert_eq!(map_key(egui::Key::K), Some(Key::Char('k')));
        assert_eq!(map_key(egui::Key::OpenCurlyBracket), Some(Key::Char('[')));
        assert_eq!(map_key(egui::Key::F11), Some(Key::F(11)));
        let m = egui::Modifiers {
            command: true,
            ctrl: true,
            shift: true,
            ..Default::default()
        };
        let c = chord(egui::Key::Plus, Some(egui::Key::Equals), m, Os::Linux).unwrap();
        assert_eq!(c, "Ctrl+Shift+=".parse().unwrap());
    }
}
