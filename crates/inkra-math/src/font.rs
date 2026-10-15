//! Fonts: the bundled STIX Two Math face (OpenType MATH table) plus optional
//! text fallback faces (CJK in `\text{…}`), glyph metrics, outlines and a path
//! cache. Read with skrifa / read-fonts (T9).

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock, RwLock};

use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::raw::TableProvider;
use skrifa::raw::tables::math::{Math, MathConstants, PartFlags, StretchAxis};
use skrifa::{FontRef, MetadataProvider};
use tiny_skia_path::{Path, PathBuilder};

/// A glyph id within a face.
pub type Gid = u16;

/// STIX Two Math (SIL OFL 1.1).
pub static MATH_FONT: &[u8] = include_bytes!("../../../assets/fonts/STIXTwoMath-Regular.otf");

static MATH_FACE: OnceLock<Option<FontRef<'static>>> = OnceLock::new();
static FALLBACKS: RwLock<Vec<(&'static [u8], u32)>> = RwLock::new(Vec::new());
type OutlineCache = Mutex<HashMap<(u8, u16), Option<Path>>>;
static OUTLINES: OnceLock<OutlineCache> = OnceLock::new();

/// The math face.
fn math_face() -> Option<&'static FontRef<'static>> {
    MATH_FACE
        .get_or_init(|| FontRef::from_index(MATH_FONT, 0).ok())
        .as_ref()
}

/// The MATH table of the math face.
fn math() -> Option<Math<'static>> {
    math_face()?.math().ok()
}

/// The MATH constants of the math face.
pub fn constants() -> Option<MathConstants<'static>> {
    math()?.math_constants().ok()
}

/// The MATH table's minimum overlap of assembly parts, in font units.
pub fn min_connector_overlap() -> Option<u16> {
    Some(
        math()?
            .math_variants()
            .ok()?
            .min_connector_overlap()
            .to_u16(),
    )
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
fn face(i: u8) -> Option<FontRef<'static>> {
    if i == 0 {
        return math_face().cloned();
    }
    let (data, index) = FALLBACKS.read().ok()?.get(i as usize - 1).copied()?;
    FontRef::from_index(data, index).ok()
}

fn glyph_of(f: &FontRef<'_>, c: char) -> Option<Gid> {
    let g = f.charmap().map(c)?;
    u16::try_from(g.to_u32()).ok()
}

/// The face and glyph that render `c`, preferring the math face.
pub fn lookup(c: char) -> Option<(u8, Gid)> {
    if let Some(f) = math_face()
        && let Some(g) = glyph_of(f, c)
    {
        return Some((0, g));
    }
    let n = FALLBACKS.read().map(|f| f.len()).unwrap_or(0);
    for i in 0..n {
        if let Some(f) = face(i as u8 + 1)
            && let Some(g) = glyph_of(&f, c)
        {
            return Some((i as u8 + 1, g));
        }
    }
    None
}

/// Collects a glyph outline as a path (y pointing down) and its extent over
/// all points, control points included.
struct Pen {
    path: PathBuilder,
    bbox: Option<[f32; 4]>,
}

impl Pen {
    fn new() -> Self {
        Self {
            path: PathBuilder::new(),
            bbox: None,
        }
    }

    fn extend(&mut self, x: f32, y: f32) {
        let b = self.bbox.get_or_insert([x, y, x, y]);
        b[0] = b[0].min(x);
        b[1] = b[1].min(y);
        b[2] = b[2].max(x);
        b[3] = b[3].max(y);
    }
}

impl OutlinePen for Pen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.extend(x, y);
        self.path.move_to(x, -y);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.extend(x, y);
        self.path.line_to(x, -y);
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.extend(x1, y1);
        self.extend(x, y);
        self.path.quad_to(x1, -y1, x, -y);
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.extend(x1, y1);
        self.extend(x2, y2);
        self.extend(x, y);
        self.path.cubic_to(x1, -y1, x2, -y2, x, -y);
    }
    fn close(&mut self) {
        self.path.close();
    }
}

fn draw(f: &FontRef<'_>, gid: Gid) -> Option<Pen> {
    let glyph = f.outline_glyphs().get(gid.into())?;
    let mut pen = Pen::new();
    glyph
        .draw(
            DrawSettings::unhinted(Size::unscaled(), LocationRef::default()),
            &mut pen,
        )
        .ok()?;
    Some(pen)
}

/// The outline of a glyph in font units, y pointing down (cached).
pub fn outline(face_id: u8, gid: Gid) -> Option<Path> {
    let cache = OUTLINES.get_or_init(Default::default);
    if let Ok(c) = cache.lock()
        && let Some(p) = c.get(&(face_id, gid))
    {
        return p.clone();
    }
    let p = face(face_id)
        .and_then(|f| draw(&f, gid))
        .and_then(|pen| pen.path.finish());
    if let Ok(mut c) = cache.lock() {
        if c.len() > 20_000 {
            c.clear();
        }
        c.insert((face_id, gid), p.clone());
    }
    p
}

/// Units per em of a face.
pub fn upem(face_id: u8) -> f32 {
    face(face_id)
        .and_then(|f| f.head().ok().map(|h| f32::from(h.units_per_em())))
        .unwrap_or(1000.0)
}

/// Advance width and vertical extent `(y_min, y_max)` of a glyph in font
/// units. The extent covers every outline point (control points included),
/// truncated to integers, like the glyph boxes TeX layout was tuned with.
pub fn metrics(face_id: u8, gid: Gid) -> (Option<f32>, Option<(f32, f32)>) {
    let Some(f) = face(face_id) else {
        return (None, None);
    };
    let adv = f
        .glyph_metrics(Size::unscaled(), LocationRef::default())
        .advance_width(gid.into());
    let ext = draw(&f, gid)
        .and_then(|p| p.bbox)
        .map(|[_, y0, _, y1]| (y0.trunc(), y1.trunc()));
    (adv, ext)
}

/// MATH glyph info of the math face: `(italic correction, top accent
/// attachment)` in font units.
pub fn glyph_info(gid: Gid) -> (Option<i32>, Option<i32>) {
    let Some(info) = math().and_then(|m| m.math_glyph_info().ok()) else {
        return (None, None);
    };
    let g = gid.into();
    let italic = info
        .math_italics_correction_info()
        .and_then(Result::ok)
        .and_then(|t| t.correction(g));
    let accent = info
        .math_top_accent_attachment()
        .and_then(Result::ok)
        .and_then(|t| t.attachment(g));
    (italic, accent)
}

/// One part of a glyph assembly.
#[derive(Debug, Clone, Copy)]
pub struct Part {
    /// The part's glyph.
    pub glyph: Gid,
    /// Full advance along the stretch axis, in font units.
    pub full_advance: u16,
    /// Repeated to reach the size.
    pub extender: bool,
}

/// How a glyph grows along an axis: bigger variants, then an assembly.
#[derive(Debug, Clone, Default)]
pub struct Construction {
    /// Variants from small to large.
    pub variants: Vec<Gid>,
    /// Italic correction of the assembly (font units) and its parts (bottom to
    /// top, or left to right).
    pub assembly: Option<(i32, Vec<Part>)>,
}

/// The vertical (`true`) or horizontal construction of a math-face glyph.
pub fn construction(gid: Gid, vertical: bool) -> Option<Construction> {
    let v = math()?.math_variants().ok()?;
    let axis = if vertical {
        StretchAxis::Vertical
    } else {
        StretchAxis::Horizontal
    };
    let c = v.glyph_construction(gid.into(), axis)?;
    let variants = c
        .math_glyph_variant_records()
        .iter()
        .map(|r| r.variant_glyph().to_u16())
        .collect();
    let assembly = c.glyph_assembly().and_then(Result::ok).map(|a| {
        let parts = a
            .part_records()
            .iter()
            .map(|p| Part {
                glyph: p.glyph_id().to_u16(),
                full_advance: p.full_advance().to_u16(),
                extender: p.part_flags().contains(PartFlags::EXTENDER_FLAG),
            })
            .collect();
        (i32::from(a.italics_correction().value().to_i16()), parts)
    });
    Some(Construction { variants, assembly })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn math_face_has_the_tables() {
        let c = constants().expect("MATH constants");
        assert!(c.axis_height().value().to_i16() > 0);
        assert!(min_connector_overlap().is_some());
        let (_, g) = lookup('(').unwrap();
        let v = construction(g, true).expect("parentheses grow");
        assert!(!v.variants.is_empty() && v.assembly.is_some());
        let (adv, ext) = metrics(0, lookup('x').unwrap().1);
        assert!(adv.unwrap() > 0.0 && ext.unwrap().1 > 0.0);
        assert!(outline(0, lookup('x').unwrap().1).is_some());
        assert!((upem(0) - 1000.0).abs() < 1.0);
    }
}
