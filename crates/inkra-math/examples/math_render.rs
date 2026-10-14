//! Renders formulas to a PNG sheet for visual inspection:
//! `cargo run -p inkra-math --example math_render -- out.png [file-with-one-formula-per-line]`

use inkra_math::{Request, render_uncached};
use tiny_skia::{Color, FillRule, Paint, Pixmap, Transform};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let out = args.get(1).cloned().unwrap_or_else(|| "math.png".into());
    let formulas: Vec<String> = match args.get(2) {
        // Plain lines, or `- ` items of a Markdown list (the acceptance corpus).
        Some(f) => {
            let text = std::fs::read_to_string(f).unwrap_or_default();
            let items: Vec<String> = text
                .lines()
                .filter_map(|l| l.strip_prefix("- "))
                .map(str::to_owned)
                .collect();
            if items.is_empty() {
                text.lines()
                    .filter(|l| !l.trim().is_empty())
                    .map(str::to_owned)
                    .collect()
            } else {
                items
            }
        }
        None => vec![r"\sum_{i=1}^{n} i^2 = \frac{n(n+1)(2n+1)}{6}".into()],
    };
    let size = 22.0;
    let rendered: Vec<_> = formulas
        .iter()
        .map(|t| render_uncached(&Request::new(t, true, size)))
        .collect();
    let w = rendered
        .iter()
        .map(|r| r.scene.width)
        .fold(400.0f32, f32::max) as u32
        + 40;
    let h: f32 = rendered.iter().map(|r| r.scene.height + 16.0).sum::<f32>() + 20.0;
    let mut pm = Pixmap::new(w, h as u32).expect("pixmap");
    pm.fill(Color::WHITE);
    let mut y = 10.0;
    for (r, t) in rendered.iter().zip(&formulas) {
        for f in &r.scene.fills {
            let mut p = Paint::default();
            let c = f.color.unwrap_or([20, 20, 30]);
            p.set_color_rgba8(c[0], c[1], c[2], 255);
            p.anti_alias = true;
            pm.fill_path(
                &f.path,
                &p,
                FillRule::Winding,
                Transform::from_translate(20.0, y),
                None,
            );
        }
        if !r.errors.is_empty() {
            eprintln!("{t}: {:?}", r.errors);
        }
        y += r.scene.height + 16.0;
    }
    pm.save_png(&out).expect("save");
}
