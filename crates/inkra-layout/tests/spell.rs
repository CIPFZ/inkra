//! Spell-check underlines in live layout (ED-104).

use epaint::text::{Fonts, TextOptions};
use inkra_doc::Doc;
use inkra_layout::{Ctx, Deco, Mode, Style, ViewLayout};

#[test]
fn misspelled_words_get_squiggles() {
    inkra_text::spell::load_now();
    let mut fonts = Fonts::new(TextOptions::default(), inkra_layout::fonts::definitions());
    let mut f = fonts.with_pixels_per_point(1.0);
    let doc = Doc::from_str(
        "This sentense has a speling mistake, but fine.\n",
        Default::default(),
    );
    let style = Style {
        spellcheck: true,
        ..Style::default()
    };
    let mut v = ViewLayout::new();
    v.sync(&doc, Mode::Live, 600.0, &style, 100);
    let cx = Ctx::simple(&doc, &style, 100);
    v.ensure(0, &cx, &mut f);
    let l = v.layout(0).unwrap();
    let n = l
        .decos
        .iter()
        .filter(|d| matches!(d, Deco::Squiggle { .. }))
        .count();
    let segs: Vec<_> = l.runs.iter().map(|r| r.map.squiggles.clone()).collect();
    assert_eq!(n, 2, "{segs:?}");
}
