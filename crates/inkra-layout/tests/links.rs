//! Clickable regions laid out with the text: equation references (MD-008) and
//! theme-driven decorations.

use epaint::text::{Fonts, TextOptions};
use inkra_doc::Doc;
use inkra_layout::{Ctx, HitAction, Mode, Style, ViewLayout};

#[test]
fn eqref_in_inline_math_links_to_its_label() {
    let mut fonts = Fonts::new(TextOptions::default(), inkra_layout::fonts::definitions());
    let mut f = fonts.with_pixels_per_point(1.0);
    let o = inkra_markdown::ParseOptions {
        inline_math: true,
        ..Default::default()
    };
    let src = "$$\nx = 1 \\label{one}\n$$\n\nSee $\\eqref{one}$ above.\n";
    let doc = Doc::from_str(src, o);
    let style = Style::default();
    let mut v = ViewLayout::new();
    v.sync(&doc, Mode::Live, 600.0, &style, 0);
    let cx = Ctx::simple(&doc, &style, 0);
    let i = v.item_of(src.find("See").unwrap());
    v.ensure(i, &cx, &mut f);
    let l = v.layout(i).unwrap();
    assert!(
        l.hits
            .iter()
            .any(|h| h.action == HitAction::InlineLink("#one".into()) && h.rect.width() > 4.0),
        "{:?}",
        l.hits
    );
}

#[test]
fn details_summary_folds_and_unfolds() {
    let mut fonts = Fonts::new(TextOptions::default(), inkra_layout::fonts::definitions());
    let mut f = fonts.with_pixels_per_point(1.0);
    let src = "Intro\n\n<details><summary>More</summary><p>hidden text</p></details>\n";
    let doc = Doc::from_str(src, Default::default());
    let style = Style::default();
    let shown =
        |v: &mut ViewLayout, f: &mut epaint::text::FontsView<'_>| -> (String, Option<u64>) {
            v.sync(&doc, Mode::Live, 600.0, &style, 0);
            v.begin_frame(0.0);
            let cx = Ctx::simple(&doc, &style, 0);
            let i = v.item_of(src.find("<details").unwrap());
            v.ensure(i, &cx, f);
            let l = v.layout(i).unwrap();
            let text = l
                .runs
                .iter()
                .map(|r| r.job.text.clone())
                .collect::<Vec<_>>()
                .join("|");
            let key = l.hits.iter().find_map(|h| match h.action {
                HitAction::ToggleDetails(k) => Some(k),
                _ => None,
            });
            (text, key)
        };
    let mut v = ViewLayout::new();
    let (text, key) = shown(&mut v, &mut f);
    assert!(text.contains("More") && !text.contains("hidden"), "{text}");
    inkra_layout::live::toggle_details(key.expect("summary is clickable"));
    let (text, _) = shown(&mut v, &mut f);
    assert!(text.contains("hidden"), "{text}");
}
