//! Incremental re-parsing (AD-1 b).
//!
//! After an edit, parsing restarts at the last *fresh* top-level block (one whose
//! first line began with only the document open) that starts before the edited
//! line, and continues until a new top-level block begins, on a line lying
//! entirely after the edit, exactly where an old block began (shifted by the edit
//! delta). From that point the old blocks are reused. Because a top-level block
//! start is a point where only the document container is open, the parse from
//! there on is identical to the old one.
//!
//! Reused blocks are not rewritten: each top-level block keeps the offsets it was
//! parsed with plus a pending `delta`, so an edit costs O(top-level blocks) integer
//! additions instead of rewriting every node. Read blocks through [`BlockRef`].

use std::ops::Range;
use std::time::Instant;

use crate::ast::{Block, BlockKind, Span};
use crate::inline::InlineContext;
use crate::options::ParseOptions;
use crate::{Source, collect_context, parse_all_inlines, parse_blocks_from};

/// Statistics about one incremental update.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EditStats {
    /// Top-level blocks produced by the re-parse.
    pub reparsed: usize,
    /// Old top-level blocks reused after re-synchronization.
    pub reused: usize,
    /// Bytes covered by the re-parse.
    pub bytes: usize,
    /// Whether all inline content was re-parsed (definitions changed).
    pub all_inlines: bool,
    /// Microseconds spent.
    pub micros: u64,
}

/// A top-level block together with the offset delta to apply to every range in
/// it: `absolute = stored + delta`.
#[derive(Debug, Clone, Copy)]
pub struct BlockRef<'a> {
    /// The block with offsets as stored.
    pub block: &'a Block,
    /// Pending shift.
    pub delta: isize,
}

impl BlockRef<'_> {
    /// Absolute position of a stored offset.
    pub fn abs(&self, p: usize) -> usize {
        (p as isize + self.delta) as usize
    }

    /// Stored offset of an absolute position.
    pub fn rel(&self, p: usize) -> usize {
        (p as isize - self.delta).max(0) as usize
    }

    /// Absolute range of a stored range.
    pub fn abs_span(&self, s: &Span) -> Span {
        self.abs(s.start)..self.abs(s.end)
    }

    /// Absolute range of the block.
    pub fn span(&self) -> Span {
        self.abs_span(&self.block.span)
    }

    /// A copy of the block with absolute offsets.
    pub fn to_absolute(&self) -> Block {
        let mut b = self.block.clone();
        if self.delta != 0 {
            b.shift(self.delta);
        }
        b
    }
}

/// A parsed document that can be updated incrementally.
#[derive(Debug, Clone)]
pub struct Document {
    blocks: Vec<Block>,
    deltas: Vec<isize>,
    /// Per top-level block: valid restart point.
    fresh: Vec<bool>,
    /// Per top-level block: contains link/footnote definitions.
    defs: Vec<bool>,
    cx: InlineContext,
    options: ParseOptions,
}

impl PartialEq for Document {
    fn eq(&self, other: &Self) -> bool {
        self.options == other.options
            && self.cx == other.cx
            && self.to_blocks() == other.to_blocks()
    }
}

fn has_defs(b: &Block) -> bool {
    let mut found = false;
    b.walk(&mut |b| {
        if matches!(
            b.kind,
            BlockKind::LinkDef { .. } | BlockKind::FootnoteDef { .. }
        ) {
            found = true;
        }
    });
    found
}

impl Document {
    /// Parses `src` fully.
    pub fn parse<S: Source + ?Sized>(src: &S, options: ParseOptions) -> Self {
        let (mut blocks, fresh, _) = parse_blocks_from(src, &options, 0, &mut |_| false);
        let cx = collect_context(&blocks);
        parse_all_inlines(src, &mut blocks, &options, &cx);
        let defs = blocks.iter().map(has_defs).collect();
        let deltas = vec![0; blocks.len()];
        Self {
            blocks,
            deltas,
            fresh,
            defs,
            cx,
            options,
        }
    }

    /// Number of top-level blocks.
    pub fn len(&self) -> usize {
        self.blocks.len()
    }

    /// Whether the document has no blocks.
    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    /// Top-level block `i`.
    pub fn get(&self, i: usize) -> Option<BlockRef<'_>> {
        self.blocks.get(i).map(|block| BlockRef {
            block,
            delta: self.deltas[i],
        })
    }

    /// Iterates over the top-level blocks.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = BlockRef<'_>> + DoubleEndedIterator {
        self.blocks
            .iter()
            .zip(&self.deltas)
            .map(|(block, &delta)| BlockRef { block, delta })
    }

    /// Absolute start of top-level block `i`.
    pub fn start(&self, i: usize) -> usize {
        (self.blocks[i].span.start as isize + self.deltas[i]) as usize
    }

    /// Absolute end of top-level block `i`.
    pub fn end(&self, i: usize) -> usize {
        (self.blocks[i].span.end as isize + self.deltas[i]) as usize
    }

    /// Index of the first top-level block whose start is `>= pos`.
    pub fn first_starting_at_or_after(&self, pos: usize) -> usize {
        let (mut lo, mut hi) = (0, self.blocks.len());
        while lo < hi {
            let mid = (lo + hi) / 2;
            if self.start(mid) < pos {
                lo = mid + 1
            } else {
                hi = mid
            }
        }
        lo
    }

    /// Index of the top-level block containing `pos` (inclusive end), if any.
    pub fn index_at(&self, pos: usize) -> Option<usize> {
        let i = self.first_starting_at_or_after(pos + 1);
        (i > 0 && pos <= self.end(i - 1)).then(|| i - 1)
    }

    /// All blocks with absolute offsets (O(document); for export and tests).
    pub fn to_blocks(&self) -> Vec<Block> {
        self.iter().map(|r| r.to_absolute()).collect()
    }

    /// Reference definitions and footnote labels.
    pub fn context(&self) -> &InlineContext {
        &self.cx
    }

    /// Parse options.
    pub fn options(&self) -> &ParseOptions {
        &self.options
    }

    /// Changes options and re-parses `src` fully.
    pub fn set_options<S: Source + ?Sized>(&mut self, src: &S, options: ParseOptions) {
        *self = Self::parse(src, options);
    }

    /// Updates the AST after `old` (a byte range of the previous text) was replaced
    /// by `inserted_len` bytes. `src` is the text after the edit.
    pub fn edit<S: Source + ?Sized>(
        &mut self,
        src: &S,
        old: Range<usize>,
        inserted_len: usize,
    ) -> EditStats {
        let t0 = Instant::now();
        let delta = inserted_len as isize - (old.end - old.start) as isize;
        let new_edit_end = old.start + inserted_len;

        let edit_line_start = src.line_at(old.start.min(src.len())).start;
        let mut first = self.first_starting_at_or_after(edit_line_start);
        // Restart only where the old parse had nothing but the document open.
        while first > 0 && !self.fresh[first - 1] {
            first -= 1;
        }
        let (mut i0, mut start) = if first == 0 {
            (0, 0)
        } else {
            (first - 1, src.line_at(self.start(first - 1)).start)
        };
        // Front matter is only recognized at offset 0, and whether the first `---`
        // line opens it depends on a closing line anywhere below.
        if self.options.front_matter && starts_with_fence(src) {
            let keep = self.blocks.first().is_some_and(|b| {
                matches!(b.kind, BlockKind::FrontMatter { .. }) && old.start > self.end(0) + 1
            });
            if !keep {
                i0 = 0;
                start = 0;
            }
        }
        let resync_from = self.first_starting_at_or_after(old.end);

        let mut matched_old: Option<usize> = None;
        let this = &*self;
        let (mut new_blocks, mut new_fresh, stopped) =
            parse_blocks_from(src, &this.options, start, &mut |p| {
                let Some(x) = p.new_top_level() else {
                    return false;
                };
                // The block's whole line must lie strictly after the edit: a line that
                // starts exactly at the edit end may have been joined or split by it.
                if x < new_edit_end || src.line_at(x).start <= new_edit_end {
                    return false;
                }
                let old_x = x as isize - delta;
                if old_x < 0 {
                    return false;
                }
                let old_x = old_x as usize;
                let (mut lo, mut hi) = (resync_from, this.blocks.len());
                while lo < hi {
                    let mid = (lo + hi) / 2;
                    if this.start(mid) < old_x {
                        lo = mid + 1
                    } else {
                        hi = mid
                    }
                }
                // Only a fresh old block parsed independently of what preceded it
                // (a table split off a paragraph did not, for example).
                if lo < this.blocks.len() && this.start(lo) == old_x && this.fresh[lo] {
                    matched_old = Some(lo);
                    true
                } else {
                    false
                }
            });
        let reuse_from = if stopped {
            matched_old.unwrap_or(self.blocks.len())
        } else {
            self.blocks.len()
        };
        let reparsed = new_blocks.len();
        let reused = self.blocks.len() - reuse_from;
        let bytes = new_blocks
            .last()
            .map_or(0, |b| b.span.end.saturating_sub(start));

        let removed_defs = self.defs[i0..reuse_from].iter().any(|&d| d);
        let new_defs: Vec<bool> = new_blocks.iter().map(has_defs).collect();
        let defs_changed = removed_defs || new_defs.iter().any(|&d| d);

        for d in &mut self.deltas[reuse_from..] {
            *d += delta;
        }
        let n_new = new_blocks.len();
        self.blocks.splice(i0..reuse_from, new_blocks.drain(..));
        self.deltas
            .splice(i0..reuse_from, std::iter::repeat_n(0, n_new));
        self.fresh.splice(i0..reuse_from, new_fresh.drain(..));
        self.defs.splice(i0..reuse_from, new_defs);

        let mut all_inlines = false;
        if defs_changed {
            let cx = self.compute_context();
            if cx != self.cx {
                self.cx = cx;
                all_inlines = true;
            }
        }
        if all_inlines {
            // Definitions changed: every leaf may resolve links differently. Blocks
            // are parsed in absolute coordinates, so normalize them first.
            for i in 0..self.blocks.len() {
                if self.deltas[i] != 0 {
                    self.blocks[i].shift(self.deltas[i]);
                    self.deltas[i] = 0;
                }
            }
            parse_all_inlines(src, &mut self.blocks, &self.options, &self.cx);
        } else {
            parse_all_inlines(
                src,
                &mut self.blocks[i0..i0 + n_new],
                &self.options,
                &self.cx,
            );
        }
        EditStats {
            reparsed,
            reused,
            bytes,
            all_inlines,
            micros: t0.elapsed().as_micros() as u64,
        }
    }

    fn compute_context(&self) -> InlineContext {
        // Definitions carry no offsets, so stored coordinates are fine here.
        let with_defs: Vec<&Block> = self
            .blocks
            .iter()
            .zip(&self.defs)
            .filter(|(_, d)| **d)
            .map(|(b, _)| b)
            .collect();
        let mut cx = InlineContext::default();
        for b in with_defs {
            let part = collect_context(std::slice::from_ref(b));
            for (k, v) in part.refs {
                cx.refs.entry(k).or_insert(v);
            }
            cx.footnotes.extend(part.footnotes);
        }
        cx
    }
}

fn starts_with_fence<S: Source + ?Sized>(src: &S) -> bool {
    let first = src.line_at(0);
    let line = src.slice(first);
    line.trim_end_matches(['\n', '\r'])
        .trim_end_matches([' ', '\t'])
        == "---"
}
