//! The editor state machine: a document, its selection and view mode, and the
//! operations that change them (all changes go through transactions).

pub mod clipboard;
pub mod context;
pub mod find;
pub mod format_ops;
pub mod live_ops;
pub mod ops;

use inkra_doc::{Doc, Selection, Transaction, UndoPolicy};
use inkra_markdown::ParseOptions;

pub use clipboard::ClipboardContent;
pub use find::{FindOptions, FindState};
pub use ops::{Break, EditPrefs, Unit};

/// How the document is displayed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ViewMode {
    /// Live rendering (ED-001).
    #[default]
    Live,
    /// Source mode (ED-120).
    Source,
}

/// One open document with its editing state.
#[derive(Debug, Clone)]
pub struct Editor {
    /// The document.
    pub doc: Doc,
    /// Selection (byte offsets).
    pub sel: Selection,
    /// View mode.
    pub mode: ViewMode,
    /// Find bar.
    pub find: FindState,
    /// Editing preferences.
    pub prefs: EditPrefs,
    /// IME pre-edit text shown at the caret (not part of the document).
    pub preedit: Option<String>,
    /// File on disk, if any.
    pub path: Option<std::path::PathBuf>,
}

impl Default for Editor {
    fn default() -> Self {
        Self::new(Doc::default())
    }
}

impl Editor {
    /// An editor for `doc` with the caret at the start.
    pub fn new(doc: Doc) -> Self {
        Self {
            doc,
            sel: Selection::caret(0),
            mode: ViewMode::default(),
            find: FindState::default(),
            prefs: EditPrefs::default(),
            preedit: None,
            path: None,
        }
    }

    /// An editor for a string.
    pub fn from_text(s: &str) -> Self {
        Self::new(Doc::from_str(s, ParseOptions::default()))
    }

    /// The text as a `String` (for tests and small documents).
    pub fn text(&self) -> String {
        self.doc.text().to_string()
    }

    /// Applies a transaction and updates the selection.
    pub fn apply(&mut self, tx: Transaction, policy: UndoPolicy) {
        let after = self.doc.apply(tx, policy);
        self.sel = after.clamp(self.doc.text().len());
        self.find.invalidate();
    }

    /// Applies a transaction, then renumbers the ordered list at the caret,
    /// merged into the same undo group (ED-101).
    pub fn apply_with_renumber(&mut self, tx: Transaction, policy: UndoPolicy) {
        self.apply(tx, policy);
        if let Some(r) = ops::renumber_list(&self.doc, self.sel.head, &self.prefs) {
            let sel = self.sel;
            let r = r.with_selection_before(sel);
            let mapped = r.map_selection(sel);
            self.apply(r.with_selection_after(mapped), UndoPolicy::Merge);
        }
    }

    /// Sets the selection (clamped to char boundaries).
    pub fn set_selection(&mut self, s: Selection) {
        let t = self.doc.text();
        self.sel = Selection::new(t.snap(s.anchor), t.snap(s.head));
    }

    /// Types `s` at the selection (ED-010, ED-100).
    pub fn type_text(&mut self, s: &str) {
        if s.is_empty() {
            return;
        }
        let tx = ops::type_text(&self.doc, self.sel, s, &self.prefs);
        if tx.edits.is_empty() {
            self.sel = tx.selection_after.unwrap_or(self.sel);
            self.doc.break_undo_group();
            return;
        }
        self.apply(tx, UndoPolicy::Typing);
    }

    /// Backspace.
    pub fn delete_backward(&mut self, unit: Unit) {
        if let Some(tx) = ops::delete_backward(&self.doc, self.sel, unit, &self.prefs) {
            let policy = if unit == Unit::Char && self.sel.is_empty() {
                UndoPolicy::Typing
            } else {
                UndoPolicy::NewGroup
            };
            self.apply_with_renumber(tx, policy);
        }
    }

    /// Delete.
    pub fn delete_forward(&mut self, unit: Unit) {
        if let Some(tx) = ops::delete_forward(&self.doc, self.sel, unit) {
            self.apply_with_renumber(tx, UndoPolicy::NewGroup);
        }
    }

    /// Enter / Shift+Enter.
    pub fn newline(&mut self, kind: Break) {
        let tx = ops::newline(&self.doc, self.sel, kind);
        self.doc.break_undo_group();
        self.apply_with_renumber(tx, UndoPolicy::NewGroup);
    }

    /// Enter / Shift+Enter in live mode (ED-013).
    pub fn newline_live(&mut self, kind: Break) {
        let tx = live_ops::newline(&self.doc, self.sel, kind);
        self.doc.break_undo_group();
        if tx.edits.is_empty() {
            return;
        }
        self.apply_with_renumber(tx, UndoPolicy::NewGroup);
    }

    /// Backspace in live mode (ED-012).
    pub fn delete_backward_live(&mut self, unit: Unit) {
        if let Some(tx) = live_ops::delete_backward(&self.doc, self.sel, unit, &self.prefs) {
            if tx.edits.is_empty() {
                self.sel = tx.selection_after.unwrap_or(self.sel);
                return;
            }
            let policy = if unit == Unit::Char
                && self.sel.is_empty()
                && tx.edits.len() == 1
                && tx.edits[0].range.len() <= 4
            {
                UndoPolicy::Typing
            } else {
                UndoPolicy::NewGroup
            };
            self.apply_with_renumber(tx, policy);
        }
    }

    /// Delete in live mode (ED-012).
    pub fn delete_forward_live(&mut self, unit: Unit) {
        if let Some(tx) = live_ops::delete_forward(&self.doc, self.sel, unit) {
            if tx.edits.is_empty() {
                self.sel = tx.selection_after.unwrap_or(self.sel);
                return;
            }
            self.apply_with_renumber(tx, UndoPolicy::NewGroup);
        }
    }

    /// Tab / Shift+Tab.
    pub fn tab(&mut self, outdent: bool) {
        if let Some(tx) = ops::tab(&self.doc, self.sel, outdent, &self.prefs) {
            self.apply_with_renumber(tx, UndoPolicy::NewGroup);
        }
    }

    /// Undo; returns false when there is nothing to undo.
    pub fn undo(&mut self) -> bool {
        match self.doc.undo() {
            Some(s) => {
                self.sel = s.clamp(self.doc.text().len());
                self.find.invalidate();
                true
            }
            None => false,
        }
    }

    /// Redo.
    pub fn redo(&mut self) -> bool {
        match self.doc.redo() {
            Some(s) => {
                self.sel = s.clamp(self.doc.text().len());
                self.find.invalidate();
                true
            }
            None => false,
        }
    }

    /// Pastes clipboard content.
    pub fn paste(&mut self, c: &ClipboardContent, plain: bool) {
        if let Some(tx) = clipboard::paste(&self.doc, self.sel, c, plain) {
            self.doc.break_undo_group();
            self.apply(tx, UndoPolicy::NewGroup);
        }
    }

    /// Cuts the selection; returns the clipboard payload.
    pub fn cut(&mut self) -> Option<(String, String)> {
        let payload = clipboard::copy_payload(&self.doc, self.sel)?;
        let r = self.sel.range();
        let tx = Transaction::single(r.clone(), "")
            .with_selection_before(self.sel)
            .with_selection_after(Selection::caret(r.start));
        self.apply(tx, UndoPolicy::NewGroup);
        Some(payload)
    }
}

#[cfg(test)]
mod format_tests;
#[cfg(test)]
mod tests;
