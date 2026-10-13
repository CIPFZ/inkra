//! # inkra-render
//!
//! **Layer L3b** (see SPEC 2.2).
//!
//! Painting: turns block layouts, selections and carets into epaint shapes
//! (`offset` maps document coordinates to screen coordinates).

use epaint::{Color32, CornerRadius, Pos2, Rect, Shape, Stroke, StrokeKind, Vec2};
use inkra_layout::{BlockLayout, Deco, Run};

/// Textures owned by the host.
pub trait Textures {
    /// The texture of an image source.
    fn image(&self, src: &str) -> Option<epaint::TextureId>;
    /// The texture of a rasterized formula.
    fn math(&self, m: &inkra_layout::MathScene) -> Option<epaint::TextureId>;
}

/// Rasterizes a vector scene (formula, diagram) at `ppp` pixels per point;
/// fills without an explicit color use `color`.
pub fn rasterize(scene: &inkra_math::Scene, color: Color32, ppp: f32) -> epaint::ColorImage {
    let w = (scene.width * ppp).ceil().max(1.0) as u32;
    let h = (scene.height * ppp).ceil().max(1.0) as u32;
    let Some(mut pm) = tiny_skia::Pixmap::new(w, h) else {
        return epaint::ColorImage::new([1, 1], vec![Color32::TRANSPARENT]);
    };
    let ts = tiny_skia::Transform::from_scale(ppp, ppp);
    for f in &scene.fills {
        let mut p = tiny_skia::Paint::default();
        let [r, g, b] = f.color.unwrap_or([color.r(), color.g(), color.b()]);
        p.set_color_rgba8(r, g, b, if f.color.is_some() { 255 } else { color.a() });
        p.anti_alias = true;
        pm.fill_path(&f.path, &p, tiny_skia::FillRule::Winding, ts, None);
    }
    // tiny-skia stores premultiplied RGBA, as does epaint's Color32.
    let px: Vec<Color32> = pm
        .data()
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| Color32::from_rgba_premultiplied(c[0], c[1], c[2], c[3]))
        .collect();
    epaint::ColorImage::new([w as usize, h as usize], px)
}

/// Paints the decorations of `layout` whose block top is at `top` (document y).
/// Images without a texture are drawn as placeholders.
pub fn paint_decos(
    out: &mut Vec<Shape>,
    layout: &BlockLayout,
    top: f32,
    offset: Vec2,
    alpha: f32,
    textures: &dyn Textures,
) {
    let o = Vec2::new(0.0, top) + offset;
    let fade = |c: Color32| {
        if alpha < 1.0 {
            c.gamma_multiply(alpha)
        } else {
            c
        }
    };
    for d in &layout.decos {
        match d {
            Deco::Rect {
                rect,
                fill,
                stroke,
                radius,
            } => {
                let r = rect.translate(o);
                out.push(Shape::rect_filled(
                    r,
                    CornerRadius::same(radius.round() as u8),
                    fade(*fill),
                ));
                if stroke.width > 0.0 {
                    out.push(Shape::rect_stroke(
                        r,
                        CornerRadius::same(radius.round() as u8),
                        Stroke::new(stroke.width, fade(stroke.color)),
                        StrokeKind::Inside,
                    ));
                }
            }
            Deco::Line { a, b, stroke } => {
                out.push(Shape::line_segment(
                    [*a + o, *b + o],
                    Stroke::new(stroke.width, fade(stroke.color)),
                ));
            }
            Deco::Image { rect, src } => {
                let r = rect.translate(o);
                match textures.image(src) {
                    Some(tex) => {
                        let tint = fade(Color32::WHITE);
                        out.push(Shape::image(
                            tex,
                            r,
                            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                            tint,
                        ));
                    }
                    None => {
                        out.push(Shape::rect_filled(
                            r,
                            CornerRadius::same(4),
                            fade(Color32::from_gray(232)),
                        ));
                        out.push(Shape::rect_stroke(
                            r,
                            CornerRadius::same(4),
                            Stroke::new(1.0, fade(Color32::from_gray(200))),
                            StrokeKind::Inside,
                        ));
                    }
                }
            }
            Deco::Squiggle { a, b, color } => {
                // A zig-zag of 2 px amplitude, 4 px period.
                let mut pts = Vec::new();
                let mut x = a.x;
                let mut up = true;
                while x <= b.x {
                    pts.push(Pos2::new(x, a.y + if up { 0.0 } else { 2.0 }) + o);
                    x += 2.0;
                    up = !up;
                }
                if pts.len() >= 2 {
                    out.push(Shape::line(pts, Stroke::new(1.0, fade(*color))));
                }
            }
            Deco::Math { rect, math } => {
                if let Some(tex) = textures.math(math) {
                    let r = rect.translate(o);
                    let tint = fade(Color32::WHITE);
                    out.push(Shape::image(
                        tex,
                        r,
                        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                        tint,
                    ));
                }
            }
            Deco::Circle {
                center,
                radius,
                fill,
                stroke,
            } => {
                out.push(Shape::circle_filled(*center + o, *radius, fade(*fill)));
                if stroke.width > 0.0 {
                    out.push(Shape::circle_stroke(
                        *center + o,
                        *radius,
                        Stroke::new(stroke.width, fade(stroke.color)),
                    ));
                }
            }
        }
    }
}

/// Paints text runs.
pub fn paint_runs(
    out: &mut Vec<Shape>,
    runs: &[Run<'_>],
    offset: Vec2,
    fallback: Color32,
    alpha: f32,
) {
    for run in runs {
        let mut shape = epaint::TextShape::new(run.origin + offset, run.galley.clone(), fallback);
        if alpha < 1.0 {
            shape.opacity_factor = alpha;
        }
        out.push(Shape::Text(shape));
    }
}

/// Paints selection rectangles.
pub fn paint_selection(out: &mut Vec<Shape>, rects: &[Rect], offset: Vec2, color: Color32) {
    for r in rects {
        out.push(Shape::rect_filled(
            r.translate(offset),
            CornerRadius::same(2),
            color,
        ));
    }
}

/// Paints a caret.
pub fn paint_caret(out: &mut Vec<Shape>, rect: Rect, offset: Vec2, color: Color32, width: f32) {
    let r = rect.translate(offset);
    let x = r.min.x.round();
    out.push(Shape::rect_filled(
        Rect::from_min_max(
            Pos2::new(x - width / 2.0, r.min.y),
            Pos2::new(x + width / 2.0, r.max.y),
        ),
        CornerRadius::ZERO,
        color,
    ));
}
