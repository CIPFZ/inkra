//! Font setup.
//!
//! egui's bundled fonts have no CJK glyphs. Until the full fallback system of TH-006
//! lands, we append the first system CJK font found at a well-known path to both
//! font families (SPEC appendix D-1: rely on system fonts).

use std::path::Path;

use egui::{FontData, FontDefinitions, FontFamily};

/// Well-known CJK font locations: (path, collection index).
const CJK_CANDIDATES: &[(&str, u32)] = &[
    // Windows
    ("C:\\Windows\\Fonts\\msyh.ttc", 0),
    ("C:\\Windows\\Fonts\\msyh.ttf", 0),
    ("C:\\Windows\\Fonts\\simsun.ttc", 0),
    ("C:\\Windows\\Fonts\\YuGothM.ttc", 0),
    ("C:\\Windows\\Fonts\\malgun.ttf", 0),
    // macOS
    ("/System/Library/Fonts/PingFang.ttc", 0),
    ("/System/Library/Fonts/STHeiti Light.ttc", 0),
    ("/System/Library/Fonts/Hiragino Sans GB.ttc", 0),
    ("/Library/Fonts/Arial Unicode.ttf", 0),
    // Linux
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
    (
        "/usr/share/fonts/truetype/droid/DroidSansFallbackFull.ttf",
        0,
    ),
];

/// Finds a system CJK font.
pub fn find_cjk_font() -> Option<(&'static str, u32)> {
    CJK_CANDIDATES
        .iter()
        .copied()
        .find(|(p, _)| Path::new(p).is_file())
}

/// Installs egui's default fonts plus a system CJK fallback if one is found.
pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    if let Some((path, index)) = find_cjk_font() {
        match std::fs::read(path) {
            Ok(bytes) => {
                let mut data = FontData::from_owned(bytes);
                data.index = index;
                fonts.font_data.insert("cjk".to_owned(), data.into());
                for family in [FontFamily::Proportional, FontFamily::Monospace] {
                    fonts
                        .families
                        .entry(family)
                        .or_default()
                        .push("cjk".to_owned());
                }
                log::info!("CJK fallback font: {path}");
            }
            Err(e) => log::warn!("cannot read {path}: {e}"),
        }
    } else {
        log::warn!("no system CJK font found; CJK text will render as boxes");
    }
    ctx.set_fonts(fonts);
}
