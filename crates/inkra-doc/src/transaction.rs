//! Transactions: the only way the document changes (AD-2).

use std::ops::Range;

use inkra_text::Text;

use crate::Selection;

/// Replace `range` (in the text before the transaction) with `text`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    /// Range in the pre-transaction text.
    pub range: Range<usize>,
    /// Replacement.
    pub text: String,
}

/// A set of non-overlapping edits applied atomically, plus the selection before
/// and after (restored by undo/redo, ED-015).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Transaction {
    /// Edits, sorted by start, non-overlapping, in pre-transaction coordinates.
    pub edits: Vec<Edit>,
    /// Selection before the change.
    pub selection_before: Option<Selection>,
    /// Selection after the change.
    pub selection_after: Option<Selection>,
}

impl Transaction {
    /// A transaction with one edit.
    pub fn single(range: Range<usize>, text: &str) -> Self {
        Self {
            edits: vec![Edit {
                range,
                text: text.to_owned(),
            }],
            ..Default::default()
        }
    }

    /// A transaction from several edits (sorted and validated by `normalized`).
    pub fn many(edits: Vec<Edit>) -> Self {
        Self {
            edits,
            ..Default::default()
        }
    }

    /// Sets the selection before.
    pub fn with_selection_before(mut self, s: Selection) -> Self {
        self.selection_before = Some(s);
        self
    }

    /// Sets the selection after.
    pub fn with_selection_after(mut self, s: Selection) -> Self {
        self.selection_after = Some(s);
        self
    }

    /// Sorts edits, clamps them to `len`, drops no-ops and merges overlaps (a
    /// later overlapping edit is merged into the earlier range).
    pub fn normalized(mut self, len: usize) -> Self {
        for e in &mut self.edits {
            e.range.start = e.range.start.min(len);
            e.range.end = e.range.end.clamp(e.range.start, len);
        }
        self.edits
            .retain(|e| !(e.range.is_empty() && e.text.is_empty()));
        self.edits.sort_by_key(|e| (e.range.start, e.range.end));
        let mut out: Vec<Edit> = Vec::with_capacity(self.edits.len());
        for e in self.edits {
            if let Some(last) = out.last_mut()
                && e.range.start < last.range.end
            {
                last.range.end = last.range.end.max(e.range.end);
                last.text.push_str(&e.text);
                continue;
            }
            out.push(e);
        }
        self.edits = out;
        self
    }

    /// The inverse transaction, given the text *before* this one is applied.
    pub fn invert(&self, before: &Text) -> Transaction {
        let mut shift: isize = 0;
        let mut edits = Vec::with_capacity(self.edits.len());
        for e in &self.edits {
            let start = (e.range.start as isize + shift) as usize;
            let removed = before.slice(e.range.clone()).into_owned();
            edits.push(Edit {
                range: start..start + e.text.len(),
                text: removed,
            });
            shift += e.text.len() as isize - (e.range.end - e.range.start) as isize;
        }
        Transaction {
            edits,
            selection_before: self.selection_after,
            selection_after: self.selection_before,
        }
    }

    /// Maps a position through the transaction. A position inside or at the end of
    /// a replaced range moves to the end of the replacement; a position at the
    /// start of a range stays before it, except at an insertion point when `after`
    /// is set.
    pub fn map_pos(&self, pos: usize, after: bool) -> usize {
        let mut shift: isize = 0;
        for e in &self.edits {
            let new_start = (e.range.start as isize + shift) as usize;
            if pos < e.range.start {
                break;
            }
            if pos == e.range.start {
                return if e.range.is_empty() && after {
                    new_start + e.text.len()
                } else {
                    new_start
                };
            }
            if pos <= e.range.end {
                return new_start + e.text.len();
            }
            shift += e.text.len() as isize - (e.range.end - e.range.start) as isize;
        }
        (pos as isize + shift).max(0) as usize
    }

    /// Maps a selection through the transaction.
    pub fn map_selection(&self, s: Selection) -> Selection {
        Selection {
            anchor: self.map_pos(s.anchor, true),
            head: self.map_pos(s.head, true),
        }
    }

    /// Net change in length.
    pub fn delta(&self) -> isize {
        self.edits
            .iter()
            .map(|e| e.text.len() as isize - (e.range.end - e.range.start) as isize)
            .sum()
    }
}
