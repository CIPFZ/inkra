//! Inputs found by `cargo fuzz` (fuzz/): each lays out quickly and without
//! panicking. A file's first byte selects display style (bit 0), the rest is TeX.

use std::time::{Duration, Instant};

fn render(tex: &str, display: bool) -> Duration {
    let t = Instant::now();
    let _ = inkra_math::render_uncached(&inkra_math::Request::new(tex, display, 16.0));
    t.elapsed()
}

#[test]
fn fuzz_regressions_lay_out_quickly() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fuzz-regressions");
    for e in std::fs::read_dir(&dir).unwrap() {
        let bytes = std::fs::read(e.unwrap().path()).unwrap();
        let (flags, tex) = bytes.split_first().unwrap();
        let tex = String::from_utf8_lossy(tex);
        // Doubled, it took 46 s when nested `\left…\middle` groups were laid out
        // twice at every level.
        let twice = format!("{tex}{tex}");
        assert!(render(&twice, flags & 1 == 1) < Duration::from_secs(5));
    }
}

#[test]
fn deeply_nested_middle_groups_are_linear() {
    let open = r"\left( a \middle| ".repeat(30);
    assert!(render(&format!("{open}x"), true) < Duration::from_secs(5));
    let closed = format!("{open}x{}", r"\right)".repeat(30));
    assert!(render(&closed, false) < Duration::from_secs(5));
}
