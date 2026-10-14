//! Drawing primitives: a scene of filled paths, text as glyph outlines.

use std::sync::RwLock;

use tiny_skia_path::{Path, PathBuilder, Rect, Stroke, StrokeDash, Transform};
use ttf_parser::{Face, GlyphId, OutlineBuilder};

/// An RGBA color.
pub type Rgba = [u8; 4];

/// A filled path.
#[derive(Debug, Clone, PartialEq)]
pub struct Fill {
    /// Path (px, y down).
    pub path: Path,
    /// Color.
    pub color: Rgba,
}

/// A rendered diagram.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Scene {
    /// Width in px.
    pub width: f32,
    /// Height in px.
    pub height: f32,
    /// Shapes in paint order.
    pub fills: Vec<Fill>,
}

impl Scene {
    /// Fills `path`.
    pub fn fill(&mut self, path: Option<Path>, color: Rgba) {
        if let Some(path) = path {
            self.fills.push(Fill { path, color });
        }
    }

    /// Strokes `path` (optionally dashed).
    pub fn stroke(
        &mut self,
        path: Option<Path>,
        color: Rgba,
        width: f32,
        dash: Option<(f32, f32)>,
    ) {
        let Some(path) = path else { return };
        let path = match dash {
            Some((on, off)) => {
                // At most ~300 dashes per stroke (a dotted edge of 1800 px);
                // longer strokes stretch the pattern. Edges across a huge
                // (fuzzed) diagram would otherwise make millions of segments.
                let b = path.bounds();
                let k = ((b.width() + b.height()) / (on + off) / 300.0).max(1.0);
                let (on, off) = (on * k, off * k);
                match StrokeDash::new(vec![on, off], 0.0).and_then(|d| path.dash(&d, 1.0)) {
                    Some(p) => p,
                    None => path,
                }
            }
            None => path,
        };
        let st = Stroke {
            width,
            line_cap: tiny_skia_path::LineCap::Butt,
            line_join: tiny_skia_path::LineJoin::Round,
            ..Default::default()
        };
        if let Some(p) = path.stroke(&st, 1.0) {
            self.fills.push(Fill { path: p, color });
        }
    }

    /// Moves everything by `(dx, dy)`.
    pub fn translate(&mut self, dx: f32, dy: f32) {
        for f in &mut self.fills {
            if let Some(p) = f.path.clone().transform(Transform::from_translate(dx, dy)) {
                f.path = p;
            }
        }
    }
}

/// A rectangle path with rounded corners.
pub fn rounded_rect(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<Path> {
    let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
    if r <= 0.01 {
        return Rect::from_xywh(x, y, w.max(0.1), h.max(0.1)).map(PathBuilder::from_rect);
    }
    let k = 0.552_284_8 * r;
    let mut b = PathBuilder::new();
    b.move_to(x + r, y);
    b.line_to(x + w - r, y);
    b.cubic_to(x + w - r + k, y, x + w, y + r - k, x + w, y + r);
    b.line_to(x + w, y + h - r);
    b.cubic_to(x + w, y + h - r + k, x + w - r + k, y + h, x + w - r, y + h);
    b.line_to(x + r, y + h);
    b.cubic_to(x + r - k, y + h, x, y + h - r + k, x, y + h - r);
    b.line_to(x, y + r);
    b.cubic_to(x, y + r - k, x + r - k, y, x + r, y);
    b.close();
    b.finish()
}

/// An ellipse path.
pub fn ellipse(cx: f32, cy: f32, rx: f32, ry: f32) -> Option<Path> {
    Rect::from_xywh(cx - rx, cy - ry, 2.0 * rx, 2.0 * ry).and_then(PathBuilder::from_oval)
}

/// A closed polygon.
pub fn polygon(pts: &[(f32, f32)]) -> Option<Path> {
    let mut b = PathBuilder::new();
    let (first, rest) = pts.split_first()?;
    b.move_to(first.0, first.1);
    for p in rest {
        b.line_to(p.0, p.1);
    }
    b.close();
    b.finish()
}

/// An open polyline.
pub fn polyline(pts: &[(f32, f32)]) -> Option<Path> {
    let mut b = PathBuilder::new();
    let (first, rest) = pts.split_first()?;
    b.move_to(first.0, first.1);
    for p in rest {
        b.line_to(p.0, p.1);
    }
    b.finish()
}

/// A smooth curve through `pts` (Catmull-Rom converted to cubic Béziers).
pub fn smooth(pts: &[(f32, f32)]) -> Option<Path> {
    if pts.len() < 3 {
        return polyline(pts);
    }
    let mut b = PathBuilder::new();
    b.move_to(pts[0].0, pts[0].1);
    for i in 0..pts.len() - 1 {
        let p0 = if i == 0 { pts[0] } else { pts[i - 1] };
        let p1 = pts[i];
        let p2 = pts[i + 1];
        let p3 = if i + 2 < pts.len() { pts[i + 2] } else { p2 };
        let c1 = (p1.0 + (p2.0 - p0.0) / 6.0, p1.1 + (p2.1 - p0.1) / 6.0);
        let c2 = (p2.0 - (p3.0 - p1.0) / 6.0, p2.1 - (p3.1 - p1.1) / 6.0);
        b.cubic_to(c1.0, c1.1, c2.0, c2.1, p2.0, p2.1);
    }
    b.finish()
}

/// A filled arrowhead at `tip` pointing away from `from`.
pub fn arrowhead(from: (f32, f32), tip: (f32, f32), size: f32) -> Option<Path> {
    let (dx, dy) = (tip.0 - from.0, tip.1 - from.1);
    let len = (dx * dx + dy * dy).sqrt().max(0.001);
    let (ux, uy) = (dx / len, dy / len);
    let base = (tip.0 - ux * size, tip.1 - uy * size);
    let half = size * 0.45;
    polygon(&[
        tip,
        (base.0 - uy * half, base.1 + ux * half),
        (base.0 + uy * half, base.1 - ux * half),
    ])
}

// ---- text ----

static FONTS: RwLock<Vec<(&'static [u8], u32)>> = RwLock::new(Vec::new());

/// Sets the fonts used for labels (first: Latin, then fallbacks such as CJK).
/// Data must live for the whole program.
pub fn set_fonts(fonts: Vec<(&'static [u8], u32)>) {
    if let Ok(mut f) = FONTS.write() {
        *f = fonts;
    }
}

fn faces() -> Vec<Face<'static>> {
    FONTS
        .read()
        .map(|f| {
            f.iter()
                .filter_map(|(d, i)| Face::parse(d, *i).ok())
                .collect()
        })
        .unwrap_or_default()
}

struct Outline<'a> {
    b: &'a mut PathBuilder,
    s: f32,
    x: f32,
    y: f32,
}

impl OutlineBuilder for Outline<'_> {
    fn move_to(&mut self, x: f32, y: f32) {
        self.b.move_to(self.x + x * self.s, self.y - y * self.s);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.b.line_to(self.x + x * self.s, self.y - y * self.s);
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.b.quad_to(
            self.x + x1 * self.s,
            self.y - y1 * self.s,
            self.x + x * self.s,
            self.y - y * self.s,
        );
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.b.cubic_to(
            self.x + x1 * self.s,
            self.y - y1 * self.s,
            self.x + x2 * self.s,
            self.y - y2 * self.s,
            self.x + x * self.s,
            self.y - y * self.s,
        );
    }
    fn close(&mut self) {
        self.b.close();
    }
}

fn glyph(faces: &[Face<'static>], c: char) -> Option<(usize, GlyphId)> {
    faces
        .iter()
        .enumerate()
        .find_map(|(i, f)| f.glyph_index(c).filter(|g| g.0 != 0).map(|g| (i, g)))
}

/// Advance width of `s` at `size` px.
pub fn text_width(s: &str, size: f32) -> f32 {
    let faces = faces();
    s.chars()
        .map(|c| match glyph(&faces, c) {
            Some((i, g)) => {
                faces[i].glyph_hor_advance(g).map_or(0.0, f32::from) * size
                    / f32::from(faces[i].units_per_em())
            }
            None => size * if (c as u32) > 0x2E80 { 1.0 } else { 0.55 },
        })
        .sum()
}

/// Wraps `s` into lines no wider than `max` (at spaces, or anywhere in CJK);
/// `<br>` and `\n` force breaks.
pub fn wrap(s: &str, size: f32, max: f32) -> Vec<String> {
    let s = s
        .replace("<br>", "\n")
        .replace("<br/>", "\n")
        .replace("<br />", "\n");
    let mut out = Vec::new();
    for para in s.split('\n') {
        let mut line = String::new();
        for word in split_words(para) {
            let candidate = format!("{line}{word}");
            if !line.is_empty() && text_width(candidate.trim_end(), size) > max {
                out.push(line.trim_end().to_owned());
                line = word.trim_start().to_owned();
            } else {
                line = candidate;
            }
        }
        out.push(line.trim_end().to_owned());
    }
    out
}

fn split_words(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for c in s.chars() {
        let cjk = (c as u32) >= 0x2E80;
        if cjk {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            out.push(c.to_string());
        } else if c == ' ' {
            cur.push(c);
            out.push(std::mem::take(&mut cur));
        } else {
            cur.push(c);
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Draws `s` with its baseline at `(x, y)`.
pub fn draw_text(scene: &mut Scene, s: &str, x: f32, y: f32, size: f32, color: Rgba) {
    let faces = faces();
    let mut b = PathBuilder::new();
    let mut pen = x;
    for c in s.chars() {
        match glyph(&faces, c) {
            Some((i, g)) => {
                let f = &faces[i];
                let sc = size / f32::from(f.units_per_em());
                let mut o = Outline {
                    b: &mut b,
                    s: sc,
                    x: pen,
                    y,
                };
                let _ = f.outline_glyph(g, &mut o);
                pen += f.glyph_hor_advance(g).map_or(0.0, f32::from) * sc;
            }
            None => pen += size * 0.55,
        }
    }
    scene.fill(b.finish(), color);
}

/// Draws lines of text centered on `cx`, the block vertically centered on `cy`.
pub fn draw_lines_centered(
    scene: &mut Scene,
    lines: &[String],
    cx: f32,
    cy: f32,
    size: f32,
    color: Rgba,
) {
    let lh = size * 1.3;
    let total = lh * lines.len() as f32;
    let mut y = cy - total / 2.0 + size * 0.95;
    for l in lines {
        let w = text_width(l, size);
        draw_text(scene, l, cx - w / 2.0, y, size, color);
        y += lh;
    }
}

/// Size of wrapped lines.
pub fn lines_size(lines: &[String], size: f32) -> (f32, f32) {
    let w = lines
        .iter()
        .map(|l| text_width(l, size))
        .fold(0.0, f32::max);
    (w, size * 1.3 * lines.len() as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dashes_per_stroke_are_bounded() {
        // A dotted edge across a million pixels (a fuzzed diagram) once made
        // ~170 000 dashes; the pattern now stretches beyond ~300.
        let mut sc = Scene {
            width: 1e6,
            height: 10.0,
            fills: Vec::new(),
        };
        sc.stroke(
            polyline(&[(0.0, 5.0), (1e6, 5.0)]),
            [0, 0, 0, 255],
            1.4,
            Some((3.0, 3.0)),
        );
        let points: usize = sc.fills.iter().map(|f| f.path.len()).sum();
        assert!(points > 0 && points < 5_000, "{points} points");
        // Short dotted lines keep their pattern.
        let mut short = Scene {
            width: 100.0,
            height: 10.0,
            fills: Vec::new(),
        };
        short.stroke(
            polyline(&[(0.0, 5.0), (60.0, 5.0)]),
            [0, 0, 0, 255],
            1.4,
            Some((3.0, 3.0)),
        );
        assert!(short.fills[0].path.len() >= 10 * 4);
    }
}
