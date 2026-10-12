//! Abstraction over the document text so the parser can read a `&str` or a rope.

use std::borrow::Cow;
use std::ops::Range;

use inkra_text::Text;

/// Read access to document text by byte offsets.
pub trait Source {
    /// Total length in bytes.
    fn len(&self) -> usize;
    /// Whether empty.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    /// Text in `range`.
    fn slice(&self, range: Range<usize>) -> Cow<'_, str>;
    /// Byte range (including terminator) of the line starting at or containing `offset`.
    fn line_at(&self, offset: usize) -> Range<usize>;
    /// Calls `f` with each line (including its terminator) starting with the line
    /// that begins at `offset`, until `f` returns `false` or the text ends.
    fn for_each_line_from(&self, offset: usize, f: &mut dyn FnMut(&str) -> bool);
}

fn str_lines_from(s: &str, offset: usize, f: &mut dyn FnMut(&str) -> bool) {
    let b = s.as_bytes();
    let mut start = offset.min(b.len());
    while start < b.len() {
        let mut end = start;
        while end < b.len() && b[end] != b'\n' && b[end] != b'\r' {
            end += 1;
        }
        if end < b.len() {
            end += if b[end] == b'\r' && b.get(end + 1) == Some(&b'\n') {
                2
            } else {
                1
            };
        }
        if !f(&s[start..end]) {
            return;
        }
        start = end;
    }
}

impl Source for str {
    fn len(&self) -> usize {
        str::len(self)
    }
    fn slice(&self, range: Range<usize>) -> Cow<'_, str> {
        Cow::Borrowed(&self[range])
    }
    fn line_at(&self, offset: usize) -> Range<usize> {
        let b = self.as_bytes();
        let mut start = offset.min(b.len());
        while start > 0 && b[start - 1] != b'\n' && b[start - 1] != b'\r' {
            start -= 1;
        }
        // A lone `\r\n` split: offset just after `\r` of a CRLF belongs to that line.
        let mut end = start;
        while end < b.len() && b[end] != b'\n' && b[end] != b'\r' {
            end += 1;
        }
        if end < b.len() {
            end += if b[end] == b'\r' && b.get(end + 1) == Some(&b'\n') {
                2
            } else {
                1
            };
        }
        start..end
    }
    fn for_each_line_from(&self, offset: usize, f: &mut dyn FnMut(&str) -> bool) {
        str_lines_from(self, offset, f);
    }
}

impl Source for String {
    fn len(&self) -> usize {
        self.as_str().len()
    }
    fn slice(&self, range: Range<usize>) -> Cow<'_, str> {
        Cow::Borrowed(&self[range])
    }
    fn line_at(&self, offset: usize) -> Range<usize> {
        self.as_str().line_at(offset)
    }
    fn for_each_line_from(&self, offset: usize, f: &mut dyn FnMut(&str) -> bool) {
        str_lines_from(self, offset, f);
    }
}

impl Source for Text {
    fn len(&self) -> usize {
        Text::len(self)
    }
    fn slice(&self, range: Range<usize>) -> Cow<'_, str> {
        Text::slice(self, range)
    }
    fn line_at(&self, offset: usize) -> Range<usize> {
        self.line_range(self.byte_to_line(offset))
    }
    fn for_each_line_from(&self, offset: usize, f: &mut dyn FnMut(&str) -> bool) {
        let first = self.byte_to_line(offset);
        let mut buf = String::new();
        for line in self.rope().lines_at(first) {
            if line.len_bytes() == 0 {
                break;
            }
            let keep_going = match line.as_str() {
                Some(s) => f(s),
                None => {
                    buf.clear();
                    for chunk in line.chunks() {
                        buf.push_str(chunk);
                    }
                    f(&buf)
                }
            };
            if !keep_going {
                break;
            }
        }
    }
}
