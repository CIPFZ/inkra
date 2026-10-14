//! Bundled fonts (TH-006, OFL) and font families used by document layout.

use std::path::Path;
use std::sync::Arc;

use epaint::text::{FontData, FontDefinitions, FontFamily, FontTweak};

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

/// A typeface: bundled, or an installed system family (TH-006).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum FontKind {
    /// Inter.
    #[default]
    Sans,
    /// Source Serif 4.
    Serif,
    /// JetBrains Mono.
    Mono,
    /// System family registered with [`system_family`] (slot index).
    System(u8),
}

/// Most system families one session uses.
const MAX_SYSTEM: usize = 8;

/// Family names of the system slots: regular, bold, italic, bold italic.
const SYSTEM_NAMES: [[&str; 4]; MAX_SYSTEM] = [
    [
        "inkra-sys0",
        "inkra-sys0-bold",
        "inkra-sys0-italic",
        "inkra-sys0-bold-italic",
    ],
    [
        "inkra-sys1",
        "inkra-sys1-bold",
        "inkra-sys1-italic",
        "inkra-sys1-bold-italic",
    ],
    [
        "inkra-sys2",
        "inkra-sys2-bold",
        "inkra-sys2-italic",
        "inkra-sys2-bold-italic",
    ],
    [
        "inkra-sys3",
        "inkra-sys3-bold",
        "inkra-sys3-italic",
        "inkra-sys3-bold-italic",
    ],
    [
        "inkra-sys4",
        "inkra-sys4-bold",
        "inkra-sys4-italic",
        "inkra-sys4-bold-italic",
    ],
    [
        "inkra-sys5",
        "inkra-sys5-bold",
        "inkra-sys5-italic",
        "inkra-sys5-bold-italic",
    ],
    [
        "inkra-sys6",
        "inkra-sys6-bold",
        "inkra-sys6-italic",
        "inkra-sys6-bold-italic",
    ],
    [
        "inkra-sys7",
        "inkra-sys7-bold",
        "inkra-sys7-italic",
        "inkra-sys7-bold-italic",
    ],
];

/// A loaded system family: its name, the bundled kind it falls back to, and
/// the faces (regular, bold, italic, bold italic) that exist.
struct SystemFamily {
    name: String,
    fallback: FontKind,
    faces: [Option<(&'static [u8], u32)>; 4],
}

static SYSTEM: std::sync::RwLock<Vec<SystemFamily>> = std::sync::RwLock::new(Vec::new());
static SYSTEM_GEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Changes when a system family is added (the UI then reloads its fonts).
pub fn generation() -> u64 {
    SYSTEM_GEN.load(std::sync::atomic::Ordering::SeqCst)
}

fn font_db() -> &'static fontdb::Database {
    static DB: std::sync::OnceLock<fontdb::Database> = std::sync::OnceLock::new();
    DB.get_or_init(|| {
        let t = std::time::Instant::now();
        let mut db = fontdb::Database::new();
        db.load_system_fonts();
        log::info!(
            "system fonts: {} faces in {:.0} ms",
            db.len(),
            t.elapsed().as_secs_f64() * 1e3
        );
        db
    })
}

/// Names that always mean a bundled face (or a generic family).
fn bundled_name(n: &str) -> Option<FontKind> {
    match n.trim().to_ascii_lowercase().as_str() {
        "inter" | "sans" | "sans-serif" | "system-ui" | "-apple-system" | "ui-sans-serif" => {
            Some(FontKind::Sans)
        }
        "source serif 4" | "source serif" | "serif" | "ui-serif" => Some(FontKind::Serif),
        "jetbrains mono" | "mono" | "monospace" | "ui-monospace" => Some(FontKind::Mono),
        _ => None,
    }
}

/// The typeface for a theme's font name (TH-006): bundled and generic names map
/// to the bundled faces; other names are looked up among the installed fonts.
pub fn resolve_name(name: &str) -> Option<FontKind> {
    if let Some(k) = bundled_name(name) {
        return Some(k);
    }
    system_family(name).map(FontKind::System)
}

/// Registers the installed family `name` (case-insensitive); `None` when it is
/// not installed or every slot is taken.
pub fn system_family(name: &str) -> Option<u8> {
    let want = name.trim();
    if let Ok(list) = SYSTEM.read()
        && let Some(k) = list.iter().position(|f| f.name.eq_ignore_ascii_case(want))
    {
        return Some(k as u8);
    }
    let db = font_db();
    let family = db
        .faces()
        .flat_map(|f| f.families.iter().map(|(n, _)| n.clone()))
        .find(|n| n.eq_ignore_ascii_case(want))?;
    let face = |weight: u16, style: fontdb::Style| -> Option<(&'static [u8], u32)> {
        let q = fontdb::Query {
            families: &[fontdb::Family::Name(&family)],
            weight: fontdb::Weight(weight),
            style,
            ..Default::default()
        };
        let id = db.query(&q)?;
        let info = db.face(id)?;
        // A query falls back to the nearest face; keep only true matches.
        let bold_ok = (weight >= 600) == (info.weight.0 >= 600);
        let italic_ok = (style == fontdb::Style::Normal) == (info.style == fontdb::Style::Normal);
        if !(bold_ok && italic_ok) && (weight >= 600 || style != fontdb::Style::Normal) {
            return None;
        }
        let (src, index) = db.face_source(id)?;
        let bytes: &'static [u8] = match src {
            fontdb::Source::File(p) => Box::leak(std::fs::read(p).ok()?.into_boxed_slice()),
            fontdb::Source::Binary(b) | fontdb::Source::SharedFile(_, b) => {
                Box::leak((*b).as_ref().to_vec().into_boxed_slice())
            }
        };
        Some((bytes, index))
    };
    let regular = face(400, fontdb::Style::Normal)?;
    let faces = [
        Some(regular),
        face(700, fontdb::Style::Normal),
        face(400, fontdb::Style::Italic),
        face(700, fontdb::Style::Italic),
    ];
    let mono = db
        .query(&fontdb::Query {
            families: &[fontdb::Family::Name(&family)],
            ..Default::default()
        })
        .and_then(|id| db.face(id))
        .is_some_and(|f| f.monospaced);
    let fallback = if mono {
        FontKind::Mono
    } else {
        FontKind::from_name(&family)
            .filter(|k| *k != FontKind::Mono)
            .unwrap_or(FontKind::Sans)
    };
    let mut list = SYSTEM.write().ok()?;
    if list.len() >= MAX_SYSTEM {
        log::warn!("too many system font families; {family} not used");
        return None;
    }
    log::info!("system font family {family} (slot {})", list.len());
    list.push(SystemFamily {
        name: family,
        fallback,
        faces,
    });
    SYSTEM_GEN.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    Some((list.len() - 1) as u8)
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
        (FontKind::System(n), b, i) => {
            SYSTEM_NAMES[usize::from(n) % MAX_SYSTEM][usize::from(b) + 2 * usize::from(i)]
        }
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

/// CJK fonts preferred for a language (TH-006: 简/繁/日/韩 by the system
/// locale), tried before the general list. Noto CJK collections hold every
/// region; the index picks it (0 JP, 1 KR, 2 SC, 3 TC, 4 HK).
fn cjk_for_locale(tag: &str) -> Vec<(&'static str, u32)> {
    let t = tag.to_ascii_lowercase();
    let noto = |i: u32| -> Vec<(&'static str, u32)> {
        [
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/google-noto-sans-cjk-fonts/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
        ]
        .iter()
        .map(|p| (*p, i))
        .collect()
    };
    let mut v: Vec<(&'static str, u32)> = Vec::new();
    if t.starts_with("ja") {
        v.extend([
            ("C:\\Windows\\Fonts\\YuGothM.ttc", 0),
            ("C:\\Windows\\Fonts\\meiryo.ttc", 0),
            ("/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc", 0),
        ]);
        v.extend(noto(0));
    } else if t.starts_with("ko") {
        v.extend([
            ("C:\\Windows\\Fonts\\malgun.ttf", 0),
            ("/System/Library/Fonts/AppleSDGothicNeo.ttc", 0),
        ]);
        v.extend(noto(1));
    } else if t.starts_with("zh-tw")
        || t.starts_with("zh-hk")
        || t.starts_with("zh-mo")
        || t.contains("hant")
    {
        v.extend([
            ("C:\\Windows\\Fonts\\msjh.ttc", 0),
            ("/System/Library/Fonts/PingFang.ttc", 1),
        ]);
        v.extend(noto(if t.starts_with("zh-hk") { 4 } else { 3 }));
    } else {
        v.extend(noto(2));
    }
    v
}

/// Finds a system CJK font for the system language.
pub fn find_cjk_font() -> Option<(&'static str, u32)> {
    cjk_for_locale(&inkra_i18n::system_language())
        .into_iter()
        .chain(CJK_CANDIDATES.iter().copied())
        .find(|(p, _)| Path::new(p).is_file())
}

/// Whether no CJK font was found (TH-006: the UI then says so).
pub fn cjk_missing() -> bool {
    cjk_font_data().is_none()
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

/// The system CJK font, read once and shared with the math engine (CJK inside
/// `\text{…}`).
pub fn cjk_font_data() -> Option<(&'static [u8], u32)> {
    static DATA: std::sync::OnceLock<Option<(&'static [u8], u32)>> = std::sync::OnceLock::new();
    *DATA.get_or_init(|| {
        let Some((path, index)) = find_cjk_font() else {
            log::warn!("no system CJK font found; CJK text will render as boxes");
            return None;
        };
        match std::fs::read(path) {
            Ok(bytes) => {
                log::info!("CJK fallback font: {path}");
                // Lives for the whole program; leaked once so egui and the math
                // engine share one copy.
                let data: &'static [u8] = Box::leak(bytes.into_boxed_slice());
                inkra_math::font::set_fallback_fonts(vec![(data, index)]);
                Some((data, index))
            }
            Err(e) => {
                log::warn!("cannot read font {path}: {e}");
                None
            }
        }
    })
}

/// The bundled font files: `(family name, data)`.
pub fn bundled() -> &'static [(&'static str, &'static [u8])] {
    FONTS
}

/// Gives the diagram engine the label fonts (Inter, then the CJK fallback).
pub fn init_vector_fonts() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let mut f: Vec<(&'static [u8], u32)> = vec![(
            FONTS
                .iter()
                .find(|(n, _)| *n == BODY)
                .map_or(&[][..], |(_, b)| *b),
            0,
        )];
        if let Some(c) = cjk_font_data() {
            f.push(c);
        }
        inkra_diagram::draw::set_fonts(f);
    });
}

/// Emoji and UI symbol fallbacks with egui's scale tweaks (the two fonts egui
/// bundles by default; its Ubuntu and Hack text faces are not shipped: NF-011
/// allows only OFL text fonts).
const EMOJI: &[(&str, &[u8], f32)] = &[
    (
        "NotoEmoji-Regular",
        include_bytes!("../../../assets/fonts/NotoEmoji-Regular.ttf"),
        0.81,
    ),
    (
        "emoji-icon-font",
        include_bytes!("../../../assets/fonts/emoji-icon-font.ttf"),
        0.90,
    ),
];

/// Font definitions: the emoji fallbacks, the bundled Inter / JetBrains Mono
/// families (also used for the UI) and a system CJK fallback when available.
pub fn definitions() -> FontDefinitions {
    init_vector_fonts();
    let mut defs = FontDefinitions::empty();
    for (name, bytes) in FONTS {
        defs.font_data
            .insert((*name).to_owned(), Arc::new(FontData::from_static(bytes)));
    }
    for (name, bytes, scale) in EMOJI {
        let tweak = FontTweak {
            scale: *scale,
            ..Default::default()
        };
        defs.font_data.insert(
            (*name).to_owned(),
            Arc::new(FontData::from_static(bytes).tweak(tweak)),
        );
    }
    let emoji: Vec<String> = EMOJI.iter().map(|(n, ..)| (*n).to_owned()).collect();
    let mut cjk = Vec::new();
    if let Some((bytes, index)) = cjk_font_data() {
        let mut d = FontData::from_static(bytes);
        d.index = index;
        defs.font_data.insert("cjk".into(), Arc::new(d));
        cjk.push("cjk".to_owned());
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
    // Installed families (TH-006); missing styles use the regular face, then
    // the matching bundled face.
    if let Ok(list) = SYSTEM.read() {
        for (fam, names) in list.iter().zip(SYSTEM_NAMES.iter()) {
            for (v, key) in names.iter().copied().enumerate() {
                let face = fam.faces[v].or(fam.faces[0]);
                if let Some((bytes, index)) = face {
                    let mut d = FontData::from_static(bytes);
                    d.index = index;
                    defs.font_data.insert(key.to_owned(), Arc::new(d));
                }
                let bundled = match family_of(fam.fallback, v & 1 == 1, v & 2 == 2) {
                    FontFamily::Name(s) => s.to_string(),
                    _ => BODY.to_owned(),
                };
                let mut chain = vec![key.to_owned(), bundled];
                chain.extend(cjk.iter().cloned());
                chain.extend(emoji.iter().cloned());
                defs.families.insert(FontFamily::Name(key.into()), chain);
            }
        }
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
