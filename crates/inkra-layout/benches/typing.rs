//! Typing latency budget (SPEC 2.5: p95 < 16 ms on a 100k-word document): one
//! character typed, the incremental reparse, the view sync and the layout of a
//! screenful of items around the caret.

use criterion::{Criterion, criterion_group, criterion_main};
use epaint::text::{Fonts, TextOptions};
use inkra_doc::Doc;
use inkra_layout::{Ctx, Mode, Style, ViewLayout};

/// About 100k words of mixed Markdown.
fn big_doc() -> String {
    let chunk = "## Section heading\n\nSome *emphasis*, **strong**, `code`, a [link](https://example.com) and 中文混排的段落，\
包含一些标点。More text follows here to make the paragraph a bit longer than one line.\n\n\
- item one\n- item two with **bold**\n  - nested item\n- [ ] task\n\n> A quote with *style*\n\n\
```rust\nfn main() {\n    println!(\"hello\");\n}\n```\n\n| a | b | c |\n|---|:-:|--:|\n| 1 | 2 | 3 |\n\n";
    let mut s = String::new();
    while s.split_whitespace().count() < 100_000 {
        for _ in 0..50 {
            s.push_str(chunk);
        }
    }
    s
}

fn benches(c: &mut Criterion) {
    let mut fonts = Fonts::new(TextOptions::default(), inkra_layout::fonts::definitions());
    let src = big_doc();
    let mut doc = Doc::from_str(&src, Default::default());
    let style = Style::default();
    let mut view = ViewLayout::new();
    let mut caret = src.len() / 2;
    caret = src[caret..].find("emphasis").map_or(caret, |p| caret + p);
    view.sync(&doc, Mode::Live, 760.0, &style, caret);
    c.bench_function("keystroke_100k_words", |b| {
        b.iter(|| {
            doc.apply(
                inkra_doc::Transaction::single(caret..caret, "x"),
                inkra_doc::UndoPolicy::Typing,
            );
            caret += 1;
            view.sync(&doc, Mode::Live, 760.0, &style, caret);
            view.begin_frame(0.0);
            let i = view.item_of(caret);
            let cx = Ctx::simple(&doc, &style, caret);
            let mut f = fonts.with_pixels_per_point(1.0);
            for k in i.saturating_sub(20)..(i + 20).min(view.items().len()) {
                view.ensure(k, &cx, &mut f);
            }
        })
    });
}

criterion_group! {
    name = typing;
    config = Criterion::default().sample_size(60);
    targets = benches
}
criterion_main!(typing);
