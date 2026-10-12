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

/// The family for a combination of attributes.
pub fn family(mono: bool, bold: bool, italic: bool) -> FontFamily {
    let name = match (mono, bold, italic) {
        (false, false, false) => BODY,
        (false, true, false) => BODY_BOLD,
        (false, false, true) => BODY_ITALIC,
        (false, true, true) => BODY_BOLD_ITALIC,
        (true, false, false) => MONO,
        (true, true, false) => MONO_BOLD,
        (true, false, true) => MONO_ITALIC,
        (true, true, true) => MONO_BOLD_ITALIC,
    };
    FontFamily::Name(name.into())
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
