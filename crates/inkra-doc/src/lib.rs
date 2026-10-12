//! # inkra-doc
//!
//! **Layer L1b** (see SPEC 2.2).
//!
//! The document model. The source text (a rope) is the single source of truth
//! (AD-2); the Markdown AST is a derived cache updated incrementally. Every change
//! is a [`Transaction`] of source edits, recorded on an undo stack with the
//! grouping rules of ED-015. No I/O happens here: files are decoded from and
//! encoded to bytes, preserving encoding, BOM and line endings (ED-106, MD-091).

mod selection;
mod transaction;
mod undo;

use std::time::Instant;

use inkra_markdown::{Document as Ast, ParseOptions};
use inkra_text::encoding::{self, Decoded, Encoding};
use inkra_text::{LineEnding, Text, eol};

pub use selection::Selection;
pub use transaction::{Edit, Transaction};
pub use undo::{UndoPolicy, UndoStack};

/// A Markdown document.
#[derive(Debug, Clone)]
pub struct Doc {
    text: Text,
    ast: Ast,
    /// Encoding used when saving.
    pub encoding: Encoding,
    /// Whether to write a byte order mark.
    pub bom: bool,
    /// Line ending used for line breaks inserted by editing.
    pub line_ending: LineEnding,
    /// The file could not be decoded losslessly (U+FFFD substitutions).
    pub lossy: bool,
    undo: UndoStack,
    version: u64,
    saved_version: u64,
    /// Recent raw edits: `(version after the edit, start, removed len, inserted len)`.
    log: std::collections::VecDeque<(u64, usize, usize, usize)>,
}

/// Number of raw edits remembered for [`Doc::dirty_since`].
const LOG_LEN: usize = 512;

impl Default for Doc {
    fn default() -> Self {
        Self::new(ParseOptions::default())
    }
}

impl Doc {
    /// An empty document (UTF-8, no BOM, LF).
    pub fn new(options: ParseOptions) -> Self {
        Self::from_str("", options)
    }

    /// A document with `s` as content.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str, options: ParseOptions) -> Self {
        let text = Text::from(s);
        let ast = Ast::parse(&text, options);
        Self {
            line_ending: eol::detect(s),
            text,
            ast,
            encoding: Encoding::Utf8,
            bom: false,
            lossy: false,
            undo: UndoStack::default(),
            version: 0,
            saved_version: 0,
            log: Default::default(),
        }
    }

    /// Decodes file bytes (ED-106). `fallback` is the preferred legacy encoding.
    pub fn from_bytes(bytes: &[u8], fallback: Option<Encoding>, options: ParseOptions) -> Self {
        let Decoded {
            text,
            encoding,
            bom,
            lossy,
        } = encoding::decode(bytes, fallback);
        let mut d = Self::from_str(&text, options);
        d.encoding = encoding;
        d.bom = bom;
        d.lossy = lossy;
        d
    }

    /// Encodes the document for saving. For an unmodified, losslessly decoded file
    /// this reproduces the original bytes exactly.
    pub fn to_bytes(&self) -> Vec<u8> {
        encoding::encode(&self.text.to_string(), self.encoding, self.bom)
    }

    /// The source text.
    pub fn text(&self) -> &Text {
        &self.text
    }

    /// The parsed Markdown.
    pub fn ast(&self) -> &Ast {
        &self.ast
    }

    /// Parse options.
    pub fn options(&self) -> &ParseOptions {
        self.ast.options()
    }

    /// Changes parse options (full re-parse; MD-080).
    pub fn set_options(&mut self, options: ParseOptions) {
        self.ast.set_options(&self.text, options);
        self.version += 1;
        // Not a text change, but layouts depend on the parse: force a full rebuild.
        self.log.clear();
    }

    /// The region changed since `version`: `(range in the current text, total
    /// length delta)`. `None` if the history does not reach back that far (callers
    /// rebuild everything); `Some((empty, 0))` if nothing changed.
    pub fn dirty_since(&self, version: u64) -> Option<(std::ops::Range<usize>, isize)> {
        if version == self.version {
            return Some((0..0, 0));
        }
        let first = self.log.iter().position(|e| e.0 > version)?;
        if first == 0 && self.log.front().is_some_and(|e| e.0 != version + 1) {
            return None;
        }
        let mut dirty: Option<std::ops::Range<usize>> = None;
        let mut delta: isize = 0;
        for &(_, start, removed, inserted) in self.log.iter().skip(first) {
            let end = start + removed;
            let d = inserted as isize - removed as isize;
            let map = |p: usize| -> usize {
                if p < start {
                    p
                } else if p >= end {
                    (p as isize + d) as usize
                } else {
                    start + inserted
                }
            };
            let new = start..start + inserted;
            dirty = Some(match dirty {
                None => new,
                Some(r) => {
                    let (a, b) = (map(r.start), map(r.end));
                    a.min(new.start)..b.max(new.end)
                }
            });
            delta += d;
        }
        Some((dirty.unwrap_or(0..0), delta))
    }

    /// Monotonic version, incremented by every change.
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Whether there are changes since the last [`Doc::mark_saved`].
    pub fn is_dirty(&self) -> bool {
        self.version != self.saved_version
    }

    /// Records that the current state was saved.
    pub fn mark_saved(&mut self) {
        self.saved_version = self.version;
    }

    /// Marks the document dirty without changing it (e.g. after "save as" target
    /// was lost).
    pub fn mark_dirty(&mut self) {
        self.saved_version = u64::MAX;
    }

    /// The line break string for inserted line breaks.
    pub fn newline(&self) -> &'static str {
        self.line_ending.as_str()
    }

    /// The undo stack.
    pub fn undo_stack(&self) -> &UndoStack {
        &self.undo
    }

    /// Applies `tx` and records it according to `policy`. Returns the selection
    /// after the transaction (its `selection_after`, or the mapped `before`).
    pub fn apply(&mut self, tx: Transaction, policy: UndoPolicy) -> Selection {
        let tx = tx.normalized(self.text.len());
        if tx.edits.is_empty() {
            return tx
                .selection_after
                .or(tx.selection_before)
                .unwrap_or_default();
        }
        let inverse = self.apply_raw(&tx);
        let after = tx
            .selection_after
            .or_else(|| tx.selection_before.map(|s| tx.map_selection(s)))
            .unwrap_or_default();
        if policy != UndoPolicy::None {
            self.undo.record(tx, inverse, policy, Instant::now());
        }
        after
    }

    /// Applies edits to text and AST; returns the inverse transaction.
    fn apply_raw(&mut self, tx: &Transaction) -> Transaction {
        let inverse = tx.invert(&self.text);
        // Apply from the end so earlier ranges stay valid.
        self.version += 1;
        for e in tx.edits.iter().rev() {
            self.text.replace(e.range.clone(), &e.text);
            self.ast.edit(&self.text, e.range.clone(), e.text.len());
            self.log
                .push_back((self.version, e.range.start, e.range.len(), e.text.len()));
            if self.log.len() > LOG_LEN {
                self.log.pop_front();
            }
        }
        inverse
    }

    /// Convenience: replaces `range` with `text` as one transaction.
    pub fn replace(
        &mut self,
        range: std::ops::Range<usize>,
        text: &str,
        policy: UndoPolicy,
    ) -> Selection {
        let end = range.start + text.len();
        let tx = Transaction::single(range, text).with_selection_after(Selection::caret(end));
        self.apply(tx, policy)
    }

    /// Undoes the last group; returns the selection to restore.
    pub fn undo(&mut self) -> Option<Selection> {
        let group = self.undo.pop_undo()?;
        let mut sel = None;
        for (_, inverse) in group.entries.iter().rev() {
            self.apply_raw(inverse);
            sel = inverse.selection_after;
        }
        let restore = group
            .entries
            .first()
            .and_then(|(tx, _)| tx.selection_before)
            .or(sel);
        self.undo.push_redo(group);
        Some(restore.unwrap_or_default())
    }

    /// Redoes the last undone group; returns the selection to restore.
    pub fn redo(&mut self) -> Option<Selection> {
        let group = self.undo.pop_redo()?;
        let mut sel = None;
        for (tx, _) in &group.entries {
            self.apply_raw(tx);
            sel = tx
                .selection_after
                .or_else(|| tx.selection_before.map(|s| tx.map_selection(s)));
        }
        self.undo.push_undo_after_redo(group);
        Some(sel.unwrap_or_default())
    }

    /// Ends the current typing group so the next edit starts a new undo group.
    pub fn break_undo_group(&mut self) {
        self.undo.seal();
    }

    /// Converts all line endings to `eol` (one undo group).
    pub fn convert_line_endings(&mut self, eol: LineEnding) {
        let old = self.text.to_string();
        let new = inkra_text::eol::convert(&old, eol);
        self.line_ending = eol;
        if new != old {
            let tx = Transaction::single(0..old.len(), &new);
            self.apply(tx, UndoPolicy::NewGroup);
        }
    }
}

#[cfg(test)]
mod tests;
