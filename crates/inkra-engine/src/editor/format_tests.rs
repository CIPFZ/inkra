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
