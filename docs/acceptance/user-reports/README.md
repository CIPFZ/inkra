# User report files

Minimal Markdown files from user reports (T6), one per report, named
`<issue number>-<short-name>.md`. `cargo test -p inkra-doc --test corpus`
checks every file here: byte-identical round trip, incremental parsing equal
to a full parse after edits, undo back to the original bytes, in both the GFM
and the CommonMark dialect. Rendering problems additionally get a test in the
crate that draws them (layout, math, diagram) and are listed in
`../user-reports.md`.

This README is itself a test file.
