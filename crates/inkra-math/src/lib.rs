//! # inkra-math
//!
//! **Layer L2** (see SPEC 2.2).
//!
//! TeX math typesetting: parse -> box layout -> drawing commands. The output is
//! a vector [`Scene`] (glyph outlines and rules as paths) that the renderer
//! rasterizes and the PDF exporter embeds. Unknown commands never fail the
//! formula: they are drawn as red source text and reported (SPEC 2.4).

pub mod font;
pub mod layout;
pub mod mathml;
pub mod parse;
pub mod symbols;

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use tiny_skia_path::{Path, PathBuilder, Rect, Transform};

use layout::{Ctx, Elem, Layout, MBox};

/// A filled path; `color: None` uses the text color.
#[derive(Debug, Clone, PartialEq)]
pub struct Fill {
    /// Path in scene coordinates (px, y down, origin top-left).
    pub path: Path,
    /// Explicit color (`\color`).
    pub color: Option<[u8; 3]>,
}

/// A rendered formula.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Scene {
    /// Width in px.
    pub width: f32,
    /// Height in px (ascent + descent).
    pub height: f32,
    /// Distance from the top to the baseline.
    pub baseline: f32,
    /// Shapes.
    pub fills: Vec<Fill>,
}

/// A typeset formula with its diagnostics.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Rendered {
    /// The formula.
    pub scene: Arc<Scene>,
    /// The `\tag` (display formulas), typeset as text with parentheses.
    pub tag: Option<Arc<Scene>>,
    /// The tag text, if any (for numbering and references).
    pub tag_text: Option<String>,
    /// `\label`s.
    pub labels: Vec<String>,
    /// Whether the formula is numbered under the AMS rule (MD-008).
    pub numbered_env: bool,
    /// `\notag`.
    pub notag: bool,
    /// Problems; non-empty means parts are shown as red source.
    pub errors: Vec<String>,
}

/// What to typeset.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Request {
    /// TeX source.
    pub tex: String,
    /// Display style (`$$…$$`).
    pub display: bool,
    /// Font size in hundredths of a px (keeps the request hashable).
    pub size_centi: u32,
    /// Automatic equation number to show when the formula has no `\tag`.
    pub number: Option<u32>,
    /// Label → number, for `\ref` and `\eqref`.
    pub refs: Vec<(String, String)>,
}

impl Request {
    /// A request at `size` px.
    pub fn new(tex: &str, display: bool, size: f32) -> Self {
        Self {
            tex: tex.to_owned(),
            display,
            size_centi: (size * 100.0).round() as u32,
            number: None,
            refs: Vec::new(),
        }
    }

    fn size(&self) -> f32 {
        self.size_centi as f32 / 100.0
    }
}

fn rect_path(x: f32, y: f32, w: f32, h: f32) -> Option<Path> {
    Rect::from_xywh(x, y, w.max(0.01), h.max(0.01)).map(PathBuilder::from_rect)
}

/// Flattens a box tree into fills. `(x, baseline_y)` is the box origin in scene
/// coordinates (y down).
fn flatten(b: &MBox, x: f32, base_y: f32, color: Option<[u8; 3]>, out: &mut Vec<Fill>) {
    for (dx, dy, e) in &b.items {
        let (ex, ey) = (x + dx, base_y - dy);
        match e {
            Elem::Glyph { face, gid, size } => {
                let Some(p) = font::outline(*face, *gid) else {
                    continue;
                };
                let s = size / font::upem(*face);
                if let Some(p) = p.transform(Transform::from_row(s, 0.0, 0.0, s, ex, ey)) {
                    out.push(Fill { path: p, color });
                }
            }
            Elem::Rule { w, h } => {
                if let Some(p) = rect_path(ex, ey - h, *w, *h) {
                    out.push(Fill { path: p, color });
                }
            }
            Elem::Box(inner) => flatten(inner, ex, ey, color, out),
            Elem::Colored(c, inner) => flatten(inner, ex, ey, Some(*c), out),
            Elem::Frame { w, h, d, t } => {
                let top = ey - h;
                let total = h + d;
                for (rx, ry, rw, rh) in [
                    (ex, top, *w, *t),
                    (ex, ey + d - t, *w, *t),
                    (ex, top, *t, total),
                    (ex + w - t, top, *t, total),
                ] {
                    if let Some(p) = rect_path(rx, ry, rw, rh) {
                        out.push(Fill { path: p, color });
                    }
                }
            }
        }
    }
}

fn scene_of(b: &MBox) -> Scene {
    let mut fills = Vec::new();
    // A little padding so antialiased edges are not clipped.
    let pad = 1.0;
    flatten(b, pad, pad + b.h, None, &mut fills);
    Scene {
        width: b.w + 2.0 * pad,
        height: b.h + b.d + 2.0 * pad,
        baseline: pad + b.h,
        fills,
    }
}

/// Typesets `req` (uncached).
pub fn render_uncached(req: &Request) -> Rendered {
    let parsed = parse::parse_with_refs(&req.tex, &req.refs);
    let mut lay = Layout::new();
    let ctx = Ctx::new(req.size(), req.display);
    let b = lay.list(&parsed.list, ctx);
    let mut errors = parsed.errors.clone();
    errors.extend(lay.errors.iter().cloned());
    let scene = Arc::new(scene_of(&b));
    let tag_text = match (&parsed.tag, req.number) {
        (Some((t, true)), _) => Some(t.clone()),
        (Some((t, false)), _) => Some(format!("({t})")),
        (None, Some(n)) if !parsed.notag => Some(format!("({n})")),
        _ => None,
    };
    let tag = tag_text.as_ref().filter(|_| req.display).map(|t| {
        let tb = lay.list(
            &[parse::Node::Text {
                text: t.clone(),
                bold: false,
                italic: false,
            }],
            ctx,
        );
        Arc::new(scene_of(&tb))
    });
    Rendered {
        scene,
        tag,
        tag_text: parsed.tag.as_ref().map(|t| t.0.clone()),
        labels: parsed.labels,
        numbered_env: parsed.numbered_env,
        notag: parsed.notag,
        errors,
    }
}

/// The whole source as an error node (shown as red TeX).
fn fallback(req: &Request) -> Rendered {
    let error = "internal error while typesetting".to_owned();
    let scene = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut lay = Layout::new();
        let b = lay.list(
            &[parse::Node::Error(req.tex.clone())],
            Ctx::new(req.size(), req.display),
        );
        scene_of(&b)
    }))
    .unwrap_or_default();
    Rendered {
        scene: Arc::new(scene),
        tag: None,
        tag_text: None,
        labels: Vec::new(),
        numbered_env: false,
        notag: false,
        errors: vec![error],
    }
}

const CACHE_MAX: usize = 4096;

static CACHE: OnceLock<Mutex<HashMap<Request, Arc<Rendered>>>> = OnceLock::new();

/// Typesets `req`, caching the result (SPEC 2.4: by source, size and color).
pub fn render(req: &Request) -> Arc<Rendered> {
    let cache = CACHE.get_or_init(Default::default);
    if let Ok(c) = cache.lock()
        && let Some(r) = c.get(req)
    {
        return r.clone();
    }
    // NF-002: a typesetting bug shows the source in red instead of taking the
    // frame down; the result is cached like any other.
    let r = Arc::new(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| render_uncached(req)))
            .unwrap_or_else(|_| fallback(req)),
    );
    if let Ok(mut c) = cache.lock() {
        if c.len() >= CACHE_MAX {
            c.clear();
        }
        c.insert(req.clone(), r.clone());
    }
    r
}

/// Whether `tex` parses without errors (for diagnostics).
pub fn check(tex: &str) -> Vec<String> {
    parse::parse(tex).errors
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_basic_formulas() {
        for tex in [
            "x",
            r"\frac{a+b}{c}",
            r"\sqrt{x^2+y^2}",
            r"\sum_{i=1}^{n} i^2 = \frac{n(n+1)(2n+1)}{6}",
            r"\int_0^\infty e^{-x^2}\,dx = \frac{\sqrt\pi}{2}",
            r"\left( \frac{1}{2} \right)^2",
            r"\begin{pmatrix} 1 & 0 \\ 0 & 1 \end{pmatrix}",
            r"f(x) = \begin{cases} x & x \ge 0 \\ -x & x < 0 \end{cases}",
            r"\hat{x} + \vec{v} + \overline{AB} + \widehat{xyz}",
            r"\text{速度} = \frac{s}{t}",
        ] {
            let r = render_uncached(&Request::new(tex, true, 20.0));
            assert!(r.errors.is_empty(), "{tex}: {:?}", r.errors);
            assert!(
                r.scene.width > 5.0 && r.scene.height > 5.0,
                "{tex}: {:?}",
                (r.scene.width, r.scene.height)
            );
            assert!(!r.scene.fills.is_empty(), "{tex}");
        }
    }

    #[test]
    fn display_operators_are_larger_and_fractions_taller() {
        let t = render_uncached(&Request::new(r"\sum x", false, 20.0)).scene;
        let d = render_uncached(&Request::new(r"\sum x", true, 20.0)).scene;
        assert!(d.height > t.height * 1.2, "{} vs {}", d.height, t.height);
        let f = render_uncached(&Request::new(r"\frac{a}{b}", true, 20.0)).scene;
        let x = render_uncached(&Request::new(r"a", true, 20.0)).scene;
        assert!(f.height > x.height * 1.8);
        // Big delimiters grow with their content.
        let small = render_uncached(&Request::new(r"\left( x \right)", true, 20.0)).scene;
        let big = render_uncached(&Request::new(
            r"\left( \frac{\frac{a}{b}}{\frac{c}{d}} \right)",
            true,
            20.0,
        ))
        .scene;
        assert!(big.height > small.height * 2.0);
    }

    #[test]
    fn errors_render_in_red_without_panicking() {
        let r = render_uncached(&Request::new(r"a \unknowncmd b", false, 16.0));
        assert_eq!(r.errors.len(), 1);
        assert!(r.scene.fills.iter().any(|f| f.color == Some([220, 38, 38])));
        for bad in [
            r"\frac{",
            r"}}}",
            r"\left(",
            r"\begin{matrix} a & ",
            r"x^^^",
            r"\sqrt[",
            "",
        ] {
            let _ = render_uncached(&Request::new(bad, true, 16.0));
        }
    }

    /// M5 acceptance: every formula of the corpus renders without errors.
    #[test]
    fn corpus() {
        let text = include_str!("../../../docs/acceptance/math-corpus.md");
        let items: Vec<&str> = text.lines().filter_map(|l| l.strip_prefix("- ")).collect();
        assert!(items.len() >= 200, "{}", items.len());
        let mut bad = Vec::new();
        for tex in &items {
            let r = render_uncached(&Request::new(tex, true, 18.0));
            if !r.errors.is_empty() || r.scene.fills.is_empty() {
                bad.push(format!("{tex}: {:?}", r.errors));
            }
        }
        assert!(bad.is_empty(), "{} failing:\n{}", bad.len(), bad.join("\n"));
    }

    #[test]
    fn tags_and_numbers() {
        let r = render_uncached(&Request {
            number: Some(3),
            ..Request::new(r"E = mc^2", true, 16.0)
        });
        assert!(r.tag.is_some());
        let r = render_uncached(&Request::new(r"E = mc^2 \tag{A}", true, 16.0));
        assert_eq!(r.tag_text.as_deref(), Some("A"));
        assert!(r.tag.is_some());
        let r = render_uncached(&Request::new(
            r"\begin{equation} x \label{eq} \end{equation}",
            true,
            16.0,
        ));
        assert!(r.numbered_env);
        assert_eq!(r.labels, ["eq"]);
    }
}
