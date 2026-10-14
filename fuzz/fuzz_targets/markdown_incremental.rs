//! An edit re-parsed incrementally equals a full parse of the edited text
//! (AD-1, TS-003). Input: the document, then a 0xFF byte, then a 4-byte edit
//! header (start, length as u16) and the inserted text.
#![no_main]

use inkra_markdown::{Document, ParseOptions, parse};
use libfuzzer_sys::fuzz_target;

fn boundary(s: &str, mut i: usize) -> usize {
    i = i.min(s.len());
    while !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fuzz_target!(|data: &[u8]| {
    let split = data.iter().position(|&b| b == 0xFF).unwrap_or(data.len());
    let Ok(before) = std::str::from_utf8(&data[..split]) else {
        return;
    };
    let rest = data.get(split + 1..).unwrap_or(&[]);
    if rest.len() < 4 {
        return;
    }
    let Ok(insert) = std::str::from_utf8(&rest[4..]) else {
        return;
    };
    let start = boundary(before, u16::from_le_bytes([rest[0], rest[1]]) as usize);
    let len = u16::from_le_bytes([rest[2], rest[3]]) as usize;
    let end = boundary(before, start + len % (before.len() - start + 1));
    for o in [ParseOptions::default(), ParseOptions::commonmark()] {
        let mut doc = Document::parse(before, o.clone());
        let mut after = before.to_owned();
        after.replace_range(start..end, insert);
        doc.edit(after.as_str(), start..end, insert.len());
        let full = parse(after.as_str(), &o);
        assert_eq!(doc.to_blocks(), full.blocks, "incremental != full");
        assert_eq!(doc.context(), &full.cx);
    }
});
