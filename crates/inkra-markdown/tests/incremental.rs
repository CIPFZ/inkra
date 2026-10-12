//! Incremental re-parsing must equal a full parse (TS-003), and the parser must
//! never panic on arbitrary input (NF-002).

use inkra_markdown::{Document, ParseOptions, parse};
use proptest::prelude::*;

fn check_edit(before: &str, range: std::ops::Range<usize>, insert: &str, o: &ParseOptions) {
    let mut doc = Document::parse(before, o.clone());
    let mut after = before.to_owned();
    after.replace_range(range.clone(), insert);
    doc.edit(after.as_str(), range.clone(), insert.len());
    let full = parse(after.as_str(), o);
    assert_eq!(
        doc.to_blocks(),
        full.blocks,
        "incremental != full\nbefore: {before:?}\nedit: {range:?} -> {insert:?}\nafter: {after:?}"
    );
    assert_eq!(doc.context(), &full.cx);
}

const SAMPLE: &str = "---\ntitle: x\n---\n\n# Title\n\nSome *text* with [a link][ref] and `code`.\n\n- item 1\n- item 2\n\n  continued\n\n> quote\n> more\n\n```rust\nfn main() {}\n```\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n[ref]: /url\n\n$$\nx^2\n$$\n\nTail [^1]\n\n[^1]: note\n";

#[test]
fn targeted_edits() {
    let o = ParseOptions::default();
    let cases: &[(&str, std::ops::Range<usize>, &str)] = &[
        ("a\n\nb\n", 1..2, ""),
        ("- a\n\nb\n", 5..5, "  "),
        ("para\n===\n", 0..0, "x"),
        ("a\n\n```\ncode\n\nmore\n", 7..7, "```\n"),
        ("a\n\n```\ncode\n```\n\nmore\n", 7..11, ""),
        ("x\n\n[r]: /u\n", 0..0, "[r] "),
        ("| a |\n\n|---|\n", 5..6, ""),
        ("---\nfoo\n\nbar\n", 9..9, "---\n"),
        ("> a\nb\n\nc\n", 3..3, "\n>"),
        ("1. a\n2. b\n\n\n3. c\n", 10..11, ""),
        ("<div>\nx\n\ny\n", 7..8, ""),
        ("    code\n\npara\n", 0..4, ""),
    ];
    for (before, range, insert) in cases {
        check_edit(before, range.clone(), insert, &o);
        check_edit(before, range.clone(), insert, &ParseOptions::commonmark());
    }
    // every single-char insertion and deletion in the sample
    for i in 0..SAMPLE.len() {
        if !SAMPLE.is_char_boundary(i) {
            continue;
        }
        for ins in ["\n", "#", "-", "`", ">", " ", "|", "[", "$", "*"] {
            check_edit(SAMPLE, i..i, ins, &o);
        }
        let mut j = i + 1;
        while j <= SAMPLE.len() && !SAMPLE.is_char_boundary(j) {
            j += 1;
        }
        if j <= SAMPLE.len() {
            check_edit(SAMPLE, i..j, "", &o);
        }
    }
}

fn markdownish() -> impl Strategy<Value = String> {
    let atoms = prop::sample::select(vec![
        "# ",
        "## ",
        "- ",
        "* ",
        "1. ",
        "2) ",
        "> ",
        "```",
        "~~~",
        "    ",
        "\t",
        "---",
        "***",
        "===",
        "|",
        "| a | b |",
        "|---|---|",
        "[x]",
        "[ ]",
        "[foo]",
        "[foo]: /url",
        "(url)",
        "![",
        "]",
        "*",
        "**",
        "_",
        "`",
        "~~",
        "$$",
        "$",
        "<div>",
        "</div>",
        "<!--",
        "-->",
        "&amp;",
        "\\",
        "text",
        "中文",
        "word ",
        "\n",
        "\n",
        "\n\n",
        "[^1]",
        "[^1]: ",
        "[TOC]",
        "[!NOTE]",
        "www.x.com",
        "a@b.co",
    ]);
    prop::collection::vec(atoms, 0..60).prop_map(|v| v.concat())
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: std::env::var("PROPTEST_CASES").ok().and_then(|s| s.parse().ok()).unwrap_or(400),
        ..ProptestConfig::default()
    })]

    #[test]
    fn incremental_equals_full(src in markdownish(), at in 0usize..400, del in 0usize..20, ins in markdownish()) {
        let mut start = at.min(src.len());
        while !src.is_char_boundary(start) { start -= 1; }
        let mut end = (start + del).min(src.len());
        while !src.is_char_boundary(end) { end -= 1; }
        let ins: String = ins.chars().take(12).collect();
        for o in [ParseOptions::default(), ParseOptions::commonmark()] {
            check_edit(&src, start..end, &ins, &o);
        }
    }

    #[test]
    fn never_panics_and_spans_are_valid(src in markdownish()) {
        let o = ParseOptions { inline_math: true, highlight: true, superscript: true, subscript: true, ..ParseOptions::default() };
        let p = parse(src.as_str(), &o);
        let mut prev_end = 0;
        for b in &p.blocks {
            prop_assert!(b.span.start >= prev_end || b.span.start == 0, "overlapping top-level blocks");
            prop_assert!(b.span.end <= src.len());
            prev_end = b.span.end;
            b.walk(&mut |b| {
                assert!(b.span.start <= b.span.end && b.span.end <= src.len(), "{:?}", b.span);
                assert!(src.is_char_boundary(b.span.start) && src.is_char_boundary(b.span.end));
                for i in &b.inlines {
                    i.walk(&mut |i| {
                        assert!(i.span.start <= i.span.end && i.span.end <= src.len(), "{:?}", i);
                        assert!(src.is_char_boundary(i.span.start) && src.is_char_boundary(i.span.end));
                    });
                }
            });
        }
        let _ = inkra_markdown::html::to_html(&src, &o);
    }

    #[test]
    fn random_bytes_never_panic(bytes in prop::collection::vec(any::<u8>(), 0..2000)) {
        let s = String::from_utf8_lossy(&bytes).into_owned();
        let _ = parse(s.as_str(), &ParseOptions::default());
        let _ = parse(s.as_str(), &ParseOptions::commonmark());
    }
}
