//! Fonts: the bundled STIX Two Math face (OpenType MATH table) plus optional
//! text fallback faces (CJK in `\text{…}`), glyph outlines and a path cache.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock, RwLock};

use tiny_skia_path::{Path, PathBuilder};
use ttf_parser::{Face, GlyphId, OutlineBuilder};

/// STIX Two Math (SIL OFL 1.1).
pub static MATH_FONT: &[u8] = include_bytes!("../../../assets/fonts/STIXTwoMath-Regular.otf");

static MATH_FACE: OnceLock<Option<Face<'static>>> = OnceLock::new();
static FALLBACKS: RwLock<Vec<(&'static [u8], u32)>> = RwLock::new(Vec::new());
type OutlineCache = Mutex<HashMap<(u8, u16), Option<Path>>>;
static OUTLINES: OnceLock<OutlineCache> = OnceLock::new();

/// The math face.
pub fn math_face() -> Option<&'static Face<'static>> {
    MATH_FACE
        .get_or_init(|| Face::parse(MATH_FONT, 0).ok())
        .as_ref()
}

/// Sets fonts used for characters the math font lacks (e.g. CJK in `\text`).
/// Call once at startup; data must live for the program.
pub fn set_fallback_fonts(fonts: Vec<(&'static [u8], u32)>) {
    if let Ok(mut f) = FALLBACKS.write() {
        *f = fonts;
    }
    if let Some(c) = OUTLINES.get()
        && let Ok(mut c) = c.lock()
    {
        c.retain(|k, _| k.0 == 0);
    }
}

/// Face `i` (0 = math, 1.. = fallbacks).
pub fn face(i: u8) -> Option<Face<'static>> {
    if i == 0 {
        return math_face().cloned();
    }
    let (data, index) = FALLBACKS.read().ok()?.get(i as usize - 1).copied()?;
    Face::parse(data, index).ok()
}

/// The face and glyph that render `c`, preferring the math face.
pub fn lookup(c: char) -> Option<(u8, GlyphId)> {
    if let Some(f) = math_face()
        && let Some(g) = f.glyph_index(c)
    {
        return Some((0, g));
    }
    let n = FALLBACKS.read().map(|f| f.len()).unwrap_or(0);
    for i in 0..n {
        if let Some(f) = face(i as u8 + 1)
            && let Some(g) = f.glyph_index(c)
        {
            return Some((i as u8 + 1, g));
        }
    }
    None
}

struct Builder(PathBuilder);

impl OutlineBuilder for Builder {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.move_to(x, -y);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.0.line_to(x, -y);
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.0.quad_to(x1, -y1, x, -y);
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.0.cubic_to(x1, -y1, x2, -y2, x, -y);
    }
    fn close(&mut self) {
        self.0.close();
    }
}

/// The outline of a glyph in font units, y pointing down (cached).
pub fn outline(face_id: u8, gid: GlyphId) -> Option<Path> {
    let cache = OUTLINES.get_or_init(Default::default);
    if let Ok(c) = cache.lock()
        && let Some(p) = c.get(&(face_id, gid.0))
    {
        return p.clone();
    }
    let f = face(face_id)?;
    let mut b = Builder(PathBuilder::new());
    let p = f.outline_glyph(gid, &mut b).and_then(|_| b.0.finish());
    if let Ok(mut c) = cache.lock() {
        if c.len() > 20_000 {
            c.clear();
        }
        c.insert((face_id, gid.0), p.clone());
    }
    p
}

/// Units per em of a face.
pub fn upem(face_id: u8) -> f32 {
    face(face_id).map_or(1000.0, |f| f32::from(f.units_per_em()))
}
