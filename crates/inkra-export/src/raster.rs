//! Image export (EX-003): the whole document as one long PNG, or each PDF page
//! as a PNG/JPEG, at 1x–3x; drawn from the same display list as PDF.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use inkra_layout::epaint::text::{Fonts, TextOptions};
use inkra_layout::epaint::{Color32, Rect};
use tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, Stroke, Transform};

use crate::display::{FaceTable, Op};
use crate::pdf::{ExportImages, Laid, PdfExport, op_y, paginate, prepare};

/// Image export options.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageExport {
    /// Theme, parse options and (for pages) the page setup.
    pub pdf: PdfExport,
    /// Resolution multiplier (1–3).
    pub scale: f32,
    /// Background color (`None`: the theme's).
    pub background: Option<Color32>,
    /// Content width in layout px for the long image.
    pub width: f32,
    /// One image per page instead of one long image.
    pub paged: bool,
    /// JPEG instead of PNG.
    pub jpeg: bool,
}

impl Default for ImageExport {
    fn default() -> Self {
        Self {
            pdf: PdfExport::default(),
            scale: 2.0,
            background: None,
            width: 800.0,
            paged: false,
            jpeg: false,
        }
    }
}

/// Largest image side in pixels.
const MAX_SIDE: f32 = 32_000.0;

struct Outline(tiny_skia_path::PathBuilder);

impl ttf_parser::OutlineBuilder for Outline {
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

fn paint(c: Color32) -> Paint<'static> {
    let mut p = Paint::default();
    // Color32 is premultiplied; tiny-skia wants straight alpha here.
    let a = c.a();
    let un = |v: u8| {
        if a == 0 {
            0
        } else {
            ((u32::from(v) * 255 + u32::from(a) / 2) / u32::from(a)).min(255) as u8
        }
    };
    p.set_color_rgba8(un(c.r()), un(c.g()), un(c.b()), a);
    p.anti_alias = true;
    p
}

fn rect_path(r: Rect, radius: f32) -> Option<tiny_skia::Path> {
    let rad = radius.min(r.width() / 2.0).min(r.height() / 2.0).max(0.0);
    if rad < 0.01 {
        return Some(PathBuilder::from_rect(tiny_skia::Rect::from_ltrb(
            r.min.x,
            r.min.y,
            r.max.x.max(r.min.x + 0.01),
            r.max.y.max(r.min.y + 0.01),
        )?));
    }
    let k = 0.552_284_8 * rad;
    let (x0, y0, x1, y1) = (r.min.x, r.min.y, r.max.x, r.max.y);
    let mut b = PathBuilder::new();
    b.move_to(x0 + rad, y0);
    b.line_to(x1 - rad, y0);
    b.cubic_to(x1 - rad + k, y0, x1, y0 + rad - k, x1, y0 + rad);
    b.line_to(x1, y1 - rad);
    b.cubic_to(x1, y1 - rad + k, x1 - rad + k, y1, x1 - rad, y1);
    b.line_to(x0 + rad, y1);
    b.cubic_to(x0 + rad - k, y1, x0, y1 - rad + k, x0, y1 - rad);
    b.line_to(x0, y0 + rad);
    b.cubic_to(x0, y0 + rad - k, x0 + rad - k, y0, x0 + rad, y0);
    b.close();
    b.finish()
}

/// Draws ops into a pixmap.
struct Raster<'a> {
    faces: &'a FaceTable,
    images: &'a ExportImages,
    glyphs: HashMap<(usize, u16), Option<tiny_skia::Path>>,
    decoded: HashMap<String, Option<Pixmap>>,
}

impl Raster<'_> {
    fn glyph(&mut self, face: usize, gid: u16) -> Option<tiny_skia::Path> {
        self.glyphs
            .entry((face, gid))
            .or_insert_with(|| {
                let mut o = Outline(tiny_skia_path::PathBuilder::new());
                self.faces
                    .face(face)
                    .outline_glyph(ttf_parser::GlyphId(gid), &mut o)?;
                o.0.finish()
            })
            .clone()
    }

    fn pixmap(&mut self, src: &str) -> Option<Pixmap> {
        if let Some(p) = self.decoded.get(src) {
            return p.clone();
        }
        let p = self.images.get(src).and_then(|l| {
            if l.svg {
                let tree = resvg::usvg::Tree::from_data(&l.bytes, &resvg::usvg::Options::default())
                    .ok()?;
                let s = tree.size();
                let k = 3.0f32.min(4096.0 / s.width().max(s.height()).max(1.0));
                let mut pm = Pixmap::new(
                    (s.width() * k).ceil() as u32,
                    (s.height() * k).ceil() as u32,
                )?;
                resvg::render(&tree, Transform::from_scale(k, k), &mut pm.as_mut());
                Some(pm)
            } else {
                let img = image::load_from_memory(&l.bytes).ok()?.to_rgba8();
                let (w, h) = img.dimensions();
                let mut pm = Pixmap::new(w, h)?;
                for (d, s) in pm.pixels_mut().iter_mut().zip(img.pixels()) {
                    *d = tiny_skia::ColorU8::from_rgba(s[0], s[1], s[2], s[3]).premultiply();
                }
                Some(pm)
            }
        });
        self.decoded.insert(src.to_owned(), p.clone());
        p
    }

    fn draw(&mut self, pm: &mut Pixmap, t: Transform, op: &Op) {
        match op {
            Op::Rect {
                rect,
                fill,
                stroke,
                radius,
            } => {
                let Some(p) = rect_path(*rect, *radius) else {
                    return;
                };
                if fill.a() > 0 {
                    pm.fill_path(&p, &paint(*fill), FillRule::Winding, t, None);
                }
                if let Some((w, c)) = stroke {
                    pm.stroke_path(
                        &p,
                        &paint(*c),
                        &Stroke {
                            width: *w,
                            ..Default::default()
                        },
                        t,
                        None,
                    );
                }
            }
            Op::Line { a, b, width, color } => {
                let mut pb = PathBuilder::new();
                pb.move_to(a.x, a.y);
                pb.line_to(b.x, b.y);
                if let Some(p) = pb.finish() {
                    pm.stroke_path(
                        &p,
                        &paint(*color),
                        &Stroke {
                            width: *width,
                            ..Default::default()
                        },
                        t,
                        None,
                    );
                }
            }
            Op::Circle { c, r, fill, stroke } => {
                let Some(p) = PathBuilder::from_circle(c.x, c.y, *r) else {
                    return;
                };
                if fill.a() > 0 {
                    pm.fill_path(&p, &paint(*fill), FillRule::Winding, t, None);
                }
                if let Some((w, col)) = stroke {
                    pm.stroke_path(
                        &p,
                        &paint(*col),
                        &Stroke {
                            width: *w,
                            ..Default::default()
                        },
                        t,
                        None,
                    );
                }
            }
            Op::Path { path, color } => {
                pm.fill_path(path, &paint(*color), FillRule::Winding, t, None)
            }
            Op::Text {
                face,
                size,
                color,
                baseline,
                glyphs,
                ..
            } => {
                let upem = f32::from(self.faces.face(*face).units_per_em()).max(1.0);
                let k = size / upem;
                let p = paint(*color);
                for g in glyphs {
                    if let Some(path) = self.glyph(*face, g.gid) {
                        let gt = t.pre_concat(Transform::from_row(k, 0.0, 0.0, k, g.x, *baseline));
                        pm.fill_path(&path, &p, FillRule::Winding, gt, None);
                    }
                }
            }
            Op::Image { rect, src } => {
                if let Some(img) = self.pixmap(src) {
                    let sx = rect.width() / img.width() as f32;
                    let sy = rect.height() / img.height() as f32;
                    let it = t.pre_concat(Transform::from_row(
                        sx, 0.0, 0.0, sy, rect.min.x, rect.min.y,
                    ));
                    let pp = tiny_skia::PixmapPaint {
                        quality: tiny_skia::FilterQuality::Bicubic,
                        ..Default::default()
                    };
                    pm.draw_pixmap(0, 0, img.as_ref(), &pp, it, None);
                }
            }
            Op::Link { .. } => {}
        }
    }
}

fn encode(pm: &Pixmap, out: &Path, jpeg: bool) -> Result<(), String> {
    let (w, h) = (pm.width(), pm.height());
    let mut data = Vec::with_capacity((w * h * 4) as usize);
    for p in pm.pixels() {
        let c = p.demultiply();
        data.extend_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
    }
    let img = image::RgbaImage::from_raw(w, h, data).ok_or("bad image size")?;
    if jpeg {
        let rgb = image::DynamicImage::ImageRgba8(img).to_rgb8();
        rgb.save_with_format(out, image::ImageFormat::Jpeg)
            .map_err(|e| e.to_string())
    } else {
        img.save_with_format(out, image::ImageFormat::Png)
            .map_err(|e| e.to_string())
    }
}

fn draw_items(
    r: &mut Raster<'_>,
    pm: &mut Pixmap,
    base: Transform,
    items: &[Laid],
    a: f32,
    b: f32,
) {
    for it in items
        .iter()
        .filter(|it| it.top < b && it.top + it.height > a)
    {
        let t = base.pre_concat(Transform::from_translate(0.0, it.top - a));
        for op in &it.ops {
            let (y0, y1) = op_y(op);
            let on = match op {
                Op::Text { .. } => it.top + y1 > a && it.top + y1 <= b + 0.5,
                _ => it.top + y1 > a && it.top + y0 < b,
            };
            if on {
                r.draw(pm, t, op);
            }
        }
    }
}

/// Exports `src` as image(s) at `out`; with `paged`, pages are written as
/// `name-1.png`, `name-2.png`… Returns the files written.
pub fn export_image(src: &str, o: &ImageExport, out: &Path) -> Result<Vec<PathBuf>, String> {
    inkra_highlight::load_now();
    let style = &o.pdf.style;
    let bg = o.background.unwrap_or(style.bg);
    let defs = inkra_layout::fonts::definitions();
    let mut faces = FaceTable::new(&defs);
    let mut store = Fonts::new(TextOptions::default(), defs);
    let mut fonts = store.with_pixels_per_point(1.0);
    let images = ExportImages::new(o.pdf.doc_path.clone());
    let scale = o.scale.clamp(0.5, 4.0);
    if !o.paged {
        let prep = prepare(src, &o.pdf, o.width, &mut fonts, &mut faces, &images);
        let pad = style.padding_x.clamp(16.0, 48.0);
        let top = prep.items.first().map_or(0.0, |i| i.top);
        let bottom = prep.items.last().map_or(top, |i| i.top + i.height);
        let (w, h) = (o.width + 2.0 * pad, bottom - top + 2.0 * pad);
        let s = scale.min(MAX_SIDE / h.max(1.0)).min(MAX_SIDE / w);
        let mut pm =
            Pixmap::new((w * s).ceil() as u32, (h * s).ceil() as u32).ok_or("image too large")?;
        pm.fill(tiny_skia::Color::from_rgba8(bg.r(), bg.g(), bg.b(), 255));
        let mut r = Raster {
            faces: &faces,
            images: &images,
            glyphs: HashMap::new(),
            decoded: HashMap::new(),
        };
        let base = Transform::from_row(s, 0.0, 0.0, s, pad * s, pad * s);
        draw_items(&mut r, &mut pm, base, &prep.items, top, bottom + 1.0);
        encode(&pm, out, o.jpeg)?;
        return Ok(vec![out.to_owned()]);
    }
    let pg = &o.pdf.page;
    let ps = pg.scale.max(0.1);
    let [mt, mr, mb, ml] = pg.margins;
    let content_w = ((pg.width - ml - mr) / ps).max(100.0);
    let content_h = ((pg.height - mt - mb) / ps).max(100.0);
    let prep = prepare(src, &o.pdf, content_w, &mut fonts, &mut faces, &images);
    let pages = paginate(&prep.items, content_h);
    let stem = out
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "page".into());
    let ext = if o.jpeg { "jpg" } else { "png" };
    let dir = out.parent().unwrap_or(Path::new("."));
    let k = scale * 96.0 / 72.0;
    let mut r = Raster {
        faces: &faces,
        images: &images,
        glyphs: HashMap::new(),
        decoded: HashMap::new(),
    };
    let mut written = Vec::new();
    for (n, p) in pages.iter().enumerate() {
        let mut pm = Pixmap::new((pg.width * k).ceil() as u32, (pg.height * k).ceil() as u32)
            .ok_or("page too large")?;
        pm.fill(tiny_skia::Color::from_rgba8(bg.r(), bg.g(), bg.b(), 255));
        let header_h = p.header.map_or(0.0, |h| h.1);
        let base = Transform::from_row(k * ps, 0.0, 0.0, k * ps, ml * k, (mt + header_h * ps) * k);
        if let Some((hi, _)) = p.header
            && let Some((h0, h1)) = prep.items[hi].table_header
        {
            let ht = Transform::from_row(k * ps, 0.0, 0.0, k * ps, ml * k, (mt - h0 * ps) * k);
            for op in prep.items[hi].ops.iter().filter(|op| {
                let (y0, y1) = op_y(op);
                y1 > h0 && y0 < h1
            }) {
                r.draw(&mut pm, ht, op);
            }
        }
        draw_items(&mut r, &mut pm, base, &prep.items, p.a, p.b);
        let file = dir.join(format!("{stem}-{}.{ext}", n + 1));
        encode(&pm, &file, o.jpeg)?;
        written.push(file);
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_and_paged_images() {
        let d = tempfile::tempdir().unwrap();
        let src =
            "# Title\n\nSome **text** and $x^2$.\n\n```mermaid\npie\n\"a\" : 1\n\"b\" : 1\n```\n";
        let out = d.path().join("doc.png");
        let files = export_image(src, &ImageExport::default(), &out).unwrap();
        let img = image::open(&files[0]).unwrap().to_rgba8();
        assert_eq!(
            img.width(),
            ((800.0 + 2.0 * Style::default().padding_x.clamp(16.0, 48.0)) * 2.0f32).ceil() as u32
        );
        // Something dark (text) was drawn on the light background.
        assert!(img.pixels().any(|p| p[0] < 80 && p[1] < 80 && p[2] < 80));
        let o = ImageExport {
            paged: true,
            jpeg: true,
            scale: 1.0,
            ..Default::default()
        };
        let files = export_image(src, &o, &out).unwrap();
        assert_eq!(files.len(), 1);
        assert!(files[0].to_string_lossy().ends_with("doc-1.jpg"));
    }

    use inkra_layout::Style;
}
