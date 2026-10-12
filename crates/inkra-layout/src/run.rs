//! Text runs: a layout job placed in a block plus a mapping between the job's
//! text and the document source.

use std::ops::Range;
use std::sync::Arc;

use epaint::text::cursor::CCursor;
use epaint::text::{Galley, LayoutJob};
use epaint::{Pos2, Rect, Vec2, pos2, vec2};

/// One mapped piece of a run's text.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Seg {
    /// Byte range in the job text.
    pub text: Range<usize>,
    /// Source byte range relative to the item start.
    pub src: Range<usize>,
    /// The text does not correspond byte-for-byte to the source (entities,
    /// rendered symbols): positions inside map to the segment ends only.
    pub atomic: bool,
}

/// Mapping between a run's text and the source; segments are in text order.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct SourceMap {
    /// Segments.
    pub segs: Vec<Seg>,
}

impl SourceMap {
    /// Adds a byte-identical segment.
    pub fn text(&mut self, text: Range<usize>, src: Range<usize>) {
        if let Some(last) = self.segs.last_mut()
            && !last.atomic
            && last.text.end == text.start
            && last.src.end == src.start
        {
            last.text.end = text.end;
            last.src.end = src.end;
            return;
        }
        self.segs.push(Seg {
            text,
            src,
            atomic: false,
        });
    }

    /// Adds an atomic segment.
    pub fn atomic(&mut self, text: Range<usize>, src: Range<usize>) {
        self.segs.push(Seg {
            text,
            src,
            atomic: true,
        });
    }

    /// Text byte for relative source offset `src`, if the position is displayed.
    /// At a boundary shared by two segments the earlier one wins unless `after`.
    pub fn to_text(&self, src: usize, after: bool) -> Option<usize> {
        let mut found = None;
        for s in &self.segs {
            if src >= s.src.start && src <= s.src.end {
                let t = if s.atomic {
                    if src == s.src.start {
                        s.text.start
                    } else if src == s.src.end {
                        s.text.end
                    } else {
                        continue;
                    }
                } else {
                    s.text.start + (src - s.src.start)
                };
                if !after {
                    return Some(t);
                }
                found = Some(t);
            }
        }
        found
    }

    /// Relative source offset for text byte `t` (clamped into a segment).
    pub fn to_src(&self, t: usize) -> usize {
        let mut best: Option<usize> = None;
        for s in &self.segs {
            if t >= s.text.start && t <= s.text.end {
                if s.atomic {
                    let mid = (s.text.start + s.text.end) / 2;
                    return if t <= mid { s.src.start } else { s.src.end };
                }
                return s.src.start + (t - s.text.start);
            }
            if t > s.text.end {
                best = Some(s.src.end);
            } else if best.is_none() {
                return s.src.start;
            }
        }
        best.unwrap_or(0)
    }

    /// Text byte of the first displayed position at or after `src`.
    pub fn to_text_ge(&self, src: usize) -> Option<usize> {
        if let Some(t) = self.to_text(src, true) {
            return Some(t);
        }
        self.segs
            .iter()
            .filter(|s| s.src.start >= src)
            .min_by_key(|s| s.src.start)
            .map(|s| s.text.start)
    }

    /// Text byte of the last displayed position at or before `src`.
    pub fn to_text_le(&self, src: usize) -> Option<usize> {
        if let Some(t) = self.to_text(src, false) {
            return Some(t);
        }
        self.segs
            .iter()
            .filter(|s| s.src.end <= src)
            .max_by_key(|s| s.src.end)
            .map(|s| s.text.end)
    }

    /// Whether relative source offset `src` is displayed (a caret can sit there).
    pub fn shows(&self, src: usize) -> bool {
        self.to_text(src, false).is_some()
    }

    /// Smallest and largest mapped relative source offsets.
    pub fn src_bounds(&self) -> Option<Range<usize>> {
        let a = self.segs.iter().map(|s| s.src.start).min()?;
        let b = self.segs.iter().map(|s| s.src.end).max()?;
        Some(a..b)
    }
}

/// A text run inside a block layout.
#[derive(Debug, Clone, PartialEq)]
pub struct RunSpec {
    /// The job to lay out.
    pub job: LayoutJob,
    /// Top-left position relative to the block origin.
    pub pos: Vec2,
    /// Source mapping.
    pub map: SourceMap,
}

/// Char index of byte `b` in `s`.
pub fn char_index(s: &str, b: usize) -> usize {
    let mut b = b.min(s.len());
    while !s.is_char_boundary(b) {
        b -= 1;
    }
    s[..b].chars().count()
}

/// Byte offset of char index `c` in `s`.
pub fn byte_index(s: &str, c: usize) -> usize {
    s.char_indices().nth(c).map_or(s.len(), |(b, _)| b)
}

/// A run materialized for this frame.
#[derive(Debug, Clone)]
pub struct Run<'a> {
    /// The spec.
    pub spec: &'a RunSpec,
    /// The laid-out galley.
    pub galley: Arc<Galley>,
    /// Absolute top-left in document coordinates.
    pub origin: Pos2,
}

impl Run<'_> {
    fn text(&self) -> &str {
        &self.galley.job.text
    }

    /// Caret rectangle (document coordinates) for text byte `t`.
    pub fn caret_at_text(&self, t: usize, prefer_next_row: bool) -> Rect {
        let idx = char_index(self.text(), t);
        let mut c = CCursor::new(idx);
        c.prefer_next_row = prefer_next_row;
        self.galley
            .pos_from_cursor(c)
            .translate(self.origin.to_vec2())
    }

    /// Text byte closest to document position `p`.
    pub fn hit(&self, p: Pos2) -> usize {
        let c = self.galley.cursor_from_pos(p - self.origin);
        byte_index(self.text(), c.index.0)
    }

    /// Vertical distance of `p` to the run's rows (0 when inside).
    pub fn distance(&self, p: Pos2) -> f32 {
        let r = self.rect();
        if p.y < r.min.y {
            r.min.y - p.y
        } else if p.y > r.max.y {
            p.y - r.max.y
        } else {
            0.0
        }
    }

    /// Bounding rectangle in document coordinates.
    pub fn rect(&self) -> Rect {
        Rect::from_min_size(self.origin, self.galley.size())
    }

    /// Selection rectangles for the text byte range `t` (document coordinates).
    pub fn selection_rects(&self, t: Range<usize>, out: &mut Vec<Rect>) {
        let text = self.text();
        let a = char_index(text, t.start);
        let b = char_index(text, t.end);
        if a >= b {
            return;
        }
        let mut row_start = 0usize;
        for row in &self.galley.rows {
            let n = row.char_count_including_newline().0;
            let row_end = row_start + n;
            let s = a.max(row_start);
            let e = b.min(row_end);
            if s < e || (s == e && a < row_start && b > row_end) {
                let x0 = row.row.x_offset(epaint::text::CharIndex(s - row_start));
                let mut x1 = row.row.x_offset(epaint::text::CharIndex(
                    (e - row_start).min(row.row.char_count_excluding_newline().0),
                ));
                if e >= row_end && row.ends_with_newline {
                    x1 += 4.0; // show the selected line break
                }
                let r = Rect::from_min_max(
                    pos2(row.pos.x + x0, row.min_y()),
                    pos2(row.pos.x + x1.max(x0 + 1.0), row.max_y()),
                );
                out.push(r.translate(self.origin.to_vec2()));
            }
            row_start = row_end;
        }
    }

    /// Text byte at the start / end of the visual row containing text byte `t`.
    pub fn row_bounds(&self, t: usize) -> (usize, usize) {
        let text = self.text();
        let idx = char_index(text, t);
        let mut row_start = 0usize;
        for row in &self.galley.rows {
            let n = row.char_count_including_newline().0;
            let excl = row.row.char_count_excluding_newline().0;
            if idx < row_start + n || std::ptr::eq(row, self.galley.rows.last().unwrap_or(row)) {
                return (
                    byte_index(text, row_start),
                    byte_index(text, row_start + excl),
                );
            }
            row_start += n;
        }
        (0, text.len())
    }
}

/// Size helper.
pub fn size_of(g: &Galley) -> Vec2 {
    vec2(g.size().x, g.size().y)
}
