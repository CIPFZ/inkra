//! TS-004: one PNG snapshot per Markdown element in the live view, plus a theme
//! switch. Only bundled fonts are used (no CJK), so the images match across
//! platforms within the comparison threshold.
//!
//! Update with `UPDATE_SNAPSHOTS=1 cargo test -p inkra-ui --test snapshots`.

use egui_kittest::{Harness, SnapshotOptions, SnapshotResults};
use inkra_engine::{Engine, Preferences};
use inkra_i18n::I18n;
use inkra_ui::Shell;

const ELEMENTS: &[(&str, &str)] = &[
    (
        "headings",
        "# Heading 1\n\n## Heading 2\n\n### Heading 3\n\n#### Heading 4\n",
    ),
    (
        "inline",
        "Text with *emphasis*, **strong**, ~~strike~~, `code`, a [link](https://example.com) and :tada:.\n",
    ),
    (
        "lists",
        "- one\n- two\n  - nested\n\n1. first\n2. second\n\n- [ ] open\n- [x] done\n",
    ),
    ("quote", "> A quote\n> over two lines\n>\n> > nested\n"),
    ("alert", "> [!WARNING]\n> Be careful.\n"),
    (
        "code",
        "```rust\nfn main() {\n    println!(\"hi\");\n}\n```\n",
    ),
    (
        "table",
        "| Name | Count | Note |\n|:--|--:|:-:|\n| a | 1 | x |\n| b | 22 | y |\n",
    ),
    ("math", "$$\n\\int_0^1 x^2\\,dx = \\frac{1}{3}\n$$\n"),
    ("footnote", "A claim[^1].\n\n[^1]: The source.\n"),
    ("rule", "Above\n\n---\n\nBelow\n"),
    ("toc", "[TOC]\n\n# One\n\n## Two\n"),
    (
        "front-matter",
        "---\ntitle: Notes\ntags: [a, b]\n---\n\nBody\n",
    ),
    (
        "html",
        "<details open><summary>More</summary><p>Inside <b>HTML</b></p></details>\n",
    ),
    (
        "diagram",
        "```mermaid\nflowchart LR\n  A[Start] --> B{Choice}\n  B -->|yes| C[Done]\n```\n",
    ),
];

fn harness(doc: &str, theme: &str) -> Harness<'static, Shell> {
    let mut prefs = Preferences::default();
    prefs.set("editor.cursorBlink", serde_json::json!(false));
    prefs.set("editor.smoothScroll", serde_json::json!(false));
    prefs.set("general.checkUpdates", serde_json::json!(false));
    prefs.set("appearance.theme", serde_json::json!(theme));
    // The same chrome on every platform (Windows defaults to its own title bar).
    prefs.set("appearance.titleBar", serde_json::json!("native"));
    let mut engine = Engine::new(prefs);
    // A trailing blank paragraph keeps the caret away from the element.
    engine.state.editor = inkra_engine::editor::Editor::from_text(&format!("{doc}\n\n"));
    let len = engine.state.editor.doc.text().len();
    engine.state.editor.sel = inkra_doc::Selection::caret(len);
    engine.state.view.menu_bar = false;
    engine.state.view.status_bar = false;
    inkra_highlight::load_now();
    let shell = Shell::new(engine, I18n::new("en-US"), None);
    let mut h = Harness::builder()
        .with_size(egui::vec2(720.0, 360.0))
        .wgpu()
        .build_ui_state(|ui, shell: &mut Shell| shell.ui(ui), shell);
    // Fonts, syntax definitions and formula textures arrive over a few frames.
    for _ in 0..8 {
        h.step();
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    h
}

fn options() -> SnapshotOptions {
    // GPU backends blend glyph edges slightly differently.
    SnapshotOptions::new().threshold(1.5).max_failed_pixels(40)
}

/// One test: wgpu devices created on parallel test threads can crash software
/// renderers.
#[test]
fn elements_and_themes_match_their_snapshots() {
    let mut results = SnapshotResults::new();
    for (name, doc) in ELEMENTS {
        let mut h = harness(doc, "Paper");
        h.snapshot_options(*name, &options());
        results.extend_harness(&mut h);
    }
    let doc = "# Ink\n\nA paragraph with **strong** text and `code`.\n\n> A quote\n";
    for theme in ["Paper", "Ink", "Journal"] {
        let mut h = harness(doc, theme);
        h.snapshot_options(format!("theme-{}", theme.to_lowercase()), &options());
        results.extend_harness(&mut h);
    }
    results.unwrap();
}
