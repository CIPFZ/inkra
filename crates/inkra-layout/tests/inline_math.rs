//! Inline formulas (MD-035) sit on the text baseline, also when they are taller
//! than the line.

use epaint::text::{Fonts, TextOptions};
use inkra_doc::Doc;
use inkra_layout::{Ctx, Deco, Mode, Style, ViewLayout};

#[test]
fn tall_inline_math_shares_the_text_baseline() {
    let mut fonts = Fonts::new(TextOptions::default(), inkra_layout::fonts::definitions());
    let mut f = fonts.with_pixels_per_point(1.0);
    let o = inkra_markdown::ParseOptions {
        inline_math: true,
        ..Default::default()
    };
    for tex in [
        "x",
        "\\frac{a}{b}",
        "x = \\frac{-b \\pm \\sqrt{b^2-4ac}}{2a}",
    ] {
        let src = format!("Text ${tex}$ text.\n");
        let doc = Doc::from_str(&src, o.clone());
        let style = Style::default();
        let mut v = ViewLayout::new();
        v.sync(&doc, Mode::Live, 600.0, &style, usize::MAX);
        let cx = Ctx::simple(&doc, &style, usize::MAX);
        let i = v.item_of(0);
        v.ensure(i, &cx, &mut f);
        let l = v.layout(i).unwrap();
        let run = &l.runs[0];
        let g = f.layout_job(run.job.clone());
        // The first glyph is the `T` of "Text".
        let text_baseline = run.pos.y + g.rows[0].pos.y + g.rows[0].row.glyphs[0].pos.y;
        let math_baseline = l
            .decos
            .iter()
            .find_map(|d| match d {
                Deco::Math { rect, math } => Some(rect.min.y + math.scene.baseline),
                _ => None,
            })
            .expect("a math decoration");
        assert!(
            (math_baseline - text_baseline).abs() < 1.0,
            "{tex}: formula baseline {math_baseline} vs text {text_baseline}"
        );
    }
}
