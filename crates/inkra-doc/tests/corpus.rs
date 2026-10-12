//! Real-world corpus round trip (TS-002): every `.md` file under `target/corpus`
//! (fetched by `cargo xtask corpus`) must decode and re-encode byte-identically,
//! parse without panicking, and stay equal to a full parse after edits.

use std::path::{Path, PathBuf};

use inkra_doc::{Doc, Transaction, UndoPolicy};
use inkra_markdown::ParseOptions;

fn files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.file_name().is_some_and(|n| n == ".git") {
            continue;
        }
        if p.is_dir() {
            files(&p, out);
        } else if p.extension().is_some_and(|x| x == "md" || x == "markdown") {
            out.push(p);
        }
    }
}

#[test]
fn corpus_round_trip_and_edits() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/corpus");
    let mut list = Vec::new();
    files(&dir, &mut list);
    list.sort();
    if list.is_empty() {
        eprintln!("corpus not present; run `cargo xtask corpus` (skipping)");
        return;
    }
    let o = ParseOptions::default();
    let mut n = 0;
    for path in &list {
        let bytes = std::fs::read(path).unwrap();
        let mut doc = Doc::from_bytes(&bytes, None, o.clone());
        assert_eq!(
            doc.to_bytes(),
            bytes,
            "round trip failed: {}",
            path.display()
        );
        let _ = inkra_markdown::html::to_html(&doc.text().to_string(), &o);
        // a few deterministic edits, checked against a full parse
        let len = doc.text().len();
        for k in 1..4 {
            let at = doc.text().snap(len * k / 4);
            let ins = ["\n", "*", "# "][k - 1];
            doc.apply(Transaction::single(at..at, ins), UndoPolicy::NewGroup);
        }
        let text = doc.text().to_string();
        let full = inkra_markdown::parse(text.as_str(), &o);
        assert_eq!(
            doc.ast().to_blocks(),
            full.blocks,
            "incremental mismatch: {}",
            path.display()
        );
        while doc.undo().is_some() {}
        assert_eq!(
            doc.to_bytes(),
            bytes,
            "undo did not restore: {}",
            path.display()
        );
        n += 1;
    }
    eprintln!("corpus: {n} files OK");
    assert!(n >= 500, "corpus has only {n} files");
}
