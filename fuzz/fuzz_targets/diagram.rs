//! Any Mermaid source lays out or reports an error, never panics (MD-100).
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(src) = std::str::from_utf8(data) else {
        return;
    };
    let _ = inkra_diagram::kind(src);
    let _ = inkra_diagram::render_uncached(src, &inkra_diagram::Palette::light());
});
