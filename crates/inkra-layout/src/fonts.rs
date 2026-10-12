//! Bundled fonts (TH-006, OFL) and font families used by document layout.

use std::path::Path;
use std::sync::Arc;

use epaint::text::{FontData, FontDefinitions, FontFamily};

/// Body text (Inter).
pub const BODY: &str = "inkra-body";
/// Body bold.
pub const BODY_BOLD: &str = "inkra-body-bold";
/// Body italic.
pub const BODY_ITALIC: &str = "inkra-body-italic";
/// Body bold italic.
pub const BODY_BOLD_ITALIC: &str = "inkra-body-bold-italic";
/// Serif body text (Source Serif 4).
pub const SERIF: &str = "inkra-serif";
/// Serif bold.
pub const SERIF_BOLD: &str = "inkra-serif-bold";
/// Serif italic.
pub const SERIF_ITALIC: &str = "inkra-serif-italic";
/// Serif bold italic.
pub const SERIF_BOLD_ITALIC: &str = "inkra-serif-bold-italic";
/// Monospace (JetBrains Mono).
pub const MONO: &str = "inkra-mono";
/// Monospace bold.
pub const MONO_BOLD: &str = "inkra-mono-bold";
/// Monospace italic.
pub const MONO_ITALIC: &str = "inkra-mono-italic";
/// Monospace bold italic.
pub const MONO_BOLD_ITALIC: &str = "inkra-mono-bold-italic";

const FONTS: &[(&str, &[u8])] = &[
    (
        SERIF,
        include_bytes!("../../../assets/fonts/SourceSerif4-Regular.ttf"),
    ),
    (
        SERIF_BOLD,
        include_bytes!("../../../assets/fonts/SourceSerif4-Bold.ttf"),
    ),
    (
        SERIF_ITALIC,
        include_bytes!("../../../assets/fonts/SourceSerif4-It.ttf"),
    ),
    (
        SERIF_BOLD_ITALIC,
        include_bytes!("../../../assets/fonts/SourceSerif4-BoldIt.ttf"),
    ),
    (
        BODY,
        include_bytes!("../../../assets/fonts/Inter-Regular.ttf"),
    ),
    (
        BODY_BOLD,
        include_bytes!("../../../assets/fonts/Inter-Bold.ttf"),
    ),
    (
        BODY_ITALIC,
        include_bytes!("../../../assets/fonts/Inter-Italic.ttf"),
    ),
    (
        BODY_BOLD_ITALIC,
        include_bytes!("../../../assets/fonts/Inter-BoldItalic.ttf"),
    ),
    (
        MONO,
        include_bytes!("../../../assets/fonts/JetBrainsMono-Regular.ttf"),
    ),
    (
        MONO_BOLD,
        include_bytes!("../../../assets/fonts/JetBrainsMono-Bold.ttf"),
    ),
    (
        MONO_ITALIC,
        include_bytes!("../../../assets/fonts/JetBrainsMono-Italic.ttf"),
    ),
    (
        MONO_BOLD_ITALIC,
        include_bytes!("../../../assets/fonts/JetBrainsMono-BoldItalic.ttf"),
    ),
];

/// A bundled typeface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum FontKind {
    /// Inter.
    #[default]
    Sans,
    /// Source Serif 4.
    Serif,
    /// JetBrains Mono.
    Mono,
}

impl FontKind {
    /// Maps a font name from a theme's fallback chain to a bundled face
    /// (TH-001); unknown names return `None` so the next entry is tried.
    pub fn from_name(name: &str) -> Option<Self> {
        let n = name.trim().to_ascii_lowercase();
        let serif = [
            "serif",
            "source serif",
            "georgia",
            "times",
            "noto serif",
            "literata",
            "merriweather",
            "charter",
            "garamond",
            "songti",
            "宋体",
        ];
        let mono = [
            "mono",
            "monospace",
            "code",
            "consolas",
            "menlo",
            "courier",
            "sarasa",
            "iosevka",
            "fira code",
        ];
        let sans = [
            "sans",
            "inter",
            "system-ui",
            "helvetica",
            "arial",
            "segoe",
            "noto sans",
            "pingfang",
            "roboto",
            "-apple-system",
        ];
        if mono.iter().any(|m| n.contains(m)) {
            Some(Self::Mono)
        } else if serif.iter().any(|m| n.contains(m)) && !n.contains("sans") {
            Some(Self::Serif)
        } else if sans.iter().any(|m| n.contains(m)) {
            Some(Self::Sans)
        } else {
            None
        }
    }
}

/// The family for a typeface and style.
pub fn family_of(kind: FontKind, bold: bool, italic: bool) -> FontFamily {
    let name = match (kind, bold, italic) {
        (FontKind::Sans, false, false) => BODY,
        (FontKind::Sans, true, false) => BODY_BOLD,
        (FontKind::Sans, false, true) => BODY_ITALIC,
        (FontKind::Sans, true, true) => BODY_BOLD_ITALIC,
        (FontKind::Serif, false, false) => SERIF,
        (FontKind::Serif, true, false) => SERIF_BOLD,
        (FontKind::Serif, false, true) => SERIF_ITALIC,
        (FontKind::Serif, true, true) => SERIF_BOLD_ITALIC,
        (FontKind::Mono, false, false) => MONO,
        (FontKind::Mono, true, false) => MONO_BOLD,
        (FontKind::Mono, false, true) => MONO_ITALIC,
        (FontKind::Mono, true, true) => MONO_BOLD_ITALIC,
    };
    FontFamily::Name(name.into())
}

/// The family for a combination of attributes (sans or mono).
pub fn family(mono: bool, bold: bool, italic: bool) -> FontFamily {
    family_of(
        if mono { FontKind::Mono } else { FontKind::Sans },
        bold,
        italic,
    )
}

/// Well-known CJK font locations: (path, collection index). SPEC appendix D-1:
/// Inkra relies on system CJK fonts.
const CJK_CANDIDATES: &[(&str, u32)] = &[
    ("C:\\Windows\\Fonts\\msyh.ttc", 0),
    ("C:\\Windows\\Fonts\\msyh.ttf", 0),
    ("C:\\Windows\\Fonts\\simsun.ttc", 0),
    ("C:\\Windows\\Fonts\\YuGothM.ttc", 0),
    ("C:\\Windows\\Fonts\\malgun.ttf", 0),
    ("/System/Library/Fonts/PingFang.ttc", 0),
    ("/System/Library/Fonts/STHeiti Light.ttc", 0),
    ("/System/Library/Fonts/Hiragino Sans GB.ttc", 0),
    ("/Library/Fonts/Arial Unicode.ttf", 0),
    ("/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc", 0),
    ("/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc", 0),
    (
        "/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc",
        0,
    ),
    (
        "/usr/share/fonts/google-noto-sans-cjk-fonts/NotoSansCJK-Regular.ttc",
        0,
    ),
    ("/usr/share/fonts/noto/NotoSansCJK-Regular.ttc", 0),
    ("/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc", 0),
    (
        "/usr/share/fonts/wenquanyi/wqy-microhei/wqy-microhei.ttc",
        0,
    ),
    ("/usr/share/fonts/truetype/wqy/wqy-microhei.ttc", 0),
    ("/usr/share/fonts/wqy-microhei/wqy-microhei.ttc", 0),
    ("/usr/share/fonts/wqy-zenhei/wqy-zenhei.ttc", 0),
    (
        "/usr/share/fonts/truetype/droid/DroidSansFallbackFull.ttf",
        0,
    ),
];

/// Bold CJK companions of the regular candidates.
const CJK_BOLD: &[(&str, u32)] = &[
    ("C:\\Windows\\Fonts\\msyhbd.ttc", 0),
    ("C:\\Windows\\Fonts\\msyhbd.ttf", 0),
];

/// Finds a system CJK font.
pub fn find_cjk_font() -> Option<(&'static str, u32)> {
    CJK_CANDIDATES
        .iter()
        .copied()
        .find(|(p, _)| Path::new(p).is_file())
}

fn load(path: &str, index: u32) -> Option<FontData> {
    match std::fs::read(path) {
        Ok(bytes) => {
            let mut d = FontData::from_owned(bytes);
            d.index = index;
            Some(d)
        }
        Err(e) => {
            log::warn!("cannot read font {path}: {e}");
            None
        }
    }
}

/// Font definitions: egui's emoji fallbacks, the bundled Inter / JetBrains Mono
/// families (also used for the UI) and a system CJK fallback when available.
pub fn definitions() -> FontDefinitions {
    let mut defs = FontDefinitions::default();
    for (name, bytes) in FONTS {
        defs.font_data
            .insert((*name).to_owned(), Arc::new(FontData::from_static(bytes)));
    }
    let emoji: Vec<String> = ["NotoEmoji-Regular", "emoji-icon-font"]
        .iter()
        .filter(|n| defs.font_data.contains_key(**n))
        .map(|s| (*s).to_owned())
        .collect();
    let mut cjk = Vec::new();
    if let Some((path, index)) = find_cjk_font() {
        if let Some(d) = load(path, index) {
            defs.font_data.insert("cjk".into(), Arc::new(d));
            cjk.push("cjk".to_owned());
            log::info!("CJK fallback font: {path}");
        }
    } else {
        log::warn!("no system CJK font found; CJK text will render as boxes");
    }
    let mut cjk_bold = Vec::new();
    if let Some((path, index)) = CJK_BOLD
        .iter()
        .copied()
        .find(|(p, _)| Path::new(p).is_file())
        && let Some(d) = load(path, index)
    {
        defs.font_data.insert("cjk-bold".into(), Arc::new(d));
        cjk_bold.push("cjk-bold".to_owned());
    }
    for (name, _) in FONTS {
        let bold = name.contains("bold");
        let mut chain = vec![(*name).to_owned()];
        if bold && !cjk_bold.is_empty() {
            chain.extend(cjk_bold.iter().cloned());
        }
        chain.extend(cjk.iter().cloned());
        chain.extend(emoji.iter().cloned());
        defs.families
            .insert(FontFamily::Name((*name).into()), chain);
    }
    // The UI uses the same faces.
    let mut ui = vec![BODY.to_owned()];
    ui.extend(cjk.iter().cloned());
    ui.extend(emoji.iter().cloned());
    defs.families.insert(FontFamily::Proportional, ui);
    let mut ui_mono = vec![MONO.to_owned()];
    ui_mono.extend(cjk.iter().cloned());
    ui_mono.extend(emoji.iter().cloned());
    defs.families.insert(FontFamily::Monospace, ui_mono);
    defs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_family_is_bound() {
        let defs = definitions();
        for kind in [FontKind::Sans, FontKind::Serif, FontKind::Mono] {
            for bold in [false, true] {
                for italic in [false, true] {
                    let fam = family_of(kind, bold, italic);
                    let chain = defs
                        .families
                        .get(&fam)
                        .unwrap_or_else(|| panic!("{fam:?} unbound"));
                    assert!(defs.font_data.contains_key(&chain[0]), "{fam:?}");
                }
            }
        }
        assert_eq!(FontKind::from_name("Source Serif 4"), Some(FontKind::Serif));
        assert_eq!(
            FontKind::from_name("Noto Sans CJK SC"),
            Some(FontKind::Sans)
        );
        assert_eq!(FontKind::from_name("JetBrains Mono"), Some(FontKind::Mono));
        assert_eq!(FontKind::from_name("Comic Neue"), None);
    }
}
