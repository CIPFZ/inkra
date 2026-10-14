//! HTML pasted or imported converts without panicking, and the Markdown it
//! produces parses (ED-016, EX-020).
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(html) = std::str::from_utf8(data) else {
        return;
    };
    let md = inkra_html::html_to_markdown(html);
    let _ = inkra_markdown::parse(md.as_str(), &inkra_markdown::ParseOptions::default());
    let _ = inkra_html::render::flatten(html);
});
