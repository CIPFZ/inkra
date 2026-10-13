use super::format_ops::*;
use super::*;

fn tx(e: &mut Editor, t: Option<inkra_doc::Transaction>) {
    if let Some(t) = t {
        e.apply(t, UndoPolicy::NewGroup);
    }
}

#[test]
fn bold_toggles() {
    let mut e = Editor::from_text("hello world");
    e.sel = Selection::new(0, 5);
    let t = toggle_inline(&e.doc, e.sel, InlineFormat::Bold, "**", "*");
    tx(&mut e, Some(t));
    assert_eq!(e.text(), "**hello** world");
    assert_eq!(e.sel, Selection::new(2, 7));
    let t = toggle_inline(&e.doc, e.sel, InlineFormat::Bold, "**", "*");
    tx(&mut e, Some(t));
    assert_eq!(e.text(), "hello world");
    // no selection: empty pair, caret inside
    let mut e = Editor::from_text("x ");
    e.sel = Selection::caret(2);
    let t = toggle_inline(&e.doc, e.sel, InlineFormat::Bold, "**", "*");
    tx(&mut e, Some(t));
    assert_eq!((e.text().as_str(), e.sel.head), ("x ****", 4));
    // caret inside bold: remove markers
    let mut e = Editor::from_text("a **bc** d");
    e.sel = Selection::caret(5);
    let t = toggle_inline(&e.doc, e.sel, InlineFormat::Bold, "**", "*");
    tx(&mut e, Some(t));
    assert_eq!((e.text().as_str(), e.sel.head), ("a bc d", 3));
    // code with backticks inside uses double backticks
    let mut e = Editor::from_text("a`b");
    e.sel = Selection::new(0, 3);
    let t = toggle_inline(&e.doc, e.sel, InlineFormat::Code, "**", "*");
    tx(&mut e, Some(t));
    assert_eq!(e.text(), "`` a`b ``");
}

#[test]
fn headings_and_paragraph() {
    let mut e = Editor::from_text("Title\n\npara");
    e.sel = Selection::caret(2);
    {
        let t = set_heading(&e.doc, e.sel, 2);
        tx(&mut e, t);
    }
    assert_eq!(e.text(), "## Title\n\npara");
    {
        let t = set_heading(&e.doc, e.sel, 1);
        tx(&mut e, t);
    }
    assert_eq!(e.text(), "# Title\n\npara");
    {
        let t = set_heading(&e.doc, e.sel, 1);
        tx(&mut e, t);
    }
    assert_eq!(e.text(), "Title\n\npara", "same level toggles back");
    let mut e = Editor::from_text("Setext\n===");
    e.sel = Selection::caret(1);
    {
        let t = set_heading(&e.doc, e.sel, 2);
        tx(&mut e, t);
    }
    assert_eq!(e.text(), "## Setext");
    let mut e = Editor::from_text("## H");
    e.sel = Selection::caret(3);
    {
        let t = shift_heading(&e.doc, e.sel, true);
        tx(&mut e, t);
    }
    assert_eq!(e.text(), "# H");
    assert!(shift_heading(&e.doc, e.sel, true).is_none());
}

#[test]
fn quotes_and_lists() {
    let mut e = Editor::from_text("a\nb");
    e.sel = Selection::caret(0);
    {
        let t = toggle_quote(&e.doc, e.sel);
        tx(&mut e, t);
    }
    assert_eq!(e.text(), "> a\n> b");
    {
        let t = toggle_quote(&e.doc, e.sel);
        tx(&mut e, t);
    }
    assert_eq!(e.text(), "a\nb");
    let mut e = Editor::from_text("one\n\ntwo");
    e.sel = Selection::new(0, 8);
    {
        let t = toggle_list(&e.doc, e.sel, ListStyle::Bullet, '-');
        tx(&mut e, t);
    }
    assert_eq!(e.text(), "- one\n- two");
    e.sel = Selection::caret(3);
    {
        let t = toggle_list(&e.doc, e.sel, ListStyle::Ordered, '-');
        tx(&mut e, t);
    }
    assert_eq!(e.text(), "1. one\n- two");
    let mut e = Editor::from_text("- a");
    e.sel = Selection::caret(3);
    {
        let t = toggle_list(&e.doc, e.sel, ListStyle::Bullet, '-');
        tx(&mut e, t);
    }
    assert_eq!(e.text(), "a");
    let mut e = Editor::from_text("- [ ] t");
    e.sel = Selection::caret(7);
    {
        let t = ops::toggle_task(&e.doc, e.sel, None);
        tx(&mut e, t);
    }
    assert_eq!(e.text(), "- [x] t");
}

#[test]
fn code_fences_rule_table_paragraphs() {
    let mut e = Editor::from_text("let x");
    e.sel = Selection::caret(2);
    {
        let t = code_fences(&e.doc, e.sel, "rust");
        tx(&mut e, t);
    }
    assert_eq!(e.text(), "```rust\nlet x\n```");
    let mut e = Editor::from_text("para");
    e.sel = Selection::caret(4);
    let t = insert_rule(&e.doc, e.sel);
    tx(&mut e, Some(t));
    assert_eq!(e.text(), "para\n\n---\n\n");
    assert_eq!(
        table_source(1, 2, "\n"),
        "| Column 1 | Column 2 |\n| --- | --- |\n|   |   |"
    );
    let mut e = Editor::from_text("```\nx\n```");
    e.sel = Selection::caret(5);
    let t = exit_block(&e.doc, e.sel);
    tx(&mut e, Some(t));
    assert_eq!((e.text().as_str(), e.sel.head), ("```\nx\n```\n\n", 11));
    let mut e = Editor::from_text("a");
    e.sel = Selection::caret(1);
    let t = ensure_trailing(&e.doc, e.sel);
    tx(&mut e, Some(t));
    assert_eq!(e.text(), "a\n\n");
}

#[test]
fn remove_link_keeps_text() {
    let mut e = Editor::from_text("see [docs](https://x.io) now");
    e.sel = Selection::caret(6);
    let t = remove_link(&e.doc, e.sel);
    tx(&mut e, t);
    assert_eq!(e.text(), "see docs now");
}

#[test]
fn alerts_and_footnotes() {
    let mut e = Editor::from_text("a\n\nline one\nline two\n\nb");
    e.sel = Selection::caret(5);
    let t = insert_alert(&e.doc, e.sel, "TIP");
    tx(&mut e, Some(t));
    assert_eq!(e.text(), "a\n\n> [!TIP]\n> line one\n> line two\n\nb");
    let blocks = e.doc.ast().to_blocks();
    assert!(matches!(
        blocks[1].kind,
        inkra_markdown::BlockKind::BlockQuote { alert: Some(_) }
    ));
    let mut e = Editor::from_text("See this[^1].\n\n[^1]: one");
    e.sel = Selection::caret(3);
    let t = insert_footnote(&e.doc, e.sel);
    tx(&mut e, Some(t));
    assert_eq!(e.text(), "See[^2] this[^1].\n\n[^1]: one\n\n[^2]: ");
    assert_eq!(e.sel.head, e.text().len());
}

#[test]
fn clear_scope_and_move() {
    let mut e = Editor::from_text("a **bold** and `code` and [l](u) z");
    e.sel = Selection::new(0, e.text().len());
    let t = clear_format(&e.doc, e.sel);
    tx(&mut e, t);
    assert_eq!(e.text(), "a bold and code and l z");
    let e = Editor::from_text("x **a *b* c** y");
    let s1 = style_scope(&e.doc, Selection::caret(7)).unwrap();
    assert_eq!(&e.text()[s1.range()], "*b*");
    let s2 = style_scope(&e.doc, s1).unwrap();
    assert_eq!(&e.text()[s2.range()], "**a *b* c**");
    let mut e = Editor::from_text("one\n\ntwo\n\nthree");
    e.sel = Selection::caret(6);
    let t = move_block(&e.doc, e.sel, true);
    tx(&mut e, t);
    assert_eq!(e.text(), "two\n\none\n\nthree");
    assert_eq!(e.sel.head, 1);
    let mut e = Editor::from_text("- a\n- b\n- c");
    e.sel = Selection::caret(2);
    let t = move_block(&e.doc, e.sel, false);
    tx(&mut e, t);
    assert_eq!(e.text(), "- b\n- a\n- c");
    let mut e = Editor::from_text("- a\n- b\n- c");
    e.sel = Selection::caret(10);
    let t = move_block(&e.doc, e.sel, true);
    tx(&mut e, t);
    assert_eq!(e.text(), "- a\n- c\n- b");
}

#[test]
fn drag_moves_and_copies_text() {
    use serde_json::json;
    let mut e = crate::Engine::new(crate::Preferences::default());
    e.state.editor = Editor::from_text("one two three");
    e.execute("edit.moveText", &json!({ "from": 0, "to": 4, "at": 13 }))
        .unwrap();
    assert_eq!(e.state.editor.text(), "two threeone ");
    assert_eq!(&e.state.editor.text()[e.state.editor.sel.range()], "one ");
    e.execute(
        "edit.moveText",
        &json!({ "from": 0, "to": 3, "at": 13, "copy": true }),
    )
    .unwrap();
    assert_eq!(e.state.editor.text(), "two threeone two");
    // Dropping onto itself does nothing.
    e.execute("edit.moveText", &json!({ "from": 0, "to": 3, "at": 2 }))
        .unwrap();
    assert_eq!(e.state.editor.text(), "two threeone two");
    e.state.editor = Editor::from_text("x");
    e.execute(
        "edit.insertFileLink",
        &json!({ "path": "/tmp/pic.png", "at": 1 }),
    )
    .unwrap();
    assert_eq!(e.state.editor.text(), "x![pic](/tmp/pic.png)");
}

#[test]
fn link_dialog_edits_and_inserts() {
    use serde_json::json;
    let mut e = crate::Engine::new(crate::Preferences::default());
    e.state.editor = Editor::from_text("see [old](http://a) here");
    e.state.editor.sel = Selection::caret(6);
    e.execute("format.editLink", &json!({})).unwrap();
    assert!(
        matches!(e.take_effects().as_slice(), [crate::Effect::LinkDialog { text, url }] if text == "old" && url == "http://a")
    );
    e.execute(
        "format.editLink",
        &json!({ "text": "new", "url": "https://b" }),
    )
    .unwrap();
    assert_eq!(e.state.editor.text(), "see [new](https://b) here");
    e.state.editor.sel = Selection::new(21, 25);
    e.execute("format.editLink", &json!({ "text": "here", "url": "x y" }))
        .unwrap();
    assert_eq!(e.state.editor.text(), "see [new](https://b) [here](<x y>)");
}

#[test]
fn ctrl_enter_before_a_leading_block_and_toc_is_atomic() {
    let mut e = Editor::from_text("```\ncode\n```\n");
    e.sel = Selection::caret(4);
    let t = exit_block(&e.doc, e.sel);
    tx(&mut e, Some(t));
    assert_eq!(e.text(), "\n\n```\ncode\n```\n");
    assert_eq!(e.sel.head, 0);
    let mut x = crate::Engine::new(crate::Preferences::default());
    x.state.editor = Editor::from_text("| a | b |\n| - | - |\n| 1 | 2 |\n");
    x.state.editor.sel = Selection::caret(2);
    x.execute("table.insertRowBelow", &serde_json::json!({}))
        .unwrap();
    assert!(
        x.state.editor.text().starts_with("\n\n| a |"),
        "{}",
        x.state.editor.text()
    );
    // Backspace after a TOC removes it whole.
    let mut e = Editor::from_text("[TOC]\n\npara");
    e.sel = Selection::caret(7);
    let t = crate::editor::live_ops::delete_backward(
        &e.doc,
        e.sel,
        crate::editor::Unit::Char,
        &Default::default(),
    );
    tx(&mut e, t);
    assert_eq!(e.text(), "para");
}

#[test]
fn select_all_in_a_table_cell() {
    let e = Editor::from_text("x\n\n| ab | cd |\n| -- | -- |\n| ef | gh |\n");
    let at = e.text().find("ef").unwrap() + 1;
    let s1 = crate::editor::ops::select_all(&e.doc, Selection::caret(at));
    assert_eq!(&e.text()[s1.range()], "ef");
    let s2 = crate::editor::ops::select_all(&e.doc, s1);
    assert!(e.text()[s2.range()].starts_with("| ab"));
    let s3 = crate::editor::ops::select_all(&e.doc, s2);
    assert_eq!(s3.range(), 0..e.text().len());
}

#[test]
fn hard_breaks_paragraph_tab_and_multi_item_tab() {
    let mut e = Editor::from_text("line");
    e.mode = crate::editor::ViewMode::Live;
    e.sel = Selection::caret(4);
    e.prefs.hard_break = 2;
    e.newline_live(crate::editor::Break::Soft);
    assert_eq!(e.text(), "line<br>\n");
    let mut e = Editor::from_text("para");
    e.sel = Selection::caret(0);
    e.prefs.paragraph_tab = 1;
    e.tab(false);
    assert_eq!(e.text(), "\u{3000}\u{3000}para");
    e.prefs.paragraph_tab = 2;
    e.tab(false);
    assert_eq!(e.text(), "\u{3000}\u{3000}para");
    let mut e = Editor::from_text("- a\n- b\n- c\n- d");
    e.sel = Selection::new(6, 14);
    e.tab(false);
    assert_eq!(e.text(), "- a\n  - b\n  - c\n  - d");
    e.undo();
    assert_eq!(e.text(), "- a\n- b\n- c\n- d", "one undo step");
}

#[test]
fn pasted_image_files_follow_the_image_preferences() {
    use serde_json::json;
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("pic.png"), [137, 80, 78, 71]).unwrap();
    std::fs::write(d.path().join("notes.txt"), "x").unwrap();
    let mut e = crate::Engine::new(crate::Preferences::default());
    e.state.editor = Editor::from_text("");
    e.state.editor.path = Some(d.path().join("doc.md"));
    e.execute(
        "edit.paste",
        &json!({ "files": [d.path().join("pic.png"), d.path().join("notes.txt")] }),
    )
    .unwrap();
    assert_eq!(
        e.state.editor.text(),
        "![pic](pic.png)\n[notes.txt](notes.txt)"
    );
}

#[test]
fn list_marker_styles() {
    let mut e = Editor::from_text("- a\n- b\n- c");
    e.sel = Selection::caret(6);
    let t = set_list_marker(&e.doc, e.sel, "1)");
    tx(&mut e, t);
    assert_eq!(e.text(), "1) a\n2) b\n3) c");
    let t = set_list_marker(&e.doc, Selection::caret(1), "*");
    tx(&mut e, t);
    assert_eq!(e.text(), "* a\n* b\n* c");
}

#[test]
fn code_language_and_math_source() {
    use serde_json::json;
    let mut e = crate::Engine::new(crate::Preferences::default());
    e.state.editor = Editor::from_text("```py\nx\n```\n\n$$\na+b\n$$\n");
    e.state.editor.sel = Selection::caret(6);
    e.execute("code.setLanguage", &json!({ "lang": "rust" }))
        .unwrap();
    assert!(e.state.editor.text().starts_with("```rust\nx"));
    e.take_effects();
    let at = e.state.editor.text().find("a+b").unwrap();
    e.state.editor.sel = Selection::caret(at + 1);
    e.execute("math.copyTex", &json!({})).unwrap();
    assert!(
        matches!(e.take_effects().as_slice(), [crate::Effect::SetClipboard { text, .. }] if text == "a+b")
    );
    let before = e.state.prefs.bool("markdown.codeWrap");
    e.execute("view.toggleCodeWrap", &json!({})).unwrap();
    assert_ne!(e.state.prefs.bool("markdown.codeWrap"), before);
}
