//! Export budget (SPEC 2.5: a 100-page PDF in < 5 s) and the HTML exporter.

use criterion::{Criterion, criterion_group, criterion_main};
use inkra_export::html::{HtmlExport, export_html};
use inkra_export::pdf::{PdfExport, export_pdf};

/// About 120 A4 pages of headings, paragraphs, lists, code, tables and math.
fn long_doc() -> String {
    let mut s = String::new();
    for i in 0..180 {
        s.push_str(&format!("## Section {i}\n\n"));
        for _ in 0..4 {
            s.push_str("Some *emphasis*, **strong**, `code` and a [link](https://example.com). More text follows to make the paragraph longer than a line or two in the page width.\n\n");
        }
        s.push_str("- item one\n- item two\n  - nested\n\n```rust\nfn main() {\n    println!(\"hello\");\n}\n```\n\n");
        s.push_str("| a | b | c |\n|---|:-:|--:|\n| 1 | 2 | 3 |\n\n$$\nx^2 + y^2 = z^2\n$$\n\n");
    }
    s
}

fn benches(c: &mut Criterion) {
    let src = long_doc();
    let pdf = PdfExport::default();
    c.bench_function("pdf_120_pages", |b| {
        b.iter(|| export_pdf(&src, &pdf).expect("pdf"))
    });
    let html = HtmlExport::default();
    let parse = inkra_markdown::ParseOptions::default();
    c.bench_function("html_120_pages", |b| {
        b.iter(|| export_html(&src, &parse, &html))
    });
}

criterion_group! {
    name = export;
    config = Criterion::default().sample_size(10);
    targets = benches
}
criterion_main!(export);
