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

fn live(s: &str) -> Editor {
    let mut e = at_end(s);
    e.mode = ViewMode::Live;
    e
}

#[test]
fn live_enter_semantics() {
    // paragraph → new paragraph (blank line)
    let mut e = live("Hello");
    e.newline_live(Break::Enter);
    assert_eq!(e.text(), "Hello\n\n");
    // heading split: the remainder becomes a paragraph
    let mut e = live("# Title here");
    e.sel = Selection::caret(7);
    e.newline_live(Break::Enter);
    assert_eq!(e.text(), "# Title\n\n here");
    // quote: new paragraph inside, empty one leaves the quote
    let mut e = live("> q");
    e.newline_live(Break::Enter);
    assert_eq!(e.text(), "> q\n>\n> ");
    e.newline_live(Break::Enter);
    assert_eq!(e.text(), "> q\n\n");
    // list continuation still applies
    let mut e = live("- a");
    e.newline_live(Break::Enter);
    assert_eq!(e.text(), "- a\n- ");
    // soft break
    let mut e = live("a");
    e.newline_live(Break::Soft);
    assert_eq!(e.text(), "a\n");
}

#[test]
fn live_enter_expands_syntax() {
    let mut e = live("```rust");
    e.newline_live(Break::Enter);
    assert_eq!((e.text().as_str(), e.sel.head), ("```rust\n\n```", 8));
    let mut e = live("$$");
    e.newline_live(Break::Enter);
    assert_eq!(e.text(), "$$\n\n$$");
    let mut e = live("|a|b|");
    e.newline_live(Break::Enter);
    assert_eq!(e.text(), "|a|b|\n| --- | --- |\n|   |   |");
    let blocks = e.doc.ast().to_blocks();
    assert!(matches!(
        blocks[0].kind,
        inkra_markdown::BlockKind::Table { .. }
    ));
    // a closed fence is not extended
    let mut e = ed("```\nx\n```", 3);
    e.mode = ViewMode::Live;
    e.newline_live(Break::Enter);
    assert_eq!(e.text(), "```\n\nx\n```");
}

#[test]
fn live_backspace_semantics() {
    // heading → paragraph
    let mut e = live("# Title");
    e.sel = Selection::caret(2);
    e.delete_backward_live(Unit::Char);
    assert_eq!(e.text(), "Title");
    // paragraph merges with the previous paragraph
    let mut e = live("a\n\nb");
    e.sel = Selection::caret(3);
    e.delete_backward_live(Unit::Char);
    assert_eq!(e.text(), "ab");
    // list item → paragraph after the list
    let mut e = live("- a\n- b");
    e.sel = Selection::caret(6);
    e.delete_backward_live(Unit::Char);
    assert_eq!(e.text(), "- a\n\nb");
    // nested item outdents
    let mut e = live("- a\n  - b");
    e.sel = Selection::caret(8);
    e.delete_backward_live(Unit::Char);
    assert_eq!(e.text(), "- a\n- b");
    // quote paragraph unquotes
    let mut e = live("> q");
    e.sel = Selection::caret(2);
    e.delete_backward_live(Unit::Char);
    assert_eq!(e.text(), "q");
    // before a code block: caret moves into it, no merge
    let mut e = live("```\ncode\n```\n\npara");
    e.sel = Selection::caret(14);
    e.delete_backward_live(Unit::Char);
    assert_eq!(e.text(), "```\ncode\n```\n\npara");
    assert_eq!(e.sel.head, 8);
    // empty code block is deleted
    let mut e = live("```\n```");
    e.sel = Selection::caret(4);
    e.delete_backward_live(Unit::Char);
    assert_eq!(e.text(), "");
}

#[test]
fn live_delete_forward_merges() {
    let mut e = live("a\n\nb");
    e.sel = Selection::caret(1);
    e.delete_forward_live(Unit::Char);
    assert_eq!(e.text(), "ab");
    let mut e = live("a\n\n# H");
    e.sel = Selection::caret(1);
    e.delete_forward_live(Unit::Char);
    assert_eq!(e.text(), "aH");
}

#[test]
fn typing_after_block_on_blank_line_starts_new_paragraph() {
    let mut e = live("beta\n");
    typed(&mut e, "x");
    assert_eq!(e.text(), "beta\n\nx");
    let blocks = e.doc.ast().to_blocks();
    assert_eq!(blocks.len(), 2);
    let mut e = live("- a\n");
    typed(&mut e, "y");
    assert_eq!(e.text(), "- a\n\ny");
    // after a blank line: nothing extra
    let mut e = live("p\n\n");
    typed(&mut e, "z");
    assert_eq!(e.text(), "p\n\nz");
    // source mode is literal
    let mut e = at_end("beta\n");
    e.mode = ViewMode::Source;
    typed(&mut e, "x");
    assert_eq!(e.text(), "beta\nx");
}

#[test]
fn typing_a_fence_does_not_double_backticks() {
    let mut e = Editor::default();
    typed(&mut e, "```rust");
    assert_eq!(e.text(), "```rust");
    let mut e = Editor::default();
    typed(&mut e, "`x");
    assert_eq!(e.text(), "`x`", "inline code still pairs");
}

#[test]
fn live_enter_on_empty_nested_item_outdents() {
    let mut e = live("- a\n  - b\n  - ");
    e.newline_live(Break::Enter);
    assert_eq!(e.text(), "- a\n  - b\n- ");
    e.newline_live(Break::Enter);
    assert_eq!(e.text(), "- a\n  - b\n\n");
}

#[test]
fn table_editing() {
    let src = "| a | b |\n| --- | --- |\n| 1 | 2 |";
    let mut e = live(src);
    e.sel = Selection::caret(2); // in "a"
    e.tab(false);
    assert_eq!(&e.text()[e.sel.range()], "b", "Tab selects the next cell");
    e.tab(false);
    assert_eq!(&e.text()[e.sel.range()], "1");
    e.tab(false);
    e.tab(false);
    // last cell: Tab adds a row
    assert_eq!(
        e.text(),
        "| a   | b   |\n| --- | --- |\n| 1   | 2   |\n|     |     |"
    );
    typed(&mut e, "x|y");
    assert!(e.text().contains("x\\|y"), "pipe escaped: {}", e.text());
    // Enter never inserts a line break inside a table
    let before = e.text();
    e.newline_live(Break::Enter);
    assert!(
        e.text().lines().count() > before.lines().count(),
        "Enter on the last row adds a row"
    );
    // Backspace at a cell start does not eat the pipe
    let t = e.text();
    let at = t.find("| 1").unwrap() + 2;
    e.sel = Selection::caret(at);
    e.delete_backward_live(Unit::Char);
    assert_eq!(e.text(), t);
    // Shift+Enter inserts <br>
    e.newline_live(Break::Soft);
    assert!(e.text().contains("<br>"));
}

#[test]
fn table_tab_walks_cells_in_order() {
    let cols = 12;
    let hdr = format!(
        "| {} |",
        (0..cols)
            .map(|i| format!("h{i}"))
            .collect::<Vec<_>>()
            .join(" | ")
    );
    let dl = format!("|{}|", vec!["---"; cols].join("|"));
    let rows: Vec<String> = (0..3)
        .map(|r| {
            format!(
                "| {} |",
                (0..cols)
                    .map(|c| format!("v{r}-{c}"))
                    .collect::<Vec<_>>()
                    .join(" | ")
            )
        })
        .collect();
    let src = format!("{hdr}\n{dl}\n{}", rows.join("\n"));
    let mut e = live(&src);
    let at = src.find("v0-11").unwrap() + 5;
    e.sel = Selection::caret(at);
    for _ in 0..11 {
        e.tab(false);
    }
    assert_eq!(&e.text()[e.sel.range()], "v1-10");
}
