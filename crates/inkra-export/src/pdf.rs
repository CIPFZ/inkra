//! PDF export (EX-002): the live layout of the document at the page's content
//! width, paginated (keep-with-next headings, orphan/widow control, repeated
//! table headers, splittable code blocks), drawn with krilla — real selectable
//! text with subset fonts, vector formulas and diagrams, clickable links,
//! internal jumps, bookmarks from headings and metadata from front matter.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use inkra_doc::Doc;
use inkra_layout::epaint::text::{FontId, Fonts, FontsView, LayoutJob, TextOptions};
use inkra_layout::epaint::{Color32, Rect, pos2};
use inkra_layout::live::leaf::ImageSizes;
use inkra_layout::{BlockLayout, Ctx, Mode, Overlay, Style, ViewLayout};
use inkra_markdown::BlockKind;
use krilla::action::{Action, LinkAction};
use krilla::annotation::{Annotation, LinkAnnotation, Target};
use krilla::color::rgb;
use krilla::destination::{Destination, XyzDestination};
use krilla::geom::{PathBuilder, Point, Transform};
use krilla::image::Image;
use krilla::num::NormalizedF32;
use krilla::outline::{Outline, OutlineNode};
use krilla::page::PageSettings;
use krilla::paint::{Fill, FillRule, Stroke};
use krilla::text::{Font, GlyphId, KrillaGlyph};
use krilla::{Data, Document};

use crate::display::{FaceTable, Op, break_points, job_ops, layout_ops};

/// Page geometry and decorations (EX-002 page setup).
#[derive(Debug, Clone, PartialEq)]
pub struct PageSetup {
    /// Page width in points.
    pub width: f32,
    /// Page height in points.
    pub height: f32,
    /// Margins in points: top, right, bottom, left.
    pub margins: [f32; 4],
    /// Header template (`{{title}} {{page}} {{pages}} {{date}}`).
    pub header: String,
    /// Footer template.
    pub footer: String,
    /// Paint the theme background.
    pub background: bool,
    /// Points per layout pixel (CSS: 0.75).
    pub scale: f32,
}

impl Default for PageSetup {
    fn default() -> Self {
        let (width, height) = paper("A4").unwrap_or((595.0, 842.0));
        Self {
            width,
            height,
            margins: [56.0, 50.0, 56.0, 50.0],
            header: String::new(),
            footer: "{{page}} / {{pages}}".into(),
            background: false,
            scale: 0.75,
        }
    }
}

/// Paper sizes in points (portrait).
pub fn paper(name: &str) -> Option<(f32, f32)> {
    let mm = |w: f32, h: f32| (w * 72.0 / 25.4, h * 72.0 / 25.4);
    Some(match name.to_ascii_lowercase().as_str() {
        "a3" => mm(297.0, 420.0),
        "a4" => mm(210.0, 297.0),
        "a5" => mm(148.0, 210.0),
        "b5" => mm(176.0, 250.0),
        "letter" => (612.0, 792.0),
        "legal" => (612.0, 1008.0),
        _ => return None,
    })
}

/// PDF export options.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PdfExport {
    /// Theme used for the export.
    pub style: Style,
    /// Page setup.
    pub page: PageSetup,
    /// The document file (relative images, default title).
    pub doc_path: Option<PathBuf>,
    /// Parse options.
    pub parse: inkra_markdown::ParseOptions,
    /// `{{date}}` (default: today, UTC).
    pub date: Option<String>,
}

/// Today's date (UTC) as `YYYY-MM-DD`.
pub fn today() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let (y, m, d) = civil(secs / 86_400);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Days since 1970-01-01 to a civil date (H. Hinnant's algorithm).
fn civil(days: u64) -> (i64, u32, u32) {
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// A loaded image.
#[derive(Clone)]
pub(crate) struct LoadedImage {
    /// For PDF (`None`: format krilla cannot embed).
    pub pdf: Option<Image>,
    /// The file data (raster export decodes it again).
    pub bytes: Arc<Vec<u8>>,
    /// SVG data.
    pub svg: bool,
    /// Size in layout px.
    pub size: [f32; 2],
}

type Loaded = Option<LoadedImage>;

/// Images resolved against the document folder, loaded on first use.
pub(crate) struct ExportImages {
    pub doc: Option<PathBuf>,
    pub cache: RefCell<HashMap<String, Loaded>>,
    /// Images taller than this are scaled down to fit a page (EX-002).
    pub max_h: f32,
}

fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let mut acc = 0u32;
    let mut bits = 0;
    for c in s.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' | b'\n' | b'\r' | b' ' => continue,
            _ => return None,
        };
        acc = (acc << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

fn decode_image(bytes: Vec<u8>) -> Loaded {
    let b = bytes.as_slice();
    let is_svg = {
        let head = String::from_utf8_lossy(&b[..b.len().min(512)]);
        head.contains("<svg")
    };
    if is_svg {
        let tree = resvg::usvg::Tree::from_data(b, &resvg::usvg::Options::default()).ok()?;
        let size = tree.size();
        let (w, h) = (size.width().max(1.0), size.height().max(1.0));
        let scale = 3.0f32.min(4096.0 / w.max(h));
        let (pw, ph) = ((w * scale).ceil() as u32, (h * scale).ceil() as u32);
        let mut pm = resvg::tiny_skia::Pixmap::new(pw, ph)?;
        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::from_scale(scale, scale),
            &mut pm.as_mut(),
        );
        let rgba: Vec<u8> = pm
            .pixels()
            .iter()
            .flat_map(|p| {
                let c = p.demultiply();
                [c.red(), c.green(), c.blue(), c.alpha()]
            })
            .collect();
        return Some(LoadedImage {
            pdf: Some(Image::from_rgba8(rgba, pw, ph)),
            bytes: Arc::new(bytes),
            svg: true,
            size: [w, h],
        });
    }
    let data: Data = bytes.clone().into();
    let pdf = if b.starts_with(b"\x89PNG") {
        Image::from_png(data, true).ok()
    } else if b.starts_with(&[0xFF, 0xD8]) {
        Image::from_jpeg(data, true).ok()
    } else if b.starts_with(b"GIF8") {
        Image::from_gif(data, true).ok()
    } else if b.len() > 12 && &b[0..4] == b"RIFF" && &b[8..12] == b"WEBP" {
        Image::from_webp(data, true).ok()
    } else {
        None
    };
    let size = match &pdf {
        Some(img) => {
            let (w, h) = img.size();
            [w as f32, h as f32]
        }
        None => {
            let d = image::ImageReader::new(std::io::Cursor::new(b))
                .with_guessed_format()
                .ok()?
                .into_dimensions()
                .ok()?;
            [d.0 as f32, d.1 as f32]
        }
    };
    Some(LoadedImage {
        pdf,
        bytes: Arc::new(bytes),
        svg: false,
        size,
    })
}

impl ExportImages {
    pub(crate) fn new(doc: Option<PathBuf>) -> Self {
        Self {
            doc,
            cache: RefCell::new(HashMap::new()),
            max_h: f32::INFINITY,
        }
    }

    pub(crate) fn get(&self, src: &str) -> Loaded {
        if let Some(v) = self.cache.borrow().get(src) {
            return v.clone();
        }
        let bytes = if let Some(rest) = src.strip_prefix("data:") {
            rest.split_once(";base64,")
                .and_then(|(_, d)| base64_decode(d))
        } else {
            crate::html::resolve_image(self.doc.as_deref(), src).and_then(|p| std::fs::read(p).ok())
        };
        let v = bytes.and_then(decode_image);
        self.cache.borrow_mut().insert(src.to_owned(), v.clone());
        v
    }
}

impl ImageSizes for ExportImages {
    fn size(&self, src: &str) -> Option<[f32; 2]> {
        self.get(src).map(|l| {
            let [w, h] = l.size;
            if h > self.max_h {
                [w * self.max_h / h, self.max_h]
            } else {
                [w, h]
            }
        })
    }
}

/// A laid-out item ready for pagination.
pub(crate) struct Laid {
    pub(crate) top: f32,
    pub(crate) height: f32,
    pub(crate) ops: Vec<Op>,
    pub(crate) breaks: Vec<f32>,
    pub(crate) heading: bool,
    /// Header row of a table: item-local y range.
    pub(crate) table_header: Option<(f32, f32)>,
}

/// One page: the document y range it shows and an optional repeated header.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Slice {
    pub(crate) a: f32,
    pub(crate) b: f32,
    /// `(item, header height)`.
    pub(crate) header: Option<(usize, f32)>,
}

/// Splits items into pages of `page_h` (layout units).
pub(crate) fn paginate(items: &[Laid], page_h: f32) -> Vec<Slice> {
    let mut pages = Vec::new();
    let mut a = items.first().map_or(0.0, |i| i.top);
    let mut header: Option<(usize, f32)> = None;
    let mut i = 0;
    while i < items.len() {
        let it = &items[i];
        let avail = page_h - header.map_or(0.0, |h| h.1);
        let limit = a + avail;
        let bottom = it.top + it.height;
        if bottom <= limit + 0.5 {
            // Keep a heading with the first lines of what follows.
            if it.heading
                && it.top > a + 0.5
                && let Some(next) = items.get(i + 1)
            {
                let first = next.breaks.first().map_or(next.height, |b| *b);
                if next.top + first > limit && next.top + first - it.top <= avail {
                    pages.push(Slice {
                        a,
                        b: it.top,
                        header,
                    });
                    a = it.top;
                    header = None;
                    continue;
                }
            }
            i += 1;
            continue;
        }
        // The item crosses the page end: split at a row boundary when possible.
        let rows: Vec<f32> = it.breaks.iter().map(|b| it.top + b).collect();
        let fitting: Vec<usize> = (0..rows.len())
            .filter(|&k| rows[k] > a + 0.5 && rows[k] <= limit)
            .collect();
        let mut cut = fitting.last().copied();
        // Widow: never leave a single line for the next page.
        if let Some(k) = cut
            && k + 2 == rows.len()
            && fitting.len() >= 2
        {
            cut = Some(fitting[fitting.len() - 2]);
        }
        // Orphan: a single line at the page end moves on with the rest.
        if let Some(k) = cut
            && k == 0
            && rows.len() > 2
            && it.top > a + 0.5
        {
            cut = None;
        }
        let at_top = it.top <= a + 0.5;
        let b = match cut {
            Some(k) => rows[k],
            None if !at_top => it.top,
            // Taller than a page and unbreakable: cut it.
            None => limit,
        };
        pages.push(Slice { a, b, header });
        // A table split below its header repeats the header on the next page.
        header = match it.table_header {
            Some((h0, h1)) if b > it.top + h1 && b < bottom => Some((i, h1 - h0)),
            _ => None,
        };
        a = b;
    }
    let end = items.last().map_or(a, |i| i.top + i.height);
    if end > a + 0.5 || pages.is_empty() {
        pages.push(Slice {
            a,
            b: end.max(a + 1.0),
            header,
        });
    }
    pages
}

/// The vertical extent of an op (item coordinates) used to assign it to pages.
pub(crate) fn op_y(op: &Op) -> (f32, f32) {
    match op {
        Op::Rect { rect, .. } | Op::Image { rect, .. } | Op::Link { rect, .. } => {
            (rect.min.y, rect.max.y)
        }
        Op::Line { a, b, .. } => (a.y.min(b.y), a.y.max(b.y)),
        Op::Circle { c, r, .. } => (c.y - r, c.y + r),
        Op::Text { baseline, size, .. } => (baseline - size * 0.8, *baseline),
        Op::Path { path, .. } => {
            let b = path.bounds();
            (b.top(), b.bottom())
        }
    }
}

fn unmultiply(c: Color32) -> (rgb::Color, NormalizedF32) {
    let a = c.a();
    if a == 255 || a == 0 {
        return (
            rgb::Color::new(c.r(), c.g(), c.b()),
            NormalizedF32::new(f32::from(a) / 255.0).unwrap_or(NormalizedF32::ONE),
        );
    }
    let f = 255.0 / f32::from(a);
    let u = |v: u8| (f32::from(v) * f).round().min(255.0) as u8;
    (
        rgb::Color::new(u(c.r()), u(c.g()), u(c.b())),
        NormalizedF32::new(f32::from(a) / 255.0).unwrap_or(NormalizedF32::ONE),
    )
}

fn fill_of(c: Color32) -> Fill {
    let (color, opacity) = unmultiply(c);
    Fill {
        paint: color.into(),
        opacity,
        rule: FillRule::NonZero,
    }
}

fn stroke_of(width: f32, c: Color32) -> Stroke {
    let (color, opacity) = unmultiply(c);
    Stroke {
        paint: color.into(),
        width,
        opacity,
        ..Default::default()
    }
}

fn to_krilla(p: &tiny_skia_path::Path) -> Option<krilla::geom::Path> {
    use tiny_skia_path::PathSegment as S;
    let mut b = PathBuilder::new();
    for s in p.segments() {
        match s {
            S::MoveTo(a) => b.move_to(a.x, a.y),
            S::LineTo(a) => b.line_to(a.x, a.y),
            S::QuadTo(c, a) => b.quad_to(c.x, c.y, a.x, a.y),
            S::CubicTo(c1, c2, a) => b.cubic_to(c1.x, c1.y, c2.x, c2.y, a.x, a.y),
            S::Close => b.close(),
        }
    }
    b.finish()
}

fn rounded_rect(r: Rect, radius: f32) -> Option<krilla::geom::Path> {
    let rad = radius.min(r.width() / 2.0).min(r.height() / 2.0).max(0.0);
    let mut b = PathBuilder::new();
    if rad < 0.01 {
        b.push_rect(krilla::geom::Rect::from_xywh(
            r.min.x,
            r.min.y,
            r.width().max(0.01),
            r.height().max(0.01),
        )?);
        return b.finish();
    }
    let k = 0.552_284_8 * rad;
    let (x0, y0, x1, y1) = (r.min.x, r.min.y, r.max.x, r.max.y);
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

fn circle(c: inkra_layout::epaint::Pos2, r: f32) -> Option<krilla::geom::Path> {
    rounded_rect(
        Rect::from_center_size(c, inkra_layout::epaint::vec2(2.0 * r, 2.0 * r)),
        r,
    )
}

fn line(
    a: inkra_layout::epaint::Pos2,
    b: inkra_layout::epaint::Pos2,
) -> Option<krilla::geom::Path> {
    let mut p = PathBuilder::new();
    p.move_to(a.x, a.y);
    p.line_to(b.x, b.y);
    p.finish()
}

/// Draws ops on a surface (current transform: layout units).
struct Painter<'a> {
    faces: &'a FaceTable,
    fonts: HashMap<usize, Font>,
    images: &'a ExportImages,
}

impl Painter<'_> {
    fn font(&mut self, face: usize) -> Option<Font> {
        if let Some(f) = self.fonts.get(&face) {
            return Some(f.clone());
        }
        let fc = &self.faces.faces[face];
        let f = Font::new(Data::from(fc.data), fc.index)?;
        self.fonts.insert(face, f.clone());
        Some(f)
    }

    fn draw(&mut self, s: &mut krilla::surface::Surface<'_>, op: &Op) {
        match op {
            Op::Rect {
                rect,
                fill,
                stroke,
                radius,
            } => {
                let Some(p) = rounded_rect(*rect, *radius) else {
                    return;
                };
                if fill.a() > 0 {
                    s.set_fill(Some(fill_of(*fill)));
                    s.set_stroke(None);
                    s.draw_path(&p);
                }
                if let Some((w, c)) = stroke {
                    s.set_fill(None);
                    s.set_stroke(Some(stroke_of(*w, *c)));
                    s.draw_path(&p);
                }
            }
            Op::Line { a, b, width, color } => {
                if let Some(p) = line(*a, *b) {
                    s.set_fill(None);
                    s.set_stroke(Some(stroke_of(*width, *color)));
                    s.draw_path(&p);
                }
            }
            Op::Circle { c, r, fill, stroke } => {
                let Some(p) = circle(*c, *r) else { return };
                if fill.a() > 0 {
                    s.set_fill(Some(fill_of(*fill)));
                    s.set_stroke(None);
                    s.draw_path(&p);
                }
                if let Some((w, col)) = stroke {
                    s.set_fill(None);
                    s.set_stroke(Some(stroke_of(*w, *col)));
                    s.draw_path(&p);
                }
            }
            Op::Path { path, color } => {
                if let Some(p) = to_krilla(path) {
                    s.set_fill(Some(fill_of(*color)));
                    s.set_stroke(None);
                    s.draw_path(&p);
                }
            }
            Op::Text {
                face,
                size,
                color,
                baseline,
                glyphs,
                text,
            } => {
                let Some(font) = self.font(*face) else { return };
                let Some(first) = glyphs.first() else { return };
                let gl: Vec<KrillaGlyph> = glyphs
                    .iter()
                    .map(|g| KrillaGlyph {
                        glyph_id: GlyphId::new(u32::from(g.gid)),
                        text_range: g.range.clone(),
                        x_advance: g.adv / size,
                        x_offset: 0.0,
                        y_offset: 0.0,
                        y_advance: 0.0,
                        location: None,
                    })
                    .collect();
                s.set_fill(Some(fill_of(*color)));
                s.set_stroke(None);
                s.draw_glyphs(
                    Point::from_xy(first.x, *baseline),
                    &gl,
                    font,
                    text,
                    *size,
                    false,
                );
            }
            Op::Image { rect, src } => {
                if let Some(LoadedImage { pdf: Some(img), .. }) = self.images.get(src)
                    && let Some(size) =
                        krilla::geom::Size::from_wh(rect.width().max(0.01), rect.height().max(0.01))
                {
                    s.push_transform(&Transform::from_translate(rect.min.x, rect.min.y));
                    s.draw_image(img, size);
                    s.pop();
                }
            }
            Op::Link { .. } => {}
        }
    }
}

fn front_matter_field(src: &str, key: &str) -> Option<String> {
    let rest = src.strip_prefix("---")?;
    let end = rest.find("\n---")?;
    rest[..end].lines().find_map(|l| {
        let v = l
            .strip_prefix(key)?
            .strip_prefix(':')?
            .trim()
            .trim_matches(['"', '\'']);
        (!v.is_empty()).then(|| v.to_owned())
    })
}

pub(crate) fn fill_template(t: &str, title: &str, page: usize, pages: usize, date: &str) -> String {
    t.replace("{{title}}", title)
        .replace("{{page}}", &page.to_string())
        .replace("{{pages}}", &pages.to_string())
        .replace("{{date}}", date)
}

/// Lays out `src` and returns the items (document coordinates) and anchors.
pub(crate) struct Prepared {
    pub(crate) items: Vec<Laid>,
    /// `anchor → (doc y)`.
    pub(crate) anchors: HashMap<String, f32>,
    /// `(level, text, doc y)`.
    pub(crate) headings: Vec<(u8, String, f32)>,
}

pub(crate) fn prepare(
    src: &str,
    o: &PdfExport,
    width: f32,
    fonts: &mut FontsView<'_>,
    faces: &mut FaceTable,
    images: &ExportImages,
) -> Prepared {
    let doc = Doc::from_str(src, o.parse.clone());
    let mut style = o.style.clone();
    style.spellcheck = false;
    style.print = true;
    // Paper does not scroll.
    style.code_wrap = true;
    style.image_fit_column = true;
    style.version = style.version.wrapping_add(0x5EED);
    let mut view = ViewLayout::new();
    view.sync(&doc, Mode::Live, width, &style, usize::MAX);
    let cx = Ctx {
        doc: &doc,
        style: &style,
        caret: usize::MAX,
        images,
        syntax_theme: &style.syntax_theme,
    };
    let n = view.items().len();
    for i in 0..n {
        view.ensure(i, &cx, fonts);
    }
    let blocks = doc.ast().to_blocks();
    // `(view index, laid item, is footnote)` in document order.
    let mut laid: Vec<(usize, Laid, bool)> = Vec::with_capacity(n);
    for i in 0..n {
        let Some(li) = view.live_items().get(i) else {
            continue;
        };
        let kind = li.block(&doc).map(|(b, _)| b.kind.clone());
        let top_kind = doc.ast().get(li.top).map(|r| r.block.kind.clone());
        if matches!(kind, Some(BlockKind::FrontMatter { .. })) {
            continue;
        }
        let footnote = matches!(top_kind, Some(BlockKind::FootnoteDef { .. }));
        let Some(l) = view.layout(i) else { continue };
        let l: Arc<BlockLayout> = l;
        let top = view.top(i);
        let height = view.items()[i].height;
        let table_header = l.overlays.iter().find_map(|ov| match ov {
            Overlay::Table { rect, rows, .. } => rows.first().map(|h| (rect.min.y, rect.min.y + h)),
            _ => None,
        });
        let item = Laid {
            top,
            height,
            ops: layout_ops(&l, fonts, faces),
            breaks: break_points(&l, fonts),
            heading: matches!(kind, Some(BlockKind::Heading { .. })),
            table_header,
        };
        laid.push((i, item, footnote));
    }
    // Footnote definitions are collected at the end of the document (MD-010),
    // below a short rule; every item keeps the gap it had above it.
    let mut gaps: Vec<f32> = Vec::with_capacity(laid.len());
    let mut prev_bottom: Option<f32> = None;
    for (_, it, _) in &laid {
        gaps.push(prev_bottom.map_or(0.0, |b| (it.top - b).max(0.0)));
        prev_bottom = Some(it.top + it.height);
    }
    let order: Vec<usize> = (0..laid.len()).filter(|&k| !laid[k].2).collect();
    let notes: Vec<usize> = (0..laid.len()).filter(|&k| laid[k].2).collect();
    let em = style.size;
    let views: Vec<usize> = laid.iter().map(|(vi, _, _)| *vi).collect();
    let mut slots: Vec<Option<Laid>> = laid.into_iter().map(|(_, it, _)| Some(it)).collect();
    let mut y = slots.iter().flatten().next().map_or(0.0, |it| it.top);
    let mut items: Vec<Laid> = Vec::with_capacity(slots.len() + 1);
    // Laid index → final index.
    let mut final_of: HashMap<usize, usize> = HashMap::new();
    let mut place = |k: usize, gap: f32, y: &mut f32, items: &mut Vec<Laid>| {
        if let Some(mut it) = slots[k].take() {
            *y += gap;
            it.top = *y;
            *y += it.height;
            final_of.insert(k, items.len());
            items.push(it);
        }
    };
    for (n, &k) in order.iter().enumerate() {
        place(k, if n == 0 { 0.0 } else { gaps[k] }, &mut y, &mut items);
    }
    if !notes.is_empty() {
        y += 1.2 * em;
        let w = width.min(12.0 * em);
        items.push(Laid {
            top: y,
            height: 0.6 * em,
            ops: vec![Op::Line {
                a: pos2(0.0, 0.3 * em),
                b: pos2(w, 0.3 * em),
                width: 1.0,
                color: style.hr,
            }],
            breaks: Vec::new(),
            heading: false,
            table_header: None,
        });
        y += 0.6 * em;
        for (n, &k) in notes.iter().enumerate() {
            place(
                k,
                if n == 0 { 0.4 * em } else { gaps[k] },
                &mut y,
                &mut items,
            );
        }
    }
    let laid_of_view: HashMap<usize, usize> =
        views.iter().enumerate().map(|(k, vi)| (*vi, k)).collect();
    // The top of the first exported item at or after view item `vi`.
    let y_of_view = |vi: usize, items: &[Laid]| -> Option<f32> {
        let k = (vi..n).find_map(|v| laid_of_view.get(&v).copied())?;
        final_of.get(&k).map(|p| items[*p].top)
    };
    let mut anchors = HashMap::new();
    let mut headings = Vec::new();
    for h in inkra_markdown::walk::headings(doc.text(), &blocks) {
        if let Some(y) = y_of_view(view.item_of(h.span.start), &items) {
            anchors.entry(h.anchor.clone()).or_insert(y);
            headings.push((h.level, h.text, y));
        }
    }
    for b in &blocks {
        // The definition's first leaf starts after the `[^label]:` marker.
        let first = view
            .items()
            .iter()
            .position(|it| it.src.start >= b.span.start)
            .unwrap_or(n);
        if let BlockKind::FootnoteDef { label, .. } = &b.kind
            && let Some(y) = y_of_view(first, &items)
        {
            anchors.entry(format!("fn:{label}")).or_insert(y);
        }
    }
    Prepared {
        items,
        anchors,
        headings,
    }
}

fn outline(list: &[(u8, String, XyzDestination)]) -> Vec<OutlineNode> {
    let mut out: Vec<(u8, OutlineNode)> = Vec::new();
    let mut stack: Vec<(u8, OutlineNode)> = Vec::new();
    for (level, text, dest) in list {
        while let Some((l, _)) = stack.last()
            && *l >= *level
        {
            let (l, node) = stack.pop().unwrap_or_else(|| unreachable!());
            match stack.last_mut() {
                Some((_, parent)) => parent.push_child(node),
                None => out.push((l, node)),
            }
        }
        stack.push((*level, OutlineNode::new(text.clone(), dest.clone())));
    }
    while let Some((l, node)) = stack.pop() {
        match stack.last_mut() {
            Some((_, parent)) => parent.push_child(node),
            None => out.push((l, node)),
        }
    }
    out.into_iter().map(|(_, n)| n).collect()
}

/// Exports `src` as PDF bytes.
pub fn export_pdf(src: &str, o: &PdfExport) -> Result<Vec<u8>, String> {
    inkra_highlight::load_now();
    let pg = &o.page;
    let s = pg.scale.max(0.1);
    let [mt, mr, mb, ml] = pg.margins;
    let content_w = ((pg.width - ml - mr) / s).max(100.0);
    let content_h = ((pg.height - mt - mb) / s).max(100.0);
    let defs = inkra_layout::fonts::definitions();
    let mut faces = FaceTable::new(&defs);
    let mut fonts_store = Fonts::new(TextOptions::default(), defs);
    let mut fonts = fonts_store.with_pixels_per_point(1.0);
    let mut images = ExportImages::new(o.doc_path.clone());
    images.max_h = content_h * 0.92;
    let prep = prepare(src, o, content_w, &mut fonts, &mut faces, &images);
    let pages = paginate(&prep.items, content_h);
    let page_of = |y: f32| -> (usize, f32) {
        let k = pages
            .iter()
            .position(|p| y < p.b - 0.01)
            .unwrap_or(pages.len().saturating_sub(1));
        let p = pages[k];
        (k, mt + (y - p.a + p.header.map_or(0.0, |h| h.1)) * s)
    };
    let title = crate::html::title_of(src, o.doc_path.as_deref());
    let date = o.date.clone().unwrap_or_else(today);
    let mut document = Document::new();
    let mut meta = krilla::metadata::Metadata::new()
        .title(title.clone())
        .creator(format!("Inkra {}", env!("CARGO_PKG_VERSION")));
    if let Some(a) = front_matter_field(src, "author") {
        meta = meta.authors(vec![a]);
    }
    document.set_metadata(meta);
    let style = &o.style;
    let n_pages = pages.len();
    // Header and footer jobs are laid out up front (the font view is borrowed).
    let deco_font = FontId::new(
        12.0,
        inkra_layout::fonts::family_of(style.body_font, false, false),
    );
    let mut deco_ops: Vec<Vec<Op>> = Vec::with_capacity(n_pages);
    for k in 0..n_pages {
        let mut ops = Vec::new();
        for (tpl, y) in [(&pg.header, mt * 0.5), (&pg.footer, pg.height - mb * 0.5)] {
            let t = fill_template(tpl, &title, k + 1, n_pages, &date);
            if t.trim().is_empty() {
                continue;
            }
            let job = LayoutJob::simple_singleline(t, deco_font.clone(), style.fg_muted);
            let w = fonts.layout_job(job.clone()).size().x;
            // Page coordinates in layout units.
            let x = (pg.width / s - w) / 2.0;
            job_ops(&job, pos2(x, y / s - 6.0), &mut fonts, &mut faces, &mut ops);
        }
        deco_ops.push(ops);
    }
    let mut painter = Painter {
        faces: &faces,
        fonts: HashMap::new(),
        images: &images,
    };
    for (k, p) in pages.iter().enumerate() {
        let settings = PageSettings::from_wh(pg.width, pg.height).ok_or("bad page size")?;
        let mut page = document.start_page_with(settings);
        let mut annotations = Vec::new();
        {
            let mut surface = page.surface();
            if pg.background
                && let Some(r) = rounded_rect(
                    Rect::from_min_size(
                        pos2(0.0, 0.0),
                        inkra_layout::epaint::vec2(pg.width, pg.height),
                    ),
                    0.0,
                )
            {
                surface.set_fill(Some(fill_of(style.bg)));
                surface.set_stroke(None);
                surface.draw_path(&r);
            }
            surface.push_transform(&Transform::from_scale(s, s));
            for op in &deco_ops[k] {
                painter.draw(&mut surface, op);
            }
            surface.pop();
            let header_h = p.header.map_or(0.0, |h| h.1);
            // The body is clipped below the repeated table header (if any);
            // the header is drawn last, clipped to its band.
            let band = |y0: f32, h: f32| {
                rounded_rect(
                    Rect::from_min_size(
                        pos2(ml - 2.0, y0),
                        inkra_layout::epaint::vec2(pg.width - ml - mr + 4.0, h),
                    ),
                    0.0,
                )
            };
            let body_top = if header_h > 0.0 {
                mt + header_h * s
            } else {
                mt - 2.0
            };
            let clipped = if let Some(clip) = band(
                body_top,
                (p.b - p.a) * s + mt + header_h * s - body_top + 2.0,
            ) {
                surface.push_clip_path(&clip, &FillRule::NonZero);
                true
            } else {
                false
            };
            for it in prep
                .items
                .iter()
                .filter(|it| it.top < p.b && it.top + it.height > p.a)
            {
                let dy = it.top - p.a + header_h;
                surface.push_transform(&Transform::from_row(s, 0.0, 0.0, s, ml, mt + dy * s));
                for op in &it.ops {
                    let (y0, y1) = op_y(op);
                    let (ay0, ay1) = (it.top + y0, it.top + y1);
                    let on_page = match op {
                        // Text belongs to the page holding its baseline.
                        Op::Text { .. } => ay1 > p.a && ay1 <= p.b + 0.5,
                        _ => ay1 > p.a && ay0 < p.b,
                    };
                    if !on_page {
                        continue;
                    }
                    if let Op::Link { rect, target } = op {
                        let r = krilla::geom::Rect::from_xywh(
                            ml + rect.min.x * s,
                            mt + (dy + rect.min.y) * s,
                            rect.width() * s,
                            rect.height() * s,
                        );
                        let tgt = if let Some(anchor) = target.strip_prefix('#') {
                            prep.anchors.get(anchor).map(|y| {
                                let (pi, py) = page_of(*y);
                                Target::Destination(Destination::Xyz(XyzDestination::new(
                                    pi,
                                    Point::from_xy(0.0, (py - 8.0).max(0.0)),
                                )))
                            })
                        } else {
                            Some(Target::Action(Action::Link(LinkAction::new(
                                target.clone(),
                            ))))
                        };
                        if let (Some(r), Some(t)) = (r, tgt) {
                            annotations.push(Annotation::new_link(
                                LinkAnnotation::new(r, t),
                                Some(target.clone()),
                            ));
                        }
                        continue;
                    }
                    painter.draw(&mut surface, op);
                }
                surface.pop();
            }
            if clipped {
                surface.pop();
            }
            if let Some((hi, _)) = p.header {
                let it = &prep.items[hi];
                if let Some((h0, h1)) = it.table_header
                    && let Some(clip) = band(mt, header_h * s)
                {
                    surface.push_clip_path(&clip, &FillRule::NonZero);
                    surface.push_transform(&Transform::from_row(s, 0.0, 0.0, s, ml, mt - h0 * s));
                    for op in it.ops.iter().filter(|op| {
                        let (y0, y1) = op_y(op);
                        y1 > h0 && y0 < h1 && !matches!(op, Op::Link { .. })
                    }) {
                        painter.draw(&mut surface, op);
                    }
                    surface.pop();
                    surface.pop();
                }
            }
            surface.finish();
        }
        for a in annotations {
            page.add_annotation(a);
        }
        page.finish();
    }
    let dests: Vec<(u8, String, XyzDestination)> = prep
        .headings
        .iter()
        .map(|(l, t, y)| {
            let (pi, py) = page_of(*y);
            (
                *l,
                t.clone(),
                XyzDestination::new(pi, Point::from_xy(0.0, (py - 8.0).max(0.0))),
            )
        })
        .collect();
    let mut ol = Outline::new();
    for node in outline(&dests) {
        ol.push_child(node);
    }
    document.set_outline(ol);
    document.finish().map_err(|e| format!("{e:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn laid(top: f32, height: f32, breaks: Vec<f32>, heading: bool) -> Laid {
        Laid {
            top,
            height,
            ops: Vec::new(),
            breaks,
            heading,
            table_header: None,
        }
    }

    #[test]
    fn pagination_rules() {
        // Fits on one page.
        let p = paginate(
            &[
                laid(0.0, 50.0, vec![], false),
                laid(60.0, 50.0, vec![], false),
            ],
            200.0,
        );
        assert_eq!(p.len(), 1);
        // An unbreakable item that does not fit moves to the next page.
        let p = paginate(
            &[
                laid(0.0, 150.0, vec![], false),
                laid(160.0, 100.0, vec![], false),
            ],
            200.0,
        );
        assert_eq!((p[0].b, p[1].a), (160.0, 160.0));
        // A heading stays with the following paragraph.
        let p = paginate(
            &[
                laid(0.0, 150.0, vec![], false),
                laid(160.0, 30.0, vec![30.0], true),
                laid(195.0, 60.0, vec![20.0, 40.0, 60.0], false),
            ],
            200.0,
        );
        assert_eq!(p[0].b, 160.0);
        // A paragraph splits at a row boundary, never leaving a single widow line.
        let rows: Vec<f32> = (1..=10).map(|k| k as f32 * 20.0).collect();
        let p = paginate(
            &[
                laid(0.0, 120.0, vec![], false),
                laid(130.0, 200.0, rows, false),
            ],
            200.0,
        );
        assert_eq!(p[0].b, 130.0 + 60.0);
        let rows: Vec<f32> = (1..=4).map(|k| k as f32 * 20.0).collect();
        let p = paginate(
            &[
                laid(0.0, 135.0, vec![], false),
                laid(140.0, 80.0, rows, false),
            ],
            200.0,
        );
        assert_eq!(p[0].b, 140.0 + 40.0, "widow avoided: two lines move on");
        assert_eq!(civil(0), (1970, 1, 1));
        assert_eq!(civil(20_000), (2024, 10, 4));
    }

    #[test]
    fn exports_a_pdf_with_text_links_and_outline() {
        let src = "---\ntitle: Report\nauthor: Ann\n---\n# Intro\n\nSee [part two](#part-two) and [site](https://example.com).\n\n$$\n\\int_0^1 x\\,dx\n$$\n\n## Part two\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n```rust\nfn main() {}\n```\n\n中文段落。\n";
        let mut long = String::from(src);
        for k in 0..80 {
            long.push_str(&format!(
                "\nParagraph {k} with some text to fill the pages.\n"
            ));
        }
        let pdf = export_pdf(&long, &PdfExport::default()).unwrap();
        assert!(pdf.starts_with(b"%PDF-"));
        let s = String::from_utf8_lossy(&pdf);
        assert!(s.contains("/Outlines"), "bookmarks");
        assert!(s.contains("https://example.com"), "external link");
        assert!(s.contains("/Dest") || s.contains("/D ["), "internal link");
        assert!(s.contains("Report"), "title metadata");
        assert!(
            s.matches("/Type /Page").count() >= 3 || s.matches("/Type/Page").count() >= 3,
            "several pages"
        );
    }
}
