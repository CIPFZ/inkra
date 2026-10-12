//! Incremental item sync must equal a full rebuild.

use inkra_doc::{Doc, Transaction, UndoPolicy};
use inkra_layout::{Mode, Style, ViewLayout};
use proptest::prelude::*;

fn ranges(v: &ViewLayout) -> Vec<(std::ops::Range<usize>, u64)> {
    v.items()
        .iter()
        .map(|i| (i.src.clone(), i.text_hash))
        .collect()
}

proptest! {
    #[test]
    fn incremental_sync_matches_rebuild(
        seed in "[a-c\\n\\r#\\- ]{0,40}",
        edits in prop::collection::vec(prop::collection::vec((0usize..60, 0usize..5, "[a-c\\n\\r ]{0,4}"), 1..4), 1..8)
    ) {
        let style = Style::default();
        let mode = Mode::Source { wrap: true };
        let mut doc = Doc::from_str(&seed, Default::default());
        let mut inc = ViewLayout::new();
        inc.sync(&doc, mode, 500.0, &style, 0);
        for batch in edits {
            // several transactions between syncs
            for (pos, del, ins) in batch {
                let len = doc.text().len();
                let start = doc.text().snap(pos.min(len));
                let end = doc.text().snap((start + del).min(len));
                doc.apply(Transaction::single(start..end, &ins), UndoPolicy::NewGroup);
            }
            inc.sync(&doc, mode, 500.0, &style, 0);
            let mut full = ViewLayout::new();
            full.sync(&doc, mode, 500.0, &style, 0);
            prop_assert_eq!(ranges(&inc), ranges(&full), "text {:?}", doc.text().to_string());
        }
        // undo everything too
        while doc.undo().is_some() {
            inc.sync(&doc, mode, 500.0, &style, 0);
        }
        let mut full = ViewLayout::new();
        full.sync(&doc, mode, 500.0, &style, 0);
        prop_assert_eq!(ranges(&inc), ranges(&full));
    }
}
