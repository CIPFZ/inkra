//! `\[ … \]` blocks and `\( … \)` inline math (MD-008, MD-035, MD-069).

use inkra_markdown::{BlockKind, InlineKind, ParseOptions, parse};

fn opts(on: bool) -> ParseOptions {
    ParseOptions {
        latex_delimiters: on,
        ..ParseOptions::default()
    }
}

#[test]
fn bracket_blocks_and_paren_inlines() {
    let src = "\\[\nx^2\n\\]\n\nInline \\(a+b\\) here.\n\n\\[ y \\]\n";
    let p = parse(src, &opts(true));
    let kinds: Vec<&str> = p.blocks.iter().map(|b| b.kind.name()).collect();
    assert_eq!(kinds, ["math_block", "paragraph", "math_block"]);
    let BlockKind::MathBlock { close, .. } = &p.blocks[0].kind else {
        unreachable!()
    };
    assert_eq!(&src[close.clone().unwrap()], "\\]");
    assert_eq!(&src[p.blocks[0].lines[0].span.clone()], "x^2");
    let mut math = Vec::new();
    for n in &p.blocks[1].inlines {
        n.walk(&mut |n| {
            if let InlineKind::Math { content } = &n.kind {
                math.push(src[content.clone()].to_owned());
            }
        });
    }
    assert_eq!(math, ["a+b"]);
    assert_eq!(src[p.blocks[2].lines[0].span.clone()].trim(), "y");
    // Off: plain escapes.
    let p = parse(src, &opts(false));
    assert!(p.blocks.iter().all(|b| b.kind.name() == "paragraph"));
}
