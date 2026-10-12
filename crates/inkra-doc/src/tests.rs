use std::time::Duration;

use proptest::prelude::*;

use super::*;

#[test]
fn round_trip_bytes() {
    for bytes in [
        &b"# Title\r\n\r\ntext  \r\n"[..],
        b"\xEF\xBB\xBFmixed\nendings\r\nhere\r",
        b"",
        "中文\n".as_bytes(),
    ] {
        let d = Doc::from_bytes(bytes, None, ParseOptions::default());
        assert_eq!(d.to_bytes(), bytes);
        assert!(!d.is_dirty());
    }
    let d = Doc::from_bytes(b"a\r\nb\r\n", None, ParseOptions::default());
    assert_eq!(d.newline(), "\r\n");
}

#[test]
fn edits_update_ast_and_undo_restores() {
    let mut d = Doc::from_str("# Hello\n\nworld\n", ParseOptions::default());
    assert_eq!(d.ast().len(), 2);
    let tx = Transaction::single(9..14, "**world**").with_selection_before(Selection::caret(9));
    d.apply(tx, UndoPolicy::NewGroup);
    assert_eq!(d.text().to_string(), "# Hello\n\n**world**\n");
    assert!(d.is_dirty());
    let blocks = d.ast().to_blocks();
    assert!(matches!(
        blocks[1].inlines[0].kind,
        inkra_markdown::InlineKind::Strong { .. }
    ));
    let sel = d.undo().unwrap();
    assert_eq!(sel, Selection::caret(9));
    assert_eq!(d.text().to_string(), "# Hello\n\nworld\n");
    d.redo().unwrap();
    assert_eq!(d.text().to_string(), "# Hello\n\n**world**\n");
    assert!(d.undo().is_some());
    assert!(d.undo().is_none());
}

#[test]
fn typing_groups_by_word_and_pause() {
    let mut d = Doc::new(ParseOptions::default());
    let now = std::time::Instant::now();
    for (i, c) in "ab cd".chars().enumerate() {
        let pos = i;
        let s = c.to_string();
        let tx = Transaction::single(pos..pos, &s);
        let inv = tx.invert(d.text());
        d.apply_raw(&tx);
        d.undo.record(
            tx,
            inv,
            UndoPolicy::Typing,
            now + Duration::from_millis(10 * i as u64),
        );
    }
    // "ab " is one group, "cd" another
    assert_eq!(d.undo_stack().undo_len(), 2);
    d.undo();
    assert_eq!(d.text().to_string(), "ab ");
    // a pause splits groups
    let tx = Transaction::single(3..3, "x");
    let inv = tx.invert(d.text());
    d.apply_raw(&tx);
    d.undo
        .record(tx, inv, UndoPolicy::Typing, now + Duration::from_secs(5));
    assert_eq!(d.undo_stack().undo_len(), 2);
}

#[test]
fn multi_edit_transactions_and_mapping() {
    let mut d = Doc::from_str("one two three", ParseOptions::default());
    let tx = Transaction::many(vec![
        Edit {
            range: 8..13,
            text: "3".into(),
        },
        Edit {
            range: 0..3,
            text: "1111".into(),
        },
    ]);
    assert_eq!(tx.clone().normalized(13).map_pos(9, true), 10);
    let tx = tx.with_selection_before(Selection::new(4, 7));
    let after = d.apply(tx, UndoPolicy::NewGroup);
    assert_eq!(d.text().to_string(), "1111 two 3");
    assert_eq!(after, Selection::new(5, 8));
    d.undo();
    assert_eq!(d.text().to_string(), "one two three");
}

#[test]
fn line_ending_conversion() {
    let mut d = Doc::from_str("a\nb\n", ParseOptions::default());
    d.convert_line_endings(LineEnding::Crlf);
    assert_eq!(d.text().to_string(), "a\r\nb\r\n");
    assert_eq!(d.newline(), "\r\n");
    d.undo();
    assert_eq!(d.text().to_string(), "a\nb\n");
}

proptest! {
    #[test]
    fn undo_all_restores_original(
        seed in "[a-z#*\\- \n>`|]{0,60}",
        ops in prop::collection::vec((0usize..80, 0usize..6, "[a-z#*\\- \n>`|]{0,5}"), 1..25)
    ) {
        let mut d = Doc::from_str(&seed, ParseOptions::default());
        let mut model = seed.clone();
        for (pos, del, ins) in ops {
            let start = pos.min(model.len());
            let end = (start + del).min(model.len());
            model.replace_range(start..end, &ins);
            d.apply(Transaction::single(start..end, &ins), UndoPolicy::NewGroup);
            prop_assert_eq!(d.text().to_string(), model.clone());
            let full = inkra_markdown::parse(model.as_str(), &ParseOptions::default());
            prop_assert_eq!(d.ast().to_blocks(), full.blocks);
        }
        while d.undo().is_some() {}
        prop_assert_eq!(d.text().to_string(), seed.clone());
        let full = inkra_markdown::parse(seed.as_str(), &ParseOptions::default());
        prop_assert_eq!(d.ast().to_blocks(), full.blocks);
    }
}

#[test]
fn dirty_since_merges_edits() {
    let mut d = Doc::from_str("0123456789", ParseOptions::default());
    let v0 = d.version();
    d.apply(Transaction::single(2..2, "ab"), UndoPolicy::NewGroup); // 01ab23456789
    d.apply(Transaction::single(8..10, ""), UndoPolicy::NewGroup); // 01ab2345 89 -> "01ab234589"
    let (r, delta) = d.dirty_since(v0).unwrap();
    assert_eq!(d.text().to_string(), "01ab234589");
    assert_eq!(delta, 0);
    assert_eq!(r, 2..8);
    assert_eq!(d.dirty_since(d.version()), Some((0..0, 0)));
    assert!(d.dirty_since(v0.wrapping_sub(1)).is_none() || v0 == 0);
}
