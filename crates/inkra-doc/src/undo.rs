//! Undo stack with ED-015 grouping.

use std::time::{Duration, Instant};

use crate::Transaction;

/// How a transaction is recorded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UndoPolicy {
    /// Its own undo group (commands, paste).
    NewGroup,
    /// Typed text: merged with the previous typing group while typing continues
    /// within the same word and without a 500 ms pause.
    Typing,
    /// Merged into the previous group regardless (automatic follow-ups such as
    /// list renumbering).
    Merge,
    /// Not recorded.
    None,
}

/// One undo group: transactions and their inverses, in application order.
#[derive(Debug, Clone)]
pub struct Group {
    /// `(transaction, inverse)` pairs.
    pub entries: Vec<(Transaction, Transaction)>,
    typing: bool,
    last: Instant,
    sealed: bool,
    bytes: usize,
}

/// Pause that ends a typing group.
const TYPING_PAUSE: Duration = Duration::from_millis(500);
/// Memory cap after which old groups are dropped (keeping at least 1000).
const MAX_BYTES: usize = 64 << 20;
const MIN_GROUPS: usize = 1000;

/// Undo/redo history.
#[derive(Debug, Clone, Default)]
pub struct UndoStack {
    undo: Vec<Group>,
    redo: Vec<Group>,
    bytes: usize,
}

fn tx_bytes(tx: &Transaction) -> usize {
    tx.edits.iter().map(|e| e.text.len() + 16).sum()
}

fn typed_text(tx: &Transaction) -> Option<(usize, &str)> {
    match tx.edits.as_slice() {
        [e] if e.range.is_empty() => Some((e.range.start, e.text.as_str())),
        _ => None,
    }
}

impl UndoStack {
    /// Number of undo groups.
    pub fn undo_len(&self) -> usize {
        self.undo.len()
    }

    /// Number of redo groups.
    pub fn redo_len(&self) -> usize {
        self.redo.len()
    }

    /// Whether undo is possible.
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    /// Whether redo is possible.
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Ends the current group.
    pub fn seal(&mut self) {
        if let Some(g) = self.undo.last_mut() {
            g.sealed = true;
        }
    }

    pub(crate) fn record(
        &mut self,
        tx: Transaction,
        inverse: Transaction,
        policy: UndoPolicy,
        now: Instant,
    ) {
        self.redo.clear();
        let bytes = tx_bytes(&tx) + tx_bytes(&inverse);
        self.bytes += bytes;
        let merge = match (policy, self.undo.last()) {
            (UndoPolicy::Merge, Some(_)) => true,
            (UndoPolicy::Typing, Some(g))
                if g.typing && !g.sealed && now.duration_since(g.last) < TYPING_PAUSE =>
            {
                let prev = g.entries.last().map(|(t, _)| t);
                match (prev.and_then(typed_text), typed_text(&tx)) {
                    (Some((p, ptext)), Some((q, qtext))) => {
                        // contiguous typing, and a word boundary has not just ended
                        let contiguous = q == p + ptext.len();
                        let prev_space = ptext.ends_with(char::is_whitespace);
                        let this_space = qtext.starts_with(char::is_whitespace);
                        contiguous && (!prev_space || this_space)
                    }
                    _ => false,
                }
            }
            _ => false,
        };
        if merge {
            if let Some(g) = self.undo.last_mut() {
                g.entries.push((tx, inverse));
                g.last = now;
                g.bytes += bytes;
            }
        } else {
            self.undo.push(Group {
                entries: vec![(tx, inverse)],
                typing: policy == UndoPolicy::Typing,
                last: now,
                sealed: false,
                bytes,
            });
        }
        while self.bytes > MAX_BYTES && self.undo.len() > MIN_GROUPS {
            let g = self.undo.remove(0);
            self.bytes -= g.bytes;
        }
    }

    pub(crate) fn pop_undo(&mut self) -> Option<Group> {
        let g = self.undo.pop()?;
        self.bytes -= g.bytes;
        Some(g)
    }

    pub(crate) fn push_redo(&mut self, mut g: Group) {
        g.sealed = true;
        self.redo.push(g);
    }

    pub(crate) fn pop_redo(&mut self) -> Option<Group> {
        self.redo.pop()
    }

    pub(crate) fn push_undo_after_redo(&mut self, mut g: Group) {
        g.sealed = true;
        self.bytes += g.bytes;
        self.undo.push(g);
    }
}
