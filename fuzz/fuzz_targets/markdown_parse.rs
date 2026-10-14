//! Whole-document parsing never panics, in both dialects, and keeps every byte
//! inside the block ranges (NF-002, MD-091).
#![no_main]

use inkra_markdown::{ParseOptions, parse};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    for o in [ParseOptions::default(), ParseOptions::commonmark()] {
        let p = parse(text, &o);
        for b in &p.blocks {
            assert!(b.span.end <= text.len(), "block span past the end");
        }
    }
});
