//! Renders the diagrams of a Markdown file (```mermaid fences) to a PNG sheet:
//! `cargo run -p inkra-diagram --example diagram_render -- out.png docs/acceptance/diagram-corpus.md`

use inkra_diagram::{Palette, render_uncached};
use tiny_skia::{Color, FillRule, Paint, Pixmap, Transform};

static INTER: &[u8] = include_bytes!("../../../assets/fonts/Inter-Regular.ttf");

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let out = args
        .get(1)
        .cloned()
        .unwrap_or_else(|| "diagrams.png".into());
    let text = std::fs::read_to_string(
        args.get(2)
            .map(String::as_str)
            .unwrap_or("docs/acceptance/diagram-corpus.md"),
    )
    .unwrap_or_default();
    inkra_diagram::draw::set_fonts(vec![(INTER, 0)]);
    let mut blocks = Vec::new();
    let mut cur: Option<String> = None;
    for l in text.lines() {
        match (&mut cur, l.trim()) {
            (None, "```mermaid") => cur = Some(String::new()),
            (Some(_), "```") => blocks.push(cur.take().unwrap_or_default()),
            (Some(b), _) => {
                b.push_str(l);
                b.push('\n');
            }
            _ => {}
        }
    }
    let pal = Palette::light();
    let scenes: Vec<_> = blocks.iter().map(|b| render_uncached(b, &pal)).collect();
    let cols = 3;
    let cell_w = scenes
        .iter()
        .filter_map(|s| s.as_ref().ok())
        .map(|s| s.width)
        .fold(200.0f32, f32::max)
        .min(700.0)
        + 20.0;
    let rows = scenes.len().div_ceil(cols);
    let mut row_h = vec![0.0f32; rows];
    for (i, s) in scenes.iter().enumerate() {
        let h = s.as_ref().map_or(40.0, |s| s.height);
        row_h[i / cols] = row_h[i / cols].max(h + 20.0);
    }
    let total_h: f32 = row_h.iter().sum();
    let mut pm =
        Pixmap::new((cell_w * cols as f32) as u32, total_h.max(10.0) as u32).expect("pixmap");
    pm.fill(Color::WHITE);
    let mut y = 0.0;
    for (r, rh) in row_h.iter().enumerate() {
        for c in 0..cols {
            let i = r * cols + c;
            let Some(s) = scenes.get(i) else { continue };
            match s {
                Ok(s) => {
                    let scale = (cell_w - 20.0).min(s.width) / s.width.max(1.0);
                    for f in &s.fills {
                        let mut p = Paint::default();
                        p.set_color_rgba8(f.color[0], f.color[1], f.color[2], f.color[3]);
                        p.anti_alias = true;
                        pm.fill_path(
                            &f.path,
                            &p,
                            FillRule::Winding,
                            Transform::from_row(
                                scale,
                                0.0,
                                0.0,
                                scale,
                                c as f32 * cell_w + 10.0,
                                y + 10.0,
                            ),
                            None,
                        );
                    }
                }
                Err(e) => eprintln!("diagram {i}: {e}"),
            }
        }
        y += rh;
    }
    pm.save_png(&out).expect("save");
    eprintln!("{} diagrams", blocks.len());
}
