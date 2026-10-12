//! Parser benchmarks (TS-006): full parse and single-character incremental edits
//! on a ~1 MB document (M1 budget: < 2 ms per edit).

use criterion::{Criterion, criterion_group, criterion_main};
use inkra_markdown::{Document, ParseOptions};

/// A realistic ~1 MB document.
pub fn big_doc() -> String {
    let chunk = "## Section heading\n\nSome *emphasis*, **strong**, `code`, a [link](https://example.com) and 中文混排的段落，\
包含一些标点。More text follows here to make the paragraph a bit longer than one line.\n\
Second line of the paragraph with ~~strike~~ and an image ![alt](img.png).\n\n\
- item one\n- item two with **bold**\n  - nested item\n- [ ] task\n\n> A quote with *style*\n> continued\n\n\
```rust\nfn main() {\n    println!(\"hello\");\n}\n```\n\n| a | b | c |\n|---|:-:|--:|\n| 1 | 2 | 3 |\n| x | y | z |\n\n";
    let mut s = String::with_capacity(1 << 20);
    while s.len() < 1 << 20 {
        s.push_str(chunk);
    }
    s
}

fn benches(c: &mut Criterion) {
    let src = big_doc();
    let o = ParseOptions::default();
    c.bench_function("parse_1mb", |b| {
        b.iter(|| inkra_markdown::parse(src.as_str(), &o))
    });
    let doc = Document::parse(src.as_str(), o.clone());
    let mid = src.len() / 2;
    let at = src[mid..].find("emphasis").map(|p| mid + p).unwrap_or(mid);
    let mut edited = src.clone();
    edited.insert(at, 'x');
    c.bench_function("edit_1mb_char", |b| {
        b.iter_batched(
            || doc.clone(),
            |mut d| d.edit(edited.as_str(), at..at, 1),
            criterion::BatchSize::LargeInput,
        )
    });
}

criterion_group!(group, benches);
criterion_main!(group);
