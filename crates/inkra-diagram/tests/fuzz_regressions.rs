//! Inputs found by `cargo fuzz` (fuzz/): each must lay out or fail cleanly, at
//! a sane size.

#[test]
fn fuzz_regressions_do_not_panic() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fuzz-regressions");
    let mut n = 0;
    for e in std::fs::read_dir(&dir).unwrap() {
        let bytes = std::fs::read(e.unwrap().path()).unwrap();
        let src = String::from_utf8_lossy(&bytes);
        if let Ok(sc) = inkra_diagram::render_uncached(&src, &inkra_diagram::Palette::light()) {
            // Nested subgraphs once pushed each other apart to 1 077 120 px.
            assert!(
                sc.width < 100_000.0 && sc.height < 100_000.0,
                "{}x{}",
                sc.width,
                sc.height
            );
        }
        n += 1;
    }
    assert!(n > 0);
}
