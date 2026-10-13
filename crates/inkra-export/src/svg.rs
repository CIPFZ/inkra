//! Vector scenes (formulas, diagrams) as SVG (EX-001: "公式输出为 SVG 内联").

use std::fmt::Write;

use tiny_skia_path::{Path, PathSegment};

/// SVG path data of a path.
pub fn path_data(p: &Path) -> String {
    let mut d = String::new();
    let r = |v: f32| (v * 100.0).round() / 100.0;
    for seg in p.segments() {
        let _ = match seg {
            PathSegment::MoveTo(a) => write!(d, "M{} {}", r(a.x), r(a.y)),
            PathSegment::LineTo(a) => write!(d, "L{} {}", r(a.x), r(a.y)),
            PathSegment::QuadTo(b, a) => write!(d, "Q{} {} {} {}", r(b.x), r(b.y), r(a.x), r(a.y)),
            PathSegment::CubicTo(b, c, a) => write!(
                d,
                "C{} {} {} {} {} {}",
                r(b.x),
                r(b.y),
                r(c.x),
                r(c.y),
                r(a.x),
                r(a.y)
            ),
            PathSegment::Close => write!(d, "Z"),
        };
    }
    d
}

fn hex([r, g, b]: [u8; 3]) -> String {
    format!("#{r:02x}{g:02x}{b:02x}")
}

/// A formula as inline SVG; uncolored ink uses `currentColor` so it follows the
/// surrounding text. `baseline` aligns it with the text (CSS `vertical-align`).
pub fn math_svg(scene: &inkra_math::Scene, label: &str) -> String {
    let mut s = String::new();
    let _ = write!(
        s,
        r#"<svg xmlns="http://www.w3.org/2000/svg" class="inkra-math" role="img" aria-label="{}" width="{:.2}" height="{:.2}" viewBox="0 0 {:.2} {:.2}" style="vertical-align:{:.2}px">"#,
        escape_attr(label),
        scene.width,
        scene.height,
        scene.width,
        scene.height,
        -(scene.height - scene.baseline)
    );
    for f in &scene.fills {
        let fill = f.color.map_or_else(|| "currentColor".to_owned(), hex);
        let _ = write!(s, r#"<path fill="{fill}" d="{}"/>"#, path_data(&f.path));
    }
    s.push_str("</svg>");
    s
}

/// A diagram as an SVG element.
pub fn diagram_svg(scene: &inkra_diagram::Scene) -> String {
    let mut s = String::new();
    let _ = write!(
        s,
        r#"<svg xmlns="http://www.w3.org/2000/svg" class="inkra-diagram" width="{:.0}" height="{:.0}" viewBox="0 0 {:.2} {:.2}">"#,
        scene.width.ceil(),
        scene.height.ceil(),
        scene.width,
        scene.height
    );
    for f in &scene.fills {
        let [r, g, b, a] = f.color;
        let opacity = if a < 255 {
            format!(r#" fill-opacity="{:.3}""#, f32::from(a) / 255.0)
        } else {
            String::new()
        };
        let _ = write!(
            s,
            r#"<path fill="{}"{opacity} d="{}"/>"#,
            hex([r, g, b]),
            path_data(&f.path)
        );
    }
    s.push_str("</svg>");
    s
}

/// Escapes an attribute value.
pub fn escape_attr(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn math_and_diagram_svg() {
        let r = inkra_math::render_uncached(&inkra_math::Request::new(r"\frac{a}{b}", true, 16.0));
        let svg = math_svg(&r.scene, r"\frac{a}{b}");
        assert!(svg.starts_with("<svg") && svg.contains("<path") && svg.ends_with("</svg>"));
        assert!(svg.contains("currentColor"));
        let d = inkra_diagram::render_uncached(
            "pie\n\"a\" : 1\n\"b\" : 2",
            &inkra_diagram::Palette::light(),
        )
        .unwrap();
        assert!(diagram_svg(&d).contains("<path"));
    }
}
