//! Selections (byte offsets into the source).

/// A selection: `anchor` is where it started, `head` where the caret is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct Selection {
    /// Fixed end.
    pub anchor: usize,
    /// Moving end (caret).
    pub head: usize,
}

impl Selection {
    /// A collapsed selection.
    pub const fn caret(pos: usize) -> Self {
        Self {
            anchor: pos,
            head: pos,
        }
    }

    /// A selection from `anchor` to `head`.
    pub const fn new(anchor: usize, head: usize) -> Self {
        Self { anchor, head }
    }

    /// Smaller end.
    pub fn start(&self) -> usize {
        self.anchor.min(self.head)
    }

    /// Larger end.
    pub fn end(&self) -> usize {
        self.anchor.max(self.head)
    }

    /// The selected range.
    pub fn range(&self) -> std::ops::Range<usize> {
        self.start()..self.end()
    }

    /// Whether nothing is selected.
    pub fn is_empty(&self) -> bool {
        self.anchor == self.head
    }

    /// Clamps both ends to `len`.
    pub fn clamp(self, len: usize) -> Self {
        Self {
            anchor: self.anchor.min(len),
            head: self.head.min(len),
        }
    }
}
