//! The virtualized view: a list of items (source lines, or live leaf blocks)
//! with cached layouts and heights, and the geometry queries the editor needs
//! (caret rectangles, hit testing, vertical movement, selection rectangles). AD-4:
//! only items near the viewport are laid out; others use estimated heights that
//! converge as they are laid out.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::sync::Arc;

use epaint::text::FontsView;
use epaint::{Pos2, Rect, pos2};
use inkra_doc::Doc;

use crate::block::BlockLayout;
use crate::hash::{FxBuild, FxHasher, hash_str};
use crate::live::leaf::{ImageSizes, LiveCx, NoImages};
use crate::live::{self, LeafKind, LiveItem};
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
    /// Absolute source range (line content or leaf span).
    pub src: Range<usize>,
    /// Hash of the item's text (height reuse across rebuilds).
    pub text_hash: u64,
    /// Current height (laid out or estimated).
    pub height: f32,
    /// Whether `height` comes from a real layout.
    pub measured: bool,
}

/// Everything layout queries need besides fonts.
pub struct Ctx<'a> {
    /// Document.
    pub doc: &'a Doc,
    /// Style.
    pub style: &'a Style,
    /// Caret (live mode reveals markers around it).
    pub caret: usize,
    /// Known image sizes.
    pub images: &'a dyn ImageSizes,
    /// syntect theme name.
    pub syntax_theme: &'a str,
}

impl<'a> Ctx<'a> {
    /// A context without images, using the default syntax theme.
    pub fn simple(doc: &'a Doc, style: &'a Style, caret: usize) -> Self {
        Self {
            doc,
            style,
            caret,
            images: &NoImages,
            syntax_theme: "InspiredGitHub",
        }
    }
}

struct Laid {
    key: u64,
    layout: Arc<BlockLayout>,
    galleys: Vec<Arc<epaint::Galley>>,
}

/// The view layout.
pub struct ViewLayout {
    items: Vec<Item>,
    live: Vec<LiveItem>,
    spacing: Vec<(f32, usize)>,
    virtual_at: Option<usize>,
    tops: Vec<f32>,
    tops_dirty: bool,
    mode: Mode,
    width: f32,
    doc_version: Option<u64>,
    style_version: u64,
    cache: HashMap<u64, Arc<BlockLayout>, FxBuild>,
    frame: HashMap<usize, Laid, FxBuild>,
    /// Sum of height changes of items above the viewport since last taken.
    pub height_shift_above: f32,
    anchor_limit: f32,
    math_numbers: HashMap<usize, u32>,
    math_refs: Vec<(String, String)>,
    doc_info: live::DocInfo,
}

impl Default for ViewLayout {
    fn default() -> Self {
        Self::new()
    }
}

impl ViewLayout {
    /// An empty view.
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            live: Vec::new(),
            spacing: Vec::new(),
            virtual_at: None,
            tops: Vec::new(),
            tops_dirty: true,
            mode: Mode::Source { wrap: true },
            width: 0.0,
            doc_version: None,
            style_version: 0,
            cache: HashMap::default(),
            frame: HashMap::default(),
            height_shift_above: 0.0,
            math_numbers: HashMap::new(),
            math_refs: Vec::new(),
            doc_info: live::DocInfo::default(),
            anchor_limit: 0.0,
        }
    }

    /// Items.
    pub fn items(&self) -> &[Item] {
        &self.items
    }

    /// Live items (live mode only).
    pub fn live_items(&self) -> &[LiveItem] {
        &self.live
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

    /// Assigns equation numbers to display math blocks (MD-008): `1` numbers
    /// AMS environments (`equation`, `align`, …), `2` every block; `\tag`s and
    /// `\label`s feed `\ref`/`\eqref`.
    fn number_equations(&mut self, doc: &Doc, mode: u8) {
        self.math_numbers.clear();
        self.math_refs.clear();
        let mut n = 0u32;
        let text = doc.text();
        for it in &self.live {
            let Some((b, delta)) = it.block(doc) else {
                continue;
            };
            let inkra_markdown::BlockKind::MathBlock { open, close } = &b.kind else {
                continue;
            };
            let abs = |r: &std::ops::Range<usize>| {
                (r.start as isize + delta) as usize..(r.end as isize + delta) as usize
            };
            let stop = close.as_ref().map_or(it.src.end, |c| abs(c).start);
            let start = abs(open).end;
            if stop <= start {
                continue;
            }
            let tex = text.slice(start..stop).into_owned();
            if mode == 0 && !tex.contains("\\label") {
                continue;
            }
            let p = inkra_math::parse::parse(&tex);
            let numbered =
                !p.notag && p.tag.is_none() && (mode == 2 || mode == 1 && p.numbered_env);
            let shown = if let Some((t, _)) = &p.tag {
                Some(t.clone())
            } else if numbered {
                n += 1;
                self.math_numbers.insert(it.src.start, n);
                Some(n.to_string())
            } else {
                None
            };
            if let Some(s) = shown {
                for l in &p.labels {
                    self.math_refs.push((l.clone(), s.clone()));
                }
            }
        }
    }

    /// Brings the item list up to date with the document, mode, width, style and
    /// (live mode) the caret.
    pub fn sync(&mut self, doc: &Doc, mode: Mode, width: f32, style: &Style, caret: usize) {
        let same_geometry =
            self.mode == mode && self.width == width && self.style_version == style.version;
        let doc_same = self.doc_version == Some(doc.version());
        match mode {
            Mode::Source { .. } => {
                if same_geometry && doc_same {
                    return;
                }
                if same_geometry
                    && !self.items.is_empty()
                    && self.live.is_empty()
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
                self.live.clear();
                self.spacing.clear();
                let lines = 0..doc.text().len_lines();
                self.items = self.line_items(doc, lines, style, &HashMap::default());
                self.tops_dirty = true;
            }
            Mode::Live => {
                let geometry_or_doc_changed =
                    !doc_same || !same_geometry || self.doc_version.is_none();
                if !geometry_or_doc_changed && self.virtual_position(doc, caret) == self.virtual_at
                {
                    return;
                }
                let mut old: HashMap<u64, f32, FxBuild> = HashMap::default();
                if same_geometry {
                    for it in &self.items {
                        if it.measured {
                            old.insert(it.text_hash, it.height);
                        }
                    }
                }
                if geometry_or_doc_changed {
                    self.live = live::flatten(doc);
                    self.number_equations(doc, style.math_numbering);
                    self.doc_info = live::DocInfo::compute(doc, &self.live);
                } else {
                    // Only the virtual empty line moved.
                    self.live.retain(|it| it.top != usize::MAX);
                }
                self.mode = mode;
                self.width = width;
                self.style_version = style.version;
                self.doc_version = Some(doc.version());
                let virtual_at = self.virtual_position(doc, caret);
                self.virtual_at = virtual_at;
                if let Some(at) = virtual_at {
                    let i = self.live.partition_point(|it| it.src.start <= at);
                    self.live.insert(
                        i,
                        LiveItem {
                            src: at..at,
                            top: usize::MAX,
                            path: Vec::new(),
                            frames: live::frames_at(doc, at),
                            kind: LeafKind::EmptyLine,
                        },
                    );
                }
                self.frame.clear();
                self.spacing.clear();
                let text = doc.text();
                let mut items = Vec::with_capacity(self.live.len());
                for k in 0..self.live.len() {
                    let prev = k.checked_sub(1).map(|p| &self.live[p]);
                    let cur = &self.live[k];
                    let gap = live::gap(doc, prev, cur, style);
                    let quotes = live::shared_quotes(prev, cur);
                    self.spacing.push((gap, quotes));
                    let s = text.slice(cur.src.clone());
                    let mut h = FxHasher::default();
                    s.hash(&mut h);
                    cur.frames.hash(&mut h);
                    gap.to_bits().hash(&mut h);
                    let th = h.finish();
                    let (height, measured) = match old.get(&th) {
                        Some(&ht) => (ht, true),
                        None => {
                            let chars = s.chars().count() as f32;
                            let lines = ((chars * style.size * 0.55) / width.max(40.0))
                                .ceil()
                                .max(1.0);
                            (gap + lines * style.size * style.line_height, false)
                        }
                    };
                    items.push(Item {
                        src: cur.src.clone(),
                        text_hash: th,
                        height,
                        measured,
                    });
                }
                self.items = items;
                self.tops_dirty = true;
            }
        }
    }

    /// Where a virtual empty paragraph is needed: the caret is on a blank line that
    /// no leaf covers (after Enter, or in an empty document). `self.live` must be
    /// up to date (without a virtual item).
    fn virtual_position(&self, doc: &Doc, caret: usize) -> Option<usize> {
        let text = doc.text();
        let caret = caret.min(text.len());
        let i = self.live.partition_point(|it| it.src.start <= caret);
        let covered = self.live[i.saturating_sub(3)..(i + 1).min(self.live.len())]
            .iter()
            .any(|it| it.top != usize::MAX && it.src.start <= caret && caret <= it.src.end);
        if covered {
            return None;
        }
        let line = text.byte_to_line(caret);
        let r = text.line_content_range(line);
        let s = text.slice(r);
        let blank = s.trim_start_matches(['>', ' ', '\t']).trim().is_empty();
        blank.then_some(caret)
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
        // One extra line on each side: an edit next to a `\\r\\n` can change the
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

    fn key_and_spec(
        &self,
        i: usize,
        cx: &Ctx<'_>,
        fonts: &mut FontsView<'_>,
    ) -> (u64, Arc<BlockLayout>) {
        let it = &self.items[i];
        match self.mode {
            Mode::Source { wrap } => {
                let text = cx.doc.text().slice(it.src.clone());
                let tokens = source::line_tokens(cx.doc, it.src.clone(), &text);
                let key = source::line_key(&text, &tokens, self.width, wrap, cx.style);
                if let Some(l) = self.cache.get(&key) {
                    return (key, l.clone());
                }
                (
                    key,
                    Arc::new(source::layout_line(
                        &text, &tokens, self.width, wrap, cx.style,
                    )),
                )
            }
            Mode::Live => {
                let item = &self.live[i];
                let (gap, quotes) = self.spacing.get(i).copied().unwrap_or((0.0, 0));
                let key = self.live_key(i, item, gap, quotes, cx);
                if let Some(l) = self.cache.get(&key) {
                    return (key, l.clone());
                }
                let lcx = LiveCx {
                    doc: cx.doc,
                    style: cx.style,
                    width: self.width,
                    caret: cx.caret,
                    space_before: gap,
                    quote_continues: quotes,
                    images: cx.images,
                    syntax_theme: cx.syntax_theme,
                    math_numbers: &self.math_numbers,
                    math_refs: &self.math_refs,
                    doc_info: &self.doc_info,
                };
                (key, Arc::new(live::leaf::layout(item, &lcx, fonts)))
            }
        }
    }

    fn live_key(&self, i: usize, item: &LiveItem, gap: f32, quotes: usize, cx: &Ctx<'_>) -> u64 {
        let mut h = FxHasher::default();
        self.items[i].text_hash.hash(&mut h);
        item.kind.hash(&mut h);
        quotes.hash(&mut h);
        gap.to_bits().hash(&mut h);
        self.width.to_bits().hash(&mut h);
        cx.style.version.hash(&mut h);
        cx.syntax_theme.hash(&mut h);
        if let Some((b, delta)) = item.block(cx.doc) {
            use inkra_markdown::BlockKind;
            if matches!(b.kind, BlockKind::CodeBlock { .. }) {
                inkra_highlight::generation().hash(&mut h);
            }
            // Images: their known size changes the layout.
            for inl in &b.inlines {
                inl.walk(&mut |n| {
                    if let inkra_markdown::InlineKind::Image { dest, .. } = &n.kind {
                        dest.hash(&mut h);
                        cx.images
                            .size(dest)
                            .map(|s| (s[0].to_bits(), s[1].to_bits()))
                            .hash(&mut h);
                    }
                });
            }
            if matches!(b.kind, inkra_markdown::BlockKind::MathBlock { .. }) {
                self.math_numbers.get(&item.src.start).hash(&mut h);
            }
            // Diagrams rendered by `mmdc` appear when the job and the image load finish.
            if inkra_diagram::external::enabled()
                && matches!(b.kind, inkra_markdown::BlockKind::CodeBlock { .. })
            {
                inkra_diagram::external::generation().hash(&mut h);
                let src = cx.doc.text().slice(item.src.clone());
                if src
                    .trim_start_matches(['`', '~'])
                    .trim_start()
                    .starts_with("mermaid")
                {
                    for p in inkra_diagram::external::ready_paths() {
                        cx.images
                            .size(&p.to_string_lossy())
                            .map(|v| (v[0].to_bits(), v[1].to_bits()))
                            .hash(&mut h);
                    }
                }
            }
            // Images inside HTML blocks change the layout once loaded.
            if matches!(b.kind, inkra_markdown::BlockKind::HtmlBlock { .. }) {
                let src = cx.doc.text().slice(item.src.clone());
                let mut rest: &str = &src;
                while let Some(i) = rest.find("<img") {
                    rest = &rest[i + 4..];
                    let end = rest.find('>').unwrap_or(rest.len());
                    if let Some(s) = inkra_html::render::tag_attr(&rest[..end], "src") {
                        cx.images
                            .size(&s)
                            .map(|v| (v[0].to_bits(), v[1].to_bits()))
                            .hash(&mut h);
                    }
                }
            }
            // References can appear in any inline formula; footnote numbers and
            // the headings of `[TOC]` in any item.
            if !self.math_refs.is_empty() {
                self.math_refs.hash(&mut h);
            }
            if !self.doc_info.footnotes.is_empty() {
                self.doc_info.footnotes.hash(&mut h);
            }
            if matches!(b.kind, inkra_markdown::BlockKind::Toc) {
                self.doc_info.headings.hash(&mut h);
            }
            if cx.style.spellcheck {
                inkra_text::spell::generation().hash(&mut h);
            }
            if matches!(b.kind, inkra_markdown::BlockKind::Table { .. })
                && let Some(ws) = cx.images.column_widths(item.src.start)
            {
                for w in ws {
                    w.to_bits().hash(&mut h);
                }
            }
            let active = cx.caret >= item.src.start && cx.caret <= item.src.end;
            active.hash(&mut h);
            if active {
                let mut path = Vec::new();
                live::inline::active_path(&b.inlines, delta, cx.caret, &mut path);
                for c in &b.children {
                    live::inline::active_path(&c.inlines, delta, cx.caret, &mut path);
                    for g in &c.children {
                        live::inline::active_path(&g.inlines, delta, cx.caret, &mut path);
                    }
                }
                for r in path {
                    (r.start - item.src.start, r.end - item.src.start).hash(&mut h);
                }
            }
        }
        // Alert titles and task markers depend on the caret too.
        for f in &item.frames {
            if let live::Frame::Quote {
                alert: Some((_, m)),
                first: true,
            } = f
            {
                (cx.caret >= m.start && cx.caret <= m.end).hash(&mut h);
            }
        }
        h.finish()
    }

    /// Lays out item `i` (if needed) for this frame.
    pub fn ensure(&mut self, i: usize, cx: &Ctx<'_>, fonts: &mut FontsView<'_>) -> bool {
        if i >= self.items.len() {
            return false;
        }
        if self.frame.contains_key(&i) {
            return true;
        }
        let (key, layout) = self.key_and_spec(i, cx, fonts);
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
        for d in &layout.decos {
            if let crate::Deco::Rect { rect, .. } | crate::Deco::Image { rect, .. } = d {
                h = h.max(rect.max.y);
            }
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

    /// Whether the caret can be displayed at `pos`.
    pub fn shows(&mut self, pos: usize, cx: &Ctx<'_>, fonts: &mut FontsView<'_>) -> bool {
        if self.items.is_empty() {
            return true;
        }
        let i = self.item_of(pos);
        let it = self.items[i].clone();
        if pos < it.src.start || pos > it.src.end {
            return false;
        }
        self.ensure(i, cx, fonts);
        let rel = pos - it.src.start;
        let Some(l) = self.layout(i) else {
            return false;
        };
        if l.caret_range
            .as_ref()
            .is_none_or(|r| rel < r.start || rel > r.end)
        {
            return false;
        }
        l.runs.iter().any(|r| r.map.shows(rel))
    }

    /// Moves `pos` to the nearest position where the caret can be displayed, in
    /// direction `forward` (live mode: skips hidden markers, gaps and rules).
    pub fn snap(
        &mut self,
        pos: usize,
        forward: bool,
        cx: &Ctx<'_>,
        fonts: &mut FontsView<'_>,
    ) -> usize {
        if self.mode != Mode::Live || self.items.is_empty() || self.shows(pos, cx, fonts) {
            return pos;
        }
        let n = self.items.len();
        let start = self.item_of(pos);
        let order: Vec<usize> = if forward {
            (start..n).collect()
        } else {
            (0..=start).rev().collect()
        };
        for i in order {
            self.ensure(i, cx, fonts);
            let base = self.items[i].src.start;
            let Some(l) = self.layout(i) else { continue };
            let Some(cr) = l.caret_range.clone() else {
                continue;
            };
            let mut best: Option<usize> = None;
            for r in &l.runs {
                for s in &r.map.segs {
                    // Only the part of the segment inside the caret range counts.
                    let (a, b) = (
                        (base + s.src.start).max(base + cr.start),
                        (base + s.src.end).min(base + cr.end),
                    );
                    if a > b {
                        continue;
                    }
                    let cand = if forward {
                        if b >= pos {
                            Some(a.max(pos).min(b))
                        } else {
                            None
                        }
                    } else if a <= pos {
                        Some(b.min(pos).max(a))
                    } else {
                        None
                    };
                    if let Some(c) = cand
                        && c >= base + cr.start
                        && c <= base + cr.end
                    {
                        best = Some(match best {
                            None => c,
                            Some(x) => {
                                if forward {
                                    x.min(c)
                                } else {
                                    x.max(c)
                                }
                            }
                        });
                    }
                }
            }
            if let Some(b) = best {
                return b;
            }
        }
        // Nothing in that direction: try the other one.
        if forward {
            self.snap_back(pos, cx, fonts)
        } else {
            pos
        }
    }

    fn snap_back(&mut self, pos: usize, cx: &Ctx<'_>, fonts: &mut FontsView<'_>) -> usize {
        let start = self.item_of(pos);
        for i in (0..=start).rev() {
            self.ensure(i, cx, fonts);
            let base = self.items[i].src.start;
            if let Some(l) = self.layout(i)
                && let Some(cr) = &l.caret_range
            {
                return base + cr.end;
            }
        }
        pos
    }

    /// Caret rectangle for source position `pos` (document coordinates).
    pub fn caret_rect(
        &mut self,
        pos: usize,
        cx: &Ctx<'_>,
        fonts: &mut FontsView<'_>,
    ) -> Option<Rect> {
        if self.items.is_empty() {
            return None;
        }
        let i = self.item_of(pos);
        self.ensure(i, cx, fonts);
        let start = self.items[i].src.start;
        let rel = pos.saturating_sub(start);
        let runs = self.runs(i);
        for run in runs.iter().filter(|r| !r.spec.map.segs.is_empty()) {
            if let Some(t) = run.spec.map.to_text(rel, false) {
                return Some(run.caret_at_text(t, false));
            }
        }
        // Not displayed: nearest displayed position before it.
        for run in runs.iter().rev().filter(|r| !r.spec.map.segs.is_empty()) {
            if let Some(t) = run.spec.map.to_text_le(rel) {
                return Some(run.caret_at_text(t, false));
            }
        }
        runs.iter()
            .find(|r| !r.spec.map.segs.is_empty())
            .map(|r| r.caret_at_text(0, false))
    }

    /// Source position closest to document point `p`.
    pub fn hit(&mut self, p: Pos2, cx: &Ctx<'_>, fonts: &mut FontsView<'_>) -> usize {
        if self.items.is_empty() {
            return 0;
        }
        let total = self.total_height();
        let p = if p.y < 0.0 { pos2(p.x, 0.0) } else { p };
        if p.y >= total {
            let last = self.items.len() - 1;
            self.ensure(last, cx, fonts);
            let base = self.items[last].src.start;
            return match self.layout(last).and_then(|l| l.caret_range.clone()) {
                Some(r) => base + r.end,
                None => self.items[last].src.end,
            };
        }
        let i = self.item_at_y(p.y);
        self.ensure(i, cx, fonts);
        let start = self.items[i].src.start;
        let runs = self.runs(i);
        let best = runs
            .iter()
            .filter(|r| !r.spec.map.segs.is_empty())
            .min_by(|a, b| {
                let da = a.distance(p);
                let db = b.distance(p);
                da.total_cmp(&db).then_with(|| {
                    let ha = (a.rect().center().x - p.x).abs();
                    let hb = (b.rect().center().x - p.x).abs();
                    ha.total_cmp(&hb)
                })
            })
            .map(|run| start + run.spec.map.to_src(run.hit(p)));
        match best {
            Some(b) => b,
            None => {
                // An item without text (a rule): snap to a neighbour.
                drop(runs);
                self.snap(start, true, cx, fonts)
            }
        }
    }

    /// The position one visual row above/below `pos`, at horizontal position `x`.
    pub fn vertical(
        &mut self,
        pos: usize,
        x: f32,
        down: bool,
        cx: &Ctx<'_>,
        fonts: &mut FontsView<'_>,
    ) -> usize {
        let Some(r) = self.caret_rect(pos, cx, fonts) else {
            return pos;
        };
        let mut y = if down { r.max.y + 1.0 } else { r.min.y - 1.0 };
        let total = self.total_height();
        for _ in 0..64 {
            if y < 0.0 {
                return if self.mode == Mode::Live {
                    self.snap(0, true, cx, fonts)
                } else {
                    0
                };
            }
            if y >= total {
                return if self.mode == Mode::Live {
                    self.hit(pos2(x, total + 1.0), cx, fonts)
                } else {
                    cx.doc.text().len()
                };
            }
            let hit = self.hit(pos2(x, y), cx, fonts);
            // Skip gaps between items: if the hit did not move to another row, step on.
            if let Some(nr) = self.caret_rect(hit, cx, fonts) {
                let moved = if down {
                    nr.min.y >= r.max.y - 0.5
                } else {
                    nr.max.y <= r.min.y + 0.5
                };
                if moved || hit != pos {
                    return hit;
                }
            }
            y += if down { 8.0 } else { -8.0 };
        }
        pos
    }

    /// Start and end of the visual row containing `pos`.
    pub fn row_bounds(
        &mut self,
        pos: usize,
        cx: &Ctx<'_>,
        fonts: &mut FontsView<'_>,
    ) -> (usize, usize) {
        if self.items.is_empty() {
            return (0, 0);
        }
        let i = self.item_of(pos);
        self.ensure(i, cx, fonts);
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
                if run.spec.map.segs.is_empty() {
                    continue;
                }
                let (Some(ta), Some(tb)) = (run.spec.map.to_text_ge(a), run.spec.map.to_text_le(b))
                else {
                    continue;
                };
                if ta > tb {
                    continue;
                }
                let before = out.len();
                run.selection_rects(ta..tb, &mut out);
                if crosses_end
                    && tb >= run.galley.job.text.len()
                    && let Some(last) = run.galley.rows.last()
                {
                    let x = run.origin.x + last.pos.x + last.row.size.x;
                    let y0 = run.origin.y + last.min_y();
                    let y1 = run.origin.y + last.max_y();
                    if out.len() == before || out.last().is_some_and(|r| r.max.x < x + 1.0) {
                        out.push(Rect::from_min_max(pos2(x, y0), pos2(x + 6.0, y1)));
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
