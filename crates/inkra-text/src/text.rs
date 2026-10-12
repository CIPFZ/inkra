//! Rope wrapper addressed by byte offsets.

use std::borrow::Cow;
use std::ops::Range;

use ropey::Rope;

/// Document text. All offsets are byte offsets into the UTF-8 text and must lie on
/// char boundaries; methods clamp offsets to the text length.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Text {
    rope: Rope,
}

impl std::fmt::Debug for Text {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Text({} bytes)", self.len())
    }
}

impl From<&str> for Text {
    fn from(s: &str) -> Self {
        Self {
            rope: Rope::from_str(s),
        }
    }
}

impl From<String> for Text {
    fn from(s: String) -> Self {
        Self::from(s.as_str())
    }
}

impl std::fmt::Display for Text {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for chunk in self.rope.chunks() {
            f.write_str(chunk)?;
        }
        Ok(())
    }
}

impl Text {
    /// Empty text.
    pub fn new() -> Self {
        Self::default()
    }

    /// Length in bytes.
    pub fn len(&self) -> usize {
        self.rope.len_bytes()
    }

    /// Whether the text is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Number of chars.
    pub fn len_chars(&self) -> usize {
        self.rope.len_chars()
    }

    /// Number of lines (`\n`, `\r\n` and lone `\r` end lines; an empty text and a
    /// text ending in a line break have a final empty line).
    pub fn len_lines(&self) -> usize {
        self.rope.len_lines()
    }

    fn clamp(&self, byte: usize) -> usize {
        let byte = byte.min(self.len());
        // Snap to a char boundary (towards the start).
        let c = self.rope.byte_to_char(byte);
        self.rope.char_to_byte(c)
    }

    /// Char index of `byte`.
    pub fn byte_to_char(&self, byte: usize) -> usize {
        self.rope.byte_to_char(byte.min(self.len()))
    }

    /// Byte offset of char index `ch`.
    pub fn char_to_byte(&self, ch: usize) -> usize {
        self.rope.char_to_byte(ch.min(self.len_chars()))
    }

    /// Line index containing `byte`.
    pub fn byte_to_line(&self, byte: usize) -> usize {
        self.rope.byte_to_line(byte.min(self.len()))
    }

    /// Byte offset of the start of `line` (clamped to the last line).
    pub fn line_to_byte(&self, line: usize) -> usize {
        self.rope.line_to_byte(line.min(self.len_lines()))
    }

    /// Byte range of `line` including its line terminator.
    pub fn line_range(&self, line: usize) -> Range<usize> {
        let start = self.line_to_byte(line);
        let end = if line + 1 >= self.len_lines() {
            self.len()
        } else {
            self.line_to_byte(line + 1)
        };
        start..end
    }

    /// Byte range of `line` without its line terminator.
    pub fn line_content_range(&self, line: usize) -> Range<usize> {
        let r = self.line_range(line);
        let s = self.slice(r.clone());
        let trimmed = s.trim_end_matches(['\n', '\r']).len();
        r.start..r.start + trimmed
    }

    /// `(line, column in bytes)` of `byte`.
    pub fn byte_to_line_col(&self, byte: usize) -> (usize, usize) {
        let byte = self.clamp(byte);
        let line = self.rope.byte_to_line(byte);
        (line, byte - self.rope.line_to_byte(line))
    }

    /// Text of `line` including its terminator. Borrowed when contiguous in the rope.
    pub fn line(&self, line: usize) -> Cow<'_, str> {
        self.slice(self.line_range(line))
    }

    /// Text in `range` (clamped). Borrowed when contiguous in the rope.
    pub fn slice(&self, range: Range<usize>) -> Cow<'_, str> {
        let start = self.clamp(range.start);
        let end = self.clamp(range.end.max(range.start));
        let s = self.rope.byte_slice(start..end);
        match s.as_str() {
            Some(s) => Cow::Borrowed(s),
            None => Cow::Owned(s.to_string()),
        }
    }

    /// The char starting at `byte`.
    pub fn char_at(&self, byte: usize) -> Option<char> {
        if byte >= self.len() {
            return None;
        }
        let c = self.rope.byte_to_char(byte);
        Some(self.rope.char(c))
    }

    /// The char ending at `byte`.
    pub fn char_before(&self, byte: usize) -> Option<char> {
        let byte = self.clamp(byte);
        if byte == 0 {
            return None;
        }
        let c = self.rope.byte_to_char(byte);
        Some(self.rope.char(c - 1))
    }

    /// Byte offset of the char boundary after `byte`.
    pub fn next_char(&self, byte: usize) -> usize {
        match self.char_at(self.clamp(byte)) {
            Some(c) => self.clamp(byte) + c.len_utf8(),
            None => self.len(),
        }
    }

    /// Byte offset of the char boundary before `byte`.
    pub fn prev_char(&self, byte: usize) -> usize {
        let byte = self.clamp(byte);
        match self.char_before(byte) {
            Some(c) => byte - c.len_utf8(),
            None => 0,
        }
    }

    /// Inserts `s` at `byte`.
    pub fn insert(&mut self, byte: usize, s: &str) {
        let c = self.byte_to_char(self.clamp(byte));
        self.rope.insert(c, s);
    }

    /// Removes `range`.
    pub fn remove(&mut self, range: Range<usize>) {
        let start = self.byte_to_char(self.clamp(range.start));
        let end = self.byte_to_char(self.clamp(range.end.max(range.start)));
        self.rope.remove(start..end);
    }

    /// Replaces `range` with `s`.
    pub fn replace(&mut self, range: Range<usize>, s: &str) {
        let start = self.clamp(range.start);
        self.remove(start..range.end.max(start));
        self.insert(start, s);
    }

    /// Iterates over the rope's string chunks.
    pub fn chunks(&self) -> impl Iterator<Item = &str> {
        self.rope.chunks()
    }

    /// Iterates over chars starting at `byte`.
    pub fn chars_at(&self, byte: usize) -> impl Iterator<Item = char> + '_ {
        self.rope.chars_at(self.byte_to_char(self.clamp(byte)))
    }

    /// Iterates backwards over chars ending at `byte`.
    pub fn chars_before(&self, byte: usize) -> impl Iterator<Item = char> + '_ {
        let mut it = self.rope.chars_at(self.byte_to_char(self.clamp(byte)));
        std::iter::from_fn(move || it.prev())
    }

    /// Whether `byte` is a char boundary within the text.
    pub fn is_char_boundary(&self, byte: usize) -> bool {
        byte <= self.len() && self.clamp(byte) == byte
    }

    /// Snaps `byte` to a char boundary (towards the start) within the text.
    pub fn snap(&self, byte: usize) -> usize {
        self.clamp(byte)
    }

    /// The underlying rope.
    pub fn rope(&self) -> &Rope {
        &self.rope
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn lines_and_offsets() {
        let t = Text::from("ab\r\nc\nd\r中");
        assert_eq!(t.len_lines(), 4);
        assert_eq!(t.line(0), "ab\r\n");
        assert_eq!(t.line_content_range(0), 0..2);
        assert_eq!(t.line(2), "d\r");
        assert_eq!(t.line(3), "中");
        assert_eq!(t.byte_to_line_col(5), (1, 1));
        assert_eq!(t.char_at(8), Some('中'));
        assert_eq!(t.next_char(8), 11);
        assert_eq!(t.prev_char(11), 8);
        assert_eq!(t.snap(9), 8);
        assert!(!t.is_char_boundary(9));
        let back: String = t.chars_before(11).take(2).collect();
        assert_eq!(back, "中\r");
    }

    #[test]
    fn edits() {
        let mut t = Text::from("hello world");
        t.replace(6..11, "Inkra");
        t.insert(0, "# ");
        t.remove(7..8);
        assert_eq!(t.to_string(), "# helloInkra");
    }

    proptest! {
        #[test]
        fn matches_string_model(ops in prop::collection::vec((0usize..64, 0usize..8, "[a-c中\\n\\r]{0,4}"), 0..40)) {
            let mut model = String::from("seed\ntext 中文\r\n");
            let mut t = Text::from(model.as_str());
            for (pos, del, ins) in ops {
                let mut start = pos.min(model.len());
                while !model.is_char_boundary(start) { start -= 1; }
                let mut end = (start + del).min(model.len());
                while !model.is_char_boundary(end) { end -= 1; }
                model.replace_range(start..end, &ins);
                t.replace(start..end, &ins);
                prop_assert_eq!(t.to_string(), model.clone());
                prop_assert_eq!(t.len(), model.len());
            }
        }
    }
}
