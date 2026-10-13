//! A device-independent display list extracted from block layouts: positioned
//! glyphs (real font glyph ids, for selectable PDF text), rectangles, lines,
//! vector paths (formulas, diagrams), images and links. PDF and PNG export
//! both draw from it.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

use inkra_layout::epaint::text::{FontDefinitions, FontFamily, FontsView, LayoutJob};
use inkra_layout::epaint::{Color32, Pos2, Rect, pos2, vec2};
use inkra_layout::{BlockLayout, Deco, HitAction};

/// A glyph of a text run.
#[derive(Debug, Clone, PartialEq)]
pub struct PGlyph {
    /// Glyph id in the face.
    pub gid: u16,
    /// Pen x position.
    pub x: f32,
    /// Advance (distance to the next glyph of the run).
    pub adv: f32,
    /// Byte range in the run's `text`.
    pub range: Range<usize>,
}

/// A drawing operation in item coordinates (points, y down).
#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    /// A rectangle.
    Rect {
        /// Rectangle.
        rect: Rect,
        /// Fill.
        fill: Color32,
        /// Outline width and color.
        stroke: Option<(f32, Color32)>,
        /// Corner radius.
        radius: f32,
    },
    /// A line.
    Line {
        /// Start.
        a: Pos2,
        /// End.
        b: Pos2,
        /// Width.
        width: f32,
        /// Color.
        color: Color32,
    },
    /// A circle.
    Circle {
        /// Center.
        c: Pos2,
        /// Radius.
        r: f32,
        /// Fill.
        fill: Color32,
        /// Outline.
        stroke: Option<(f32, Color32)>,
    },
    /// Glyphs of one face, size and color on one baseline.
    Text {
        /// Index into the face table.
        face: usize,
        /// Font size.
        size: f32,
        /// Color.
        color: Color32,
        /// Baseline y.
        baseline: f32,
        /// Glyphs.
        glyphs: Vec<PGlyph>,
        /// The characters (for text extraction).
        text: String,
    },
    /// A filled vector path (already positioned).
    Path {
        /// Path.
        path: tiny_skia_path::Path,
        /// Color.
        color: Color32,
    },
    /// An image.
    Image {
        /// Rectangle.
        rect: Rect,
        /// Source as written in the document.
        src: String,
    },
    /// A link area.
    Link {
        /// Rectangle.
        rect: Rect,
        /// URL or `#anchor`.
        target: String,
    },
}

/// A font face available for export.
#[derive(Clone)]
pub struct Face {
    /// Font name in the definitions.
    pub name: String,
    /// File data.
    pub data: &'static [u8],
    /// Face index in a collection.
    pub index: u32,
    /// Size scale (epaint font tweak).
    pub scale: f32,
    parsed: Arc<ttf_parser::Face<'static>>,
}

/// The faces of the editor's font definitions and the family fallback chains.
pub struct FaceTable {
    /// Faces in definition order.
    pub faces: Vec<Face>,
    by_name: HashMap<String, usize>,
    chains: HashMap<FontFamily, Vec<usize>>,
    cache: HashMap<(FontFamily, char), Option<(usize, u16)>>,
}

impl FaceTable {
    /// Builds the table from font definitions.
    pub fn new(defs: &FontDefinitions) -> Self {
        let mut faces = Vec::new();
        let mut by_name = HashMap::new();
        let mut names: Vec<&String> = defs.font_data.keys().collect();
        names.sort();
        for name in names {
            let fd = &defs.font_data[name];
            let data: &'static [u8] = match &fd.font {
                std::borrow::Cow::Borrowed(b) => b,
                std::borrow::Cow::Owned(v) => leak_once(name, v),
            };
            let Ok(parsed) = ttf_parser::Face::parse(data, fd.index) else {
                continue;
            };
            by_name.insert(name.clone(), faces.len());
            faces.push(Face {
                name: name.clone(),
                data,
                index: fd.index,
                scale: fd.tweak.scale,
                parsed: Arc::new(parsed),
            });
        }
        let chains = defs
            .families
            .iter()
            .map(|(fam, list)| {
                (
                    fam.clone(),
                    list.iter()
                        .filter_map(|n| by_name.get(n).copied())
                        .collect(),
                )
            })
            .collect();
        Self {
            faces,
            by_name,
            chains,
            cache: HashMap::new(),
        }
    }

    /// The face and glyph that draw `c` in `family`.
    pub fn resolve(&mut self, family: &FontFamily, c: char) -> Option<(usize, u16)> {
        if let Some(r) = self.cache.get(&(family.clone(), c)) {
            return *r;
        }
        let r = self.chains.get(family).and_then(|chain| {
            chain.iter().find_map(|&i| {
                self.faces[i]
                    .parsed
                    .glyph_index(c)
                    .filter(|g| g.0 != 0)
                    .map(|g| (i, g.0))
            })
        });
        self.cache.insert((family.clone(), c), r);
        r
    }

    /// The parsed face `i`.
    pub fn face(&self, i: usize) -> &ttf_parser::Face<'static> {
        &self.faces[i].parsed
    }

    /// Index of the face named `name`.
    pub fn index_of(&self, name: &str) -> Option<usize> {
        self.by_name.get(name).copied()
    }
}

/// Font files loaded at run time (system CJK fonts) are kept for the program's
/// lifetime, once per name.
fn leak_once(name: &str, data: &[u8]) -> &'static [u8] {
    static KEPT: std::sync::Mutex<Vec<(String, usize, &'static [u8])>> =
        std::sync::Mutex::new(Vec::new());
    let Ok(mut kept) = KEPT.lock() else {
        return Box::leak(data.to_vec().into_boxed_slice());
    };
    if let Some((_, _, d)) = kept.iter().find(|(n, l, _)| n == name && *l == data.len()) {
        return d;
    }
    let d: &'static [u8] = Box::leak(data.to_vec().into_boxed_slice());
    kept.push((name.to_owned(), data.len(), d));
    d
}

fn invisible(c: char) -> bool {
    c.is_control() || matches!(c, '\u{200B}'..='\u{200F}' | '\u{2060}'..='\u{2064}' | '\u{FEFF}')
}

/// Converts a laid-out job at `origin` into text, background and decoration ops.
pub fn job_ops(
    job: &LayoutJob,
    origin: Pos2,
    fonts: &mut FontsView<'_>,
    faces: &mut FaceTable,
    out: &mut Vec<Op>,
) {
    let galley = fonts.layout_job(job.clone());
    let text = &job.text;
    let starts: Vec<usize> = text.char_indices().map(|(i, _)| i).collect();
    let section_of = |byte: usize| {
        job.sections
            .iter()
            .position(|s| s.byte_range.start.0 <= byte && byte < s.byte_range.end.0)
    };
    let mut k = 0usize;
    for row in &galley.rows {
        let row_top = origin.y + row.pos.y;
        let row_h = row.row.size.y;
        let mut cur: Option<Op> = None;
        for g in &row.row.glyphs {
            let ci = k;
            k += 1;
            let Some(&byte) = starts.get(ci) else {
                continue;
            };
            let Some(si) = section_of(byte) else { continue };
            let fmt = &job.sections[si].format;
            let x = origin.x + row.pos.x + g.pos.x;
            let baseline = row_top + g.pos.y;
            if fmt.background != Color32::TRANSPARENT {
                out.push(Op::Rect {
                    rect: Rect::from_min_size(pos2(x, row_top), vec2(g.advance_width, row_h)),
                    fill: fmt.background,
                    stroke: None,
                    radius: 0.0,
                });
            }
            if fmt.underline.width > 0.0 {
                let y = baseline + 0.12 * fmt.font_id.size;
                out.push(Op::Line {
                    a: pos2(x, y),
                    b: pos2(x + g.advance_width, y),
                    width: fmt.underline.width.max(0.6),
                    color: fmt.underline.color,
                });
            }
            if fmt.strikethrough.width > 0.0 {
                let y = baseline - 0.3 * fmt.font_id.size;
                out.push(Op::Line {
                    a: pos2(x, y),
                    b: pos2(x + g.advance_width, y),
                    width: fmt.strikethrough.width.max(0.6),
                    color: fmt.strikethrough.color,
                });
            }
            if invisible(g.chr) {
                continue;
            }
            let Some((face, gid)) = faces.resolve(&fmt.font_id.family, g.chr) else {
                continue;
            };
            let size = fmt.font_id.size * faces.faces[face].scale;
            let color = fmt.color;
            let same = matches!(&cur, Some(Op::Text { face: f, size: s, color: c, baseline: b, .. })
                if *f == face && (*s - size).abs() < 0.01 && *c == color && (*b - baseline).abs() < 0.01);
            if !same && let Some(op) = cur.take() {
                out.push(op);
            }
            let op = cur.get_or_insert_with(|| Op::Text {
                face,
                size,
                color,
                baseline,
                glyphs: Vec::new(),
                text: String::new(),
            });
            if let Op::Text { glyphs, text, .. } = op {
                let start = text.len();
                text.push(g.chr);
                if let Some(prev) = glyphs.last_mut() {
                    prev.adv = x - prev.x;
                }
                glyphs.push(PGlyph {
                    gid,
                    x,
                    adv: g.advance_width,
                    range: start..text.len(),
                });
            }
        }
        if let Some(op) = cur.take() {
            out.push(op);
        }
        if row.ends_with_newline {
            k += 1;
        }
    }
}

/// Converts a block layout (item coordinates) into ops.
pub fn layout_ops(l: &BlockLayout, fonts: &mut FontsView<'_>, faces: &mut FaceTable) -> Vec<Op> {
    let mut out = Vec::new();
    for d in &l.decos {
        match d {
            Deco::Rect {
                rect,
                fill,
                stroke,
                radius,
            } => out.push(Op::Rect {
                rect: *rect,
                fill: *fill,
                stroke: (stroke.width > 0.0 && stroke.color != Color32::TRANSPARENT)
                    .then_some((stroke.width, stroke.color)),
                radius: *radius,
            }),
            Deco::Line { a, b, stroke } => out.push(Op::Line {
                a: *a,
                b: *b,
                width: stroke.width,
                color: stroke.color,
            }),
            Deco::Circle {
                center,
                radius,
                fill,
                stroke,
            } => out.push(Op::Circle {
                c: *center,
                r: *radius,
                fill: *fill,
                stroke: (stroke.width > 0.0).then_some((stroke.width, stroke.color)),
            }),
            Deco::Image { rect, src } => out.push(Op::Image {
                rect: *rect,
                src: src.clone(),
            }),
            Deco::Math { rect, math } => {
                let t = tiny_skia_path::Transform::from_translate(rect.min.x, rect.min.y);
                for f in &math.scene.fills {
                    let color = f
                        .color
                        .map_or(math.color, |[r, g, b]| Color32::from_rgb(r, g, b));
                    if let Some(p) = f.path.clone().transform(t) {
                        out.push(Op::Path { path: p, color });
                    }
                }
            }
            Deco::Squiggle { .. } => {}
        }
    }
    for r in &l.runs {
        job_ops(&r.job, pos2(r.pos.x, r.pos.y), fonts, faces, &mut out);
    }
    for h in &l.hits {
        let target = match &h.action {
            HitAction::Link(u) | HitAction::InlineLink(u) => u.clone(),
            HitAction::Footnote(label) => format!("#fn:{label}"),
            HitAction::ToggleTask(_) => continue,
        };
        out.push(Op::Link {
            rect: h.rect,
            target,
        });
    }
    out
}

/// Row bottoms inside a layout: the positions where it may be split across
/// pages (item coordinates, sorted).
pub fn break_points(l: &BlockLayout, fonts: &mut FontsView<'_>) -> Vec<f32> {
    let mut out = Vec::new();
    for r in &l.runs {
        let g = fonts.layout_job(r.job.clone());
        for row in &g.rows {
            out.push(r.pos.y + row.pos.y + row.row.size.y);
        }
    }
    out.sort_by(f32::total_cmp);
    out.dedup_by(|a, b| (*a - *b).abs() < 0.5);
    out
}
