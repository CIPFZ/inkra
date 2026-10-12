//! The virtualized view: a list of items (source lines or blocks) with cached
//! layouts and heights, and the geometry queries the editor needs (caret
//! rectangles, hit testing, vertical movement, selection rectangles). AD-4: only
//! items near the viewport are laid out; others use estimated heights that
//! converge as they are laid out.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

use epaint::text::FontsView;
use epaint::{Pos2, Rect, pos2};
use inkra_doc::Doc;

use crate::block::BlockLayout;
use crate::run::Run;
use crate::source;
use crate::style::Style;

/// View mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mode {
    /// Source mode (ED-120/121).
    Source {
        /// Soft-wrap long lines.
        wrap: bool,
    },
    /// Live rendering (ED-001).
    Live,
}

/// One item of the view.
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    /// Absolute source range (line content or block span).
    pub src: Range<usize>,
    /// Hash of the item's text (height reuse across rebuilds).
    pub text_hash: u64,
    /// Current height (laid out or estimated).
    pub height: f32,
    /// Whether `height` comes from a real layout.
    pub measured: bool,
}

/// Per-frame state that influences layout.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct ViewState {
    /// Caret position (live mode reveals markers around it).
    pub caret: usize,
}

struct Laid {
    key: u64,
    layout: Arc<BlockLayout>,
    galleys: Vec<Arc<epaint::Galley>>,
}

/// The view layout.
pub struct ViewLayout {
    items: Vec<Item>,
    tops: Vec<f32>,
    tops_dirty: bool,
    mode: Mode,
    width: f32,
    doc_version: Option<u64>,
    style_version: u64,
    cache: HashMap<u64, Arc<BlockLayout>, FxBuild>,
    frame: HashMap<usize, Laid, FxBuild>,
    /// Sum of height changes of items above `anchor_limit` since last taken.
    pub height_shift_above: f32,
    anchor_limit: f32,
}

impl Default for ViewLayout {
    fn default() -> Self {
        Self::new()
    }
}

use crate::hash::{FxBuild, hash_str};

impl ViewLayout {
    /// An empty view.
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            tops: Vec::new(),
            tops_dirty: true,
            mode: Mode::Source { wrap: true },
            width: 0.0,
            doc_version: None,
            style_version: 0,
            cache: HashMap::default(),
            frame: HashMap::default(),
            height_shift_above: 0.0,
            anchor_limit: 0.0,
        }
    }

    /// Items.
    pub fn items(&self) -> &[Item] {
        &self.items
    }

    /// Current mode.
    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// Starts a new frame: drops materialized galleys (they are re-fetched from
    /// epaint's cache, which survives font atlas rebuilds correctly).
    pub fn begin_frame(&mut self, viewport_top: f32) {
        self.frame.clear();
        self.anchor_limit = viewport_top;
        if self.cache.len() > 20_000 {
            self.cache.clear();
        }
    }

    /// Rebuilds the item list if the document, mode, width or style changed.
    /// Text edits are applied incrementally: only lines in the changed region are
    /// re-derived (O(changed lines) plus an offset shift).
    pub fn sync(&mut self, doc: &Doc, mode: Mode, width: f32, style: &Style) {
        let same_geometry =
            self.mode == mode && self.width == width && self.style_version == style.version;
        if same_geometry && self.doc_version == Some(doc.version()) {
            return;
        }
        if same_geometry
            && !self.items.is_empty()
            && let Some((dirty, delta)) = self.doc_version.and_then(|v| doc.dirty_since(v))
        {
            self.apply_dirty(doc, dirty, delta, style);
            self.doc_version = Some(doc.version());
            self.frame.clear();
            return;
        }
        self.mode = mode;
        self.width = width;
        self.style_version = style.version;
        self.doc_version = Some(doc.version());
        self.frame.clear();
        let text = doc.text();
        let lines = 0..text.len_lines();
        self.items = self.line_items(doc, lines, style, &HashMap::default());
        self.tops_dirty = true;
    }

    fn estimate(&self, s: &str, style: &Style) -> f32 {
        let rh = source::row_height(style);
        let wrap = !matches!(self.mode, Mode::Source { wrap: false });
        let rows = if wrap {
            ((s.chars().count() as f32 * style.mono_size * 0.6) / self.width.max(40.0))
                .ceil()
                .max(1.0)
        } else {
            1.0
        };
        rows * rh
    }

    fn line_items(
        &self,
        doc: &Doc,
        lines: Range<usize>,
        style: &Style,
        old: &HashMap<u64, f32, FxBuild>,
    ) -> Vec<Item> {
        let text = doc.text();
        let mut items = Vec::with_capacity(lines.len());
        for line in lines {
            let r = text.line_content_range(line);
            let s = text.slice(r.clone());
            let h = hash_str(&s);
            let (height, measured) = match old.get(&h) {
                Some(&ht) => (ht, true),
                None => (self.estimate(&s, style), false),
            };
            items.push(Item {
                src: r,
                text_hash: h,
                height,
                measured,
            });
        }
        items
    }

    fn apply_dirty(&mut self, doc: &Doc, dirty: Range<usize>, delta: isize, style: &Style) {
        if dirty.is_empty() && delta == 0 {
            return;
        }
        let text = doc.text();
        let new_lines = text.len_lines();
        let old_dirty_end = (dirty.end as isize - delta).max(dirty.start as isize) as usize;
        // One extra line on each side: an edit next to a `\r\n` can change the
        // terminator (and thus the line structure) of its neighbours.
        let first_line = text
            .byte_to_line(dirty.start.min(text.len()))
            .saturating_sub(1);
        let new_last = (text.byte_to_line(dirty.end.min(text.len())) + 1).min(new_lines - 1);
        let old_len = self.items.len();
        let old_last = (self
            .items
            .partition_point(|it| it.src.start <= old_dirty_end)
            .saturating_sub(1)
            + 1)
        .min(old_len - 1);
        if first_line > old_last || first_line >= old_len {
            self.items = self.line_items(doc, 0..new_lines, style, &HashMap::default());
            self.tops_dirty = true;
            return;
        }
        let mut old: HashMap<u64, f32, FxBuild> = HashMap::default();
        for it in &self.items[first_line..=old_last] {
            if it.measured {
                old.insert(it.text_hash, it.height);
            }
        }
        let new_items = self.line_items(doc, first_line..new_last + 1, style, &old);
        for it in &mut self.items[old_last + 1..] {
            it.src.start = (it.src.start as isize + delta) as usize;
            it.src.end = (it.src.end as isize + delta) as usize;
        }
        self.items.splice(first_line..=old_last, new_items);
        self.tops_dirty = true;
        if self.items.len() != new_lines {
            // Structure disagreement (should not happen): rebuild.
            self.items = self.line_items(doc, 0..new_lines, style, &old);
        }
    }

    fn update_tops(&mut self) {
        if !self.tops_dirty {
            return;
        }
        self.tops.clear();
        self.tops.reserve(self.items.len() + 1);
        let mut y = 0.0;
        for it in &self.items {
            self.tops.push(y);
            y += it.height;
        }
        self.tops.push(y);
        self.tops_dirty = false;
    }

    /// Total content height.
    pub fn total_height(&mut self) -> f32 {
        self.update_tops();
        *self.tops.last().unwrap_or(&0.0)
    }

    /// Top of item `i`.
    pub fn top(&mut self, i: usize) -> f32 {
        self.update_tops();
        self.tops.get(i).copied().unwrap_or(0.0)
    }

    /// Item index at document y.
    pub fn item_at_y(&mut self, y: f32) -> usize {
        self.update_tops();
        if self.items.is_empty() {
            return 0;
        }
        let i = self.tops.partition_point(|&t| t <= y);
        i.saturating_sub(1).min(self.items.len() - 1)
    }

    /// Items overlapping the y range.
    pub fn visible(&mut self, y: Range<f32>) -> Range<usize> {
        if self.items.is_empty() {
            return 0..0;
        }
        let a = self.item_at_y(y.start);
        let b = self.item_at_y(y.end) + 1;
        a..b.min(self.items.len())
    }

    /// Index of the item containing source position `pos` (the last item whose
    /// start is `<= pos`).
    pub fn item_of(&self, pos: usize) -> usize {
        let i = self.items.partition_point(|it| it.src.start <= pos);
        i.saturating_sub(1)
    }

    fn key_and_spec(&self, i: usize, doc: &Doc, style: &Style) -> (u64, Arc<BlockLayout>) {
        let it = &self.items[i];
        let wrap = match self.mode {
            Mode::Source { wrap } => wrap,
            Mode::Live => true,
        };
        let text = doc.text().slice(it.src.clone());
        let tokens = source::line_tokens(doc, it.src.clone(), &text);
        let key = source::line_key(&text, &tokens, self.width, wrap, style);
        if let Some(l) = self.cache.get(&key) {
            return (key, l.clone());
        }
        (
            key,
            Arc::new(source::layout_line(&text, &tokens, self.width, wrap, style)),
        )
    }

    /// Lays out item `i` (if needed) for this frame and returns its runs.
    pub fn ensure(
        &mut self,
        i: usize,
        doc: &Doc,
        style: &Style,
        fonts: &mut FontsView<'_>,
    ) -> bool {
        if i >= self.items.len() {
            return false;
        }
        if self.frame.contains_key(&i) {
            return true;
        }
        let (key, layout) = self.key_and_spec(i, doc, style);
        self.cache.entry(key).or_insert_with(|| layout.clone());
        let galleys: Vec<Arc<epaint::Galley>> = layout
            .runs
            .iter()
            .map(|r| fonts.layout_job(r.job.clone()))
            .collect();
        let mut h = layout.height;
        for (g, r) in galleys.iter().zip(&layout.runs) {
            h = h.max(r.pos.y + g.size().y);
        }
        if (self.items[i].height - h).abs() > 0.01 {
            self.note_height_change(i, h);
        }
        let it = &mut self.items[i];
        it.height = h;
        it.measured = true;
        self.frame.insert(
            i,
            Laid {
                key,
                layout,
                galleys,
            },
        );
        true
    }

    fn note_height_change(&mut self, i: usize, new_h: f32) {
        self.update_tops();
        let old_h = self.items[i].height;
        if self.tops[i] + old_h <= self.anchor_limit {
            self.height_shift_above += new_h - old_h;
        }
        self.tops_dirty = true;
    }

    /// Takes the accumulated height change above the viewport (for scroll anchoring).
    pub fn take_shift_above(&mut self) -> f32 {
        std::mem::take(&mut self.height_shift_above)
    }

    /// Materialized runs of item `i` in document coordinates (call `ensure` first).
    pub fn runs(&mut self, i: usize) -> Vec<Run<'_>> {
        self.update_tops();
        let top = self.tops.get(i).copied().unwrap_or(0.0);
        let Some(l) = self.frame.get(&i) else {
            return Vec::new();
        };
        l.layout
            .runs
            .iter()
            .zip(&l.galleys)
            .map(|(spec, g)| Run {
                spec,
                galley: g.clone(),
                origin: pos2(spec.pos.x, top + spec.pos.y),
            })
            .collect()
    }

    /// Layout of item `i` for this frame.
    pub fn layout(&self, i: usize) -> Option<Arc<BlockLayout>> {
        self.frame.get(&i).map(|l| l.layout.clone())
    }

    /// Key of item `i` for this frame.
    pub fn key(&self, i: usize) -> Option<u64> {
        self.frame.get(&i).map(|l| l.key)
    }

    /// Caret rectangle for source position `pos` (document coordinates).
    pub fn caret_rect(
        &mut self,
        pos: usize,
        doc: &Doc,
        style: &Style,
        fonts: &mut FontsView<'_>,
    ) -> Option<Rect> {
        if self.items.is_empty() {
            return None;
        }
        let i = self.item_of(pos);
        self.ensure(i, doc, style, fonts);
        let start = self.items[i].src.start;
        let rel = pos.saturating_sub(start);
        let runs = self.runs(i);
        for run in &runs {
            if let Some(t) = run.spec.map.to_text(rel, false) {
                return Some(run.caret_at_text(t, false));
            }
        }
        runs.first().map(|r| r.caret_at_text(0, false))
    }

    /// Source position closest to document point `p`.
    pub fn hit(&mut self, p: Pos2, doc: &Doc, style: &Style, fonts: &mut FontsView<'_>) -> usize {
        if self.items.is_empty() {
            return 0;
        }
        let total = self.total_height();
        if p.y < 0.0 {
            return 0;
        }
        if p.y >= total {
            return self.items.last().map_or(0, |it| it.src.end);
        }
        let i = self.item_at_y(p.y);
        self.ensure(i, doc, style, fonts);
        let start = self.items[i].src.start;
        let runs = self.runs(i);
        let best = runs
            .iter()
            .min_by(|a, b| a.distance(p).total_cmp(&b.distance(p)))
            .map(|run| start + run.spec.map.to_src(run.hit(p)));
        best.unwrap_or(start)
    }

    /// The position one visual row above/below `pos`, at horizontal position `x`.
    pub fn vertical(
        &mut self,
        pos: usize,
        x: f32,
        down: bool,
        doc: &Doc,
        style: &Style,
        fonts: &mut FontsView<'_>,
    ) -> usize {
        let Some(r) = self.caret_rect(pos, doc, style, fonts) else {
            return pos;
        };
        let y = if down { r.max.y + 1.0 } else { r.min.y - 1.0 };
        if y < 0.0 {
            return 0;
        }
        let total = self.total_height();
        if y >= total {
            return doc.text().len();
        }
        self.hit(pos2(x, y), doc, style, fonts)
    }

    /// Start and end of the visual row containing `pos`.
    pub fn row_bounds(
        &mut self,
        pos: usize,
        doc: &Doc,
        style: &Style,
        fonts: &mut FontsView<'_>,
    ) -> (usize, usize) {
        if self.items.is_empty() {
            return (0, 0);
        }
        let i = self.item_of(pos);
        self.ensure(i, doc, style, fonts);
        let start = self.items[i].src.start;
        let rel = pos.saturating_sub(start);
        for run in self.runs(i) {
            if let Some(t) = run.spec.map.to_text(rel, false) {
                let (a, b) = run.row_bounds(t);
                return (
                    start + run.spec.map.to_src(a),
                    start + run.spec.map.to_src(b),
                );
            }
        }
        let it = &self.items[i];
        (it.src.start, it.src.end)
    }

    /// Selection rectangles of `range` within the visible items.
    pub fn selection_rects(&mut self, range: Range<usize>, visible: Range<usize>) -> Vec<Rect> {
        let mut out = Vec::new();
        for i in visible {
            let it = self.items[i].clone();
            if it.src.end < range.start || it.src.start > range.end {
                continue;
            }
            let a = range.start.max(it.src.start) - it.src.start;
            let b = range.end.min(it.src.end) - it.src.start;
            let crosses_end = range.end > it.src.end;
            for run in self.runs(i) {
                let (Some(ta), Some(tb)) = (
                    run.spec.map.to_text(a, true),
                    run.spec.map.to_text(b, false),
                ) else {
                    continue;
                };
                let before = out.len();
                run.selection_rects(ta..tb, &mut out);
                if crosses_end {
                    // Show the selected line break.
                    if let Some(last) = run.galley.rows.last() {
                        let x = run.origin.x + last.pos.x + last.row.size.x;
                        let y0 = run.origin.y + last.min_y();
                        let y1 = run.origin.y + last.max_y();
                        if out.len() == before || out.last().is_some_and(|r| r.max.x < x + 1.0) {
                            out.push(Rect::from_min_max(pos2(x, y0), pos2(x + 6.0, y1)));
                        }
                    }
                }
            }
        }
        out
    }

    /// The column width.
    pub fn width(&self) -> f32 {
        self.width
    }
}
