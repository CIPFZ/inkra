use super::*;

fn ed(s: &str, caret: usize) -> Editor {
    let mut e = Editor::from_text(s);
    e.sel = Selection::caret(caret);
    e
}

fn at_end(s: &str) -> Editor {
    ed(s, s.len())
}

fn typed(e: &mut Editor, s: &str) {
    for c in s.chars() {
        e.type_text(&c.to_string());
    }
}

#[test]
fn typing_and_undo_groups() {
    let mut e = Editor::default();
    typed(&mut e, "# Hello world");
    assert_eq!(e.text(), "# Hello world");
    e.undo();
    assert_eq!(e.text(), "# Hello ");
    e.undo();
    assert_eq!(e.text(), "# ");
}

#[test]
fn auto_pair_brackets_and_quotes() {
    let mut e = Editor::default();
    typed(&mut e, "(");
    assert_eq!((e.text().as_str(), e.sel.head), ("()", 1));
    typed(&mut e, "x)");
    assert_eq!((e.text().as_str(), e.sel.head), ("(x)", 3));
    let mut e = at_end("a ");
    typed(&mut e, "[");
    e.delete_backward(Unit::Char);
    assert_eq!(e.text(), "a ");
    let mut e = at_end("don");
    typed(&mut e, "'t");
    assert_eq!(e.text(), "don't");
    let mut e = ed("word", 0);
    e.sel = Selection::new(0, 4);
    typed(&mut e, "\"");
    assert_eq!(
        (e.text().as_str(), e.sel),
        ("\"word\"", Selection::new(1, 5))
    );
    // markdown symbols are not paired by default
    let mut e = Editor::default();
    typed(&mut e, "*");
    assert_eq!(e.text(), "*");
    e.prefs.pair_markdown = true;
    let mut e2 = Editor {
        prefs: EditPrefs {
            pair_markdown: true,
            ..Default::default()
        },
        ..Editor::default()
    };
    typed(&mut e2, "*");
    assert_eq!(e2.text(), "**");
    // no pairing before a word character
    let mut e = ed("abc", 0);
    typed(&mut e, "(");
    assert_eq!(e.text(), "(abc");
}

#[test]
fn no_markdown_pairing_in_code() {
    let mut e = Editor {
        prefs: EditPrefs {
            pair_markdown: true,
            ..Default::default()
        },
        ..at_end("```\nx")
    };
    typed(&mut e, "*");
    assert_eq!(e.text(), "```\nx*");
}

#[test]
fn list_continuation_and_exit() {
    let mut e = at_end("- a");
    e.newline(Break::Enter);
    assert_eq!(e.text(), "- a\n- ");
    e.newline(Break::Enter);
    assert_eq!(e.text(), "- a\n\n");
    let mut e = at_end("1. a");
    e.newline(Break::Enter);
    assert_eq!(e.text(), "1. a\n2. ");
    let mut e = at_end("- [x] done");
    e.newline(Break::Enter);
    assert_eq!(e.text(), "- [x] done\n- [ ] ");
    let mut e = at_end("> - q");
    e.newline(Break::Enter);
    assert_eq!(e.text(), "> - q\n> - ");
    // loose lists stay loose
    let mut e = at_end("- a\n\n- b");
    e.newline(Break::Enter);
    assert_eq!(e.text(), "- a\n\n- b\n\n- ");
}

#[test]
fn m3_acceptance_case_3_nested_list_then_exit() {
    // `- a` Enter `b` Tab `c` Enter Enter Enter
    let mut e = Editor::default();
    typed(&mut e, "- a");
    e.newline(Break::Enter);
    typed(&mut e, "b");
    e.tab(false);
    assert_eq!(e.text(), "- a\n  - b");
    typed(&mut e, "c");
    e.newline(Break::Enter);
    assert_eq!(e.text(), "- a\n  - bc\n  - ");
    e.newline(Break::Enter);
    assert_eq!(e.text(), "- a\n  - bc\n- ");
    e.newline(Break::Enter);
    assert_eq!(e.text(), "- a\n  - bc\n\n");
    typed(&mut e, "para");
    let blocks = e.doc.ast().to_blocks();
    assert_eq!(blocks.len(), 2, "{blocks:#?}");
    assert!(matches!(
        blocks[0].kind,
        inkra_markdown::BlockKind::List { .. }
    ));
    assert!(matches!(
        blocks[1].kind,
        inkra_markdown::BlockKind::Paragraph
    ));
}

#[test]
fn tab_nests_and_shift_tab_outdents() {
    let mut e = at_end("- a\n- b");
    e.tab(false);
    assert_eq!(e.text(), "- a\n  - b");
    e.tab(true);
    assert_eq!(e.text(), "- a\n- b");
    // first item cannot be nested
    let mut e = at_end("- a");
    e.tab(false);
    assert_eq!(e.text(), "- a");
    // ordered lists nest under the content column
    let mut e = at_end("1. a\n2. b");
    e.tab(false);
    assert_eq!(e.text(), "1. a\n   1. b");
    // plain paragraph: four spaces
    let mut e = at_end("x");
    e.tab(false);
    assert_eq!(e.text(), "x    ");
}

#[test]
fn ordered_lists_renumber() {
    let mut e = ed("1. a\n2. b\n3. c\n", 0);
    let l = ops::line_at(&e.doc, 6);
    e.sel = l;
    e.delete_backward(Unit::Char);
    assert_eq!(e.text(), "1. a\n2. c\n");
    // inserting in the middle renumbers the tail, in one undo group
    let mut e = ed("1. a\n2. b\n", 4);
    e.newline(Break::Enter);
    typed(&mut e, "x");
    assert_eq!(e.text(), "1. a\n2. x\n3. b\n");
    e.undo();
    e.undo();
    assert_eq!(e.text(), "1. a\n2. b\n");
}

#[test]
fn code_and_quote_continuation() {
    let mut e = at_end("```\n    let x = 1;");
    e.newline(Break::Enter);
    assert_eq!(e.text(), "```\n    let x = 1;\n    ");
    let mut e = at_end("> quoted");
    e.newline(Break::Enter);
    assert_eq!(e.text(), "> quoted\n> ");
    e.newline(Break::Enter);
    assert_eq!(e.text(), "> quoted\n");
}

#[test]
fn soft_break_and_crlf() {
    let mut e = at_end("text");
    e.newline(Break::Soft);
    assert_eq!(e.text(), "text\n");
    let mut e = at_end("a\r\nb");
    e.newline(Break::Enter);
    assert_eq!(e.text(), "a\r\nb\r\n");
    e.delete_backward(Unit::Char);
    assert_eq!(e.text(), "a\r\nb");
}

#[test]
fn word_and_line_deletion() {
    let mut e = at_end("hello brave world");
    e.delete_backward(Unit::Word);
    assert_eq!(e.text(), "hello brave ");
    e.delete_backward(Unit::Line);
    assert_eq!(e.text(), "");
    let mut e = ed("abc def", 0);
    e.delete_forward(Unit::Word);
    assert_eq!(e.text(), " def");
}

#[test]
fn selection_commands() {
    let e = ed("one two\n\n```\ncode\nmore\n```\n", 5);
    assert_eq!(ops::word_at(&e.doc, 5), Selection::new(4, 7));
    assert_eq!(ops::line_at(&e.doc, 5), Selection::new(0, 8));
    let all = ops::select_all(&e.doc, Selection::caret(15));
    assert_eq!(&e.text()[all.range()], "code\nmore");
    let all2 = ops::select_all(&e.doc, all);
    assert_eq!(all2, Selection::new(0, e.text().len()));
}

#[test]
fn paste_rules() {
    let mut e = at_end("x ");
    let c = ClipboardContent {
        text: Some("bold".into()),
        html: Some("<p><b>bold</b> <a href=\"u\">l</a></p>".into()),
        ..Default::default()
    };
    e.paste(&c, false);
    assert_eq!(e.text(), "x **bold** [l](u)");
    let mut e = at_end("x ");
    e.paste(&c, true);
    assert_eq!(e.text(), "x bold");
    let mut e = at_end("```\n");
    e.paste(
        &ClipboardContent {
            text: Some("# not a heading".into()),
            html: Some("<h1>h</h1>".into()),
            ..Default::default()
        },
        false,
    );
    assert_eq!(e.text(), "```\n# not a heading");
}
