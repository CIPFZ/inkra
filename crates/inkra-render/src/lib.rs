//! # inkra-render
//!
//! **Layer L3b** (see SPEC 2.2).
//!
//! Painting: turns block layouts, selections and carets into epaint shapes
//! (`offset` maps document coordinates to screen coordinates).

use epaint::{Color32, CornerRadius, Pos2, Rect, Shape, Stroke, StrokeKind, Vec2};
use inkra_layout::{BlockLayout, Deco, Run};

/// Looks up the texture of an image source: `(texture, uv rect)`.
pub type ImageLookup<'a> = &'a dyn Fn(&str) -> Option<epaint::TextureId>;

/// Paints the decorations of `layout` whose block top is at `top` (document y).
/// Images without a texture are drawn as placeholders.
pub fn paint_decos(
    out: &mut Vec<Shape>,
    layout: &BlockLayout,
    top: f32,
    offset: Vec2,
    alpha: f32,
    images: ImageLookup<'_>,
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
                match images(src) {
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
