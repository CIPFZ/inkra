//! Coarse performance guard for the M1 budget (release builds only).

use std::time::Instant;

use inkra_markdown::{Document, ParseOptions};

#[path = "../benches/parse.rs"]
#[allow(dead_code, missing_docs)]
mod bench;

#[test]
fn single_char_edit_on_1mb_is_fast() {
    let src = bench::big_doc();
    let o = ParseOptions::default();
    let t = Instant::now();
    let mut doc = Document::parse(src.as_str(), o);
    let full = t.elapsed();
    let mut text = src.clone();
    let mut worst = std::time::Duration::ZERO;
    let mut total = std::time::Duration::ZERO;
    let n = 50;
    for i in 0..n {
        let at = text.len() * (i + 1) / (n + 1);
        let at = (at..text.len())
            .find(|&p| text.is_char_boundary(p))
            .unwrap();
        text.insert(at, 'x');
        let t = Instant::now();
        doc.edit(text.as_str(), at..at, 1);
        let dt = t.elapsed();
        worst = worst.max(dt);
        total += dt;
    }
    eprintln!(
        "1 MB: full parse {full:?}; edit avg {:?}, worst {worst:?}",
        total / n as u32
    );
    if !cfg!(debug_assertions) {
        assert!(worst.as_micros() < 2000, "edit took {worst:?}");
    }
}
