//! Inline content → layout job with source mapping (ED-002, ED-004).
//!
//! Markers (`**`, `[`, `](url)`, backslashes, …) are displayed only for inline
//! nodes that contain the caret; otherwise they are hidden and have no mapping,
//! so the caret cannot stop inside them.

use std::ops::Range;

use epaint::text::{ByteIndex, FontId, LayoutJob, LayoutSection, TextFormat, TextWrapping};
use epaint::{Color32, Stroke};
use inkra_markdown::ast::{Inline, InlineKind, LinkSyntax};
use inkra_text::Text;

use crate::run::{InlineObject, MathScene, SourceMap};

/// Formula size relative to the surrounding text (STIX Two Math has a smaller
/// x-height than the body faces).
pub const MATH_SCALE: f32 = 1.12;

thread_local! {
    /// Label → number of the document being laid out (`\eqref` in inline math).
    static REFS: std::cell::RefCell<Vec<(String, String)>> = const { std::cell::RefCell::new(Vec::new()) };
}

thread_local! {
    static NOTES: std::cell::RefCell<Vec<(String, u32)>> = const { std::cell::RefCell::new(Vec::new()) };
}

thread_local! {
    static SPELL: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Enables spell-check underlines for text laid out on this thread.
pub fn set_spellcheck(on: bool) {
    SPELL.with(|s| s.set(on));
}

/// Sets the footnote numbers used while laying out on this thread.
pub fn set_footnotes(notes: &[(String, u32)]) {
    NOTES.with(|n| {
        let mut n = n.borrow_mut();
        if n.as_slice() != notes {
            *n = notes.to_vec();
        }
    });
}

/// Sets the equation references used by inline math laid out on this thread.
pub fn set_math_refs(refs: &[(String, String)]) {
    REFS.with(|r| {
        let mut r = r.borrow_mut();
        if r.as_slice() != refs {
            *r = refs.to_vec();
        }
    });
}

/// Typesets a formula (cached) as a paintable scene.
pub fn math_scene(req: &inkra_math::Request, color: Color32) -> MathScene {
    use std::hash::{Hash, Hasher};
    let r = inkra_math::render(req);
    let mut h = std::collections::hash_map::DefaultHasher::new();
    req.hash(&mut h);
    MathScene {
        scene: r.scene.clone(),
        key: h.finish(),
        color,
    }
}
use crate::style::Style;

/// Inline text format.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fmt {
    /// Monospace (code).
    pub mono: bool,
    /// Typeface when not monospace.
    pub kind: crate::fonts::FontKind,
    /// Bold.
    pub bold: bool,
    /// Italic.
    pub italic: bool,
    /// Font size.
    pub size: f32,
    /// Color.
    pub color: Color32,
    /// Background.
    pub bg: Color32,
    /// Strikethrough.
    pub strike: bool,
    /// Underline.
    pub underline: bool,
    /// Vertical alignment (superscript/subscript).
    pub valign: epaint::emath::Align,
    /// Line height in points.
    pub line_height: f32,
}

impl Fmt {
    /// Body text format of `style`.
    pub fn body(style: &Style) -> Self {
        Self {
            mono: false,
            kind: style.body_font,
            bold: false,
            italic: false,
            size: style.size,
            color: style.fg,
            bg: Color32::TRANSPARENT,
            strike: false,
            underline: false,
            valign: epaint::emath::Align::BOTTOM,
            line_height: (style.size * style.line_height).round(),
        }
    }

    fn text_format(&self) -> TextFormat {
        TextFormat {
            font_id: FontId::new(
                self.size,
                crate::fonts::family_of(
                    if self.mono {
                        crate::fonts::FontKind::Mono
                    } else {
                        self.kind
                    },
                    self.bold,
                    self.italic,
                ),
            ),
            color: self.color,
            background: self.bg,
            line_height: Some(self.line_height),
            strikethrough: if self.strike {
                Stroke::new(1.0, self.color)
            } else {
                Stroke::NONE
            },
            underline: if self.underline {
                Stroke::new(1.0, self.color)
            } else {
                Stroke::NONE
            },
            valign: self.valign,
            ..Default::default()
        }
    }
}

/// Builds one layout job.
pub struct Builder<'a> {
    text: &'a Text,
    /// Absolute offset that relative mappings are measured from.
    base: usize,
    delta: isize,
    caret: Option<usize>,
    style: &'a Style,
    /// The job.
    pub job: LayoutJob,
    /// The mapping.
    pub map: SourceMap,
}

impl<'a> Builder<'a> {
    /// A builder whose mapping is relative to `base`; `delta` converts stored AST
    /// offsets to absolute ones; `caret` (absolute) reveals markers around it.
    pub fn new(
        text: &'a Text,
        base: usize,
        delta: isize,
        caret: Option<usize>,
        style: &'a Style,
        wrap: f32,
    ) -> Self {
        let job = LayoutJob {
            wrap: TextWrapping {
                max_width: wrap.max(20.0),
                ..Default::default()
            },
            break_on_newline: true,
            ..Default::default()
        };
        Self {
            text,
            base,
            delta,
            caret,
            style,
            job,
            map: SourceMap::default(),
        }
    }

    fn abs(&self, p: usize) -> usize {
        (p as isize + self.delta) as usize
    }

    fn abs_range(&self, r: &Range<usize>) -> Range<usize> {
        self.abs(r.start)..self.abs(r.end)
    }

    /// Whether the caret is within `r` (stored coordinates, inclusive ends).
    pub fn active(&self, r: &Range<usize>) -> bool {
        let r = self.abs_range(r);
        self.caret.is_some_and(|c| r.start <= c && c <= r.end)
    }

    /// Appends display text. `src` is the absolute source range; `atomic` marks
    /// text that is not byte-identical to the source.
    pub fn push(&mut self, s: &str, fmt: &Fmt, src: Option<Range<usize>>, atomic: bool) {
        if s.is_empty() && src.is_none() {
            return;
        }
        let start = self.job.text.len();
        self.job.text.push_str(s);
        let end = self.job.text.len();
        if start < end {
            self.job.sections.push(LayoutSection {
                leading_space: 0.0,
                byte_range: ByteIndex(start)..ByteIndex(end),
                format: fmt.text_format(),
            });
        }
        if let Some(r) = src {
            let rel = r.start.saturating_sub(self.base)..r.end.saturating_sub(self.base);
            if atomic {
                self.map.atomic(start..end, rel);
            } else {
                self.map.text(start..end, rel);
            }
        }
    }

    /// Reserves space for an object (inline math) mapped atomically to `src`:
    /// two invisible word joiners, the second carrying the object's width as
    /// letter spacing and a line height tall enough for it.
    pub fn push_object(&mut self, math: MathScene, fmt: &Fmt, src: Range<usize>) {
        const WJ: &str = "\u{2060}";
        let start = self.job.text.len();
        let f0 = fmt.text_format();
        self.job.text.push_str(WJ);
        let mid = self.job.text.len();
        self.job.sections.push(LayoutSection {
            leading_space: 0.0,
            byte_range: ByteIndex(start)..ByteIndex(mid),
            format: f0,
        });
        let mut f1 = fmt.text_format();
        f1.extra_letter_spacing = math.scene.width;
        f1.line_height = Some(fmt.line_height.max(math.scene.height + 2.0));
        self.job.text.push_str(WJ);
        let end = self.job.text.len();
        self.job.sections.push(LayoutSection {
            leading_space: 0.0,
            byte_range: ByteIndex(mid)..ByteIndex(end),
            format: f1,
        });
        let rel = src.start.saturating_sub(self.base)..src.end.saturating_sub(self.base);
        self.map.atomic(start..end, rel);
        self.map.objects.push(InlineObject {
            at: mid,
            width_bits: math.scene.width.to_bits(),
            math,
        });
    }

    /// Appends the source text of absolute range `r`.
    pub fn push_src(&mut self, r: Range<usize>, fmt: &Fmt) {
        if r.is_empty() {
            return;
        }
        let s = self.text.slice(r.clone()).into_owned();
        // Tabs and NULs are displayed as-is (NUL as U+FFFD would break 1:1 mapping).
        self.push(&s, fmt, Some(r), false);
    }

    /// Appends a visible marker (muted).
    pub fn push_marker(&mut self, r: Range<usize>, fmt: &Fmt) {
        let mut m = *fmt;
        m.color = self.style.mark_fg;
        m.bold = false;
        m.italic = false;
        m.strike = false;
        m.underline = false;
        m.bg = Color32::TRANSPARENT;
        self.push_src(r, &m);
    }

    /// Ensures the job is not empty so an empty line still has height and a caret
    /// position at `at` (absolute).
    pub fn finish_empty(&mut self, at: usize, fmt: &Fmt) {
        if self.job.sections.is_empty() {
            self.job.sections.push(LayoutSection {
                leading_space: 0.0,
                byte_range: ByteIndex(0)..ByteIndex(0),
                format: fmt.text_format(),
            });
        }
        if self.map.segs.is_empty() {
            let rel = at.saturating_sub(self.base);
            self.map
                .text(self.job.text.len()..self.job.text.len(), rel..rel);
        }
    }

    /// Appends inline nodes.
    pub fn inlines(&mut self, list: &[Inline], fmt: &Fmt) {
        for i in list {
            self.inline(i, fmt);
        }
    }

    fn inline(&mut self, i: &Inline, fmt: &Fmt) {
        let span = self.abs_range(&i.span);
        let active = self.active(&i.span);
        let style = self.style;
        match &i.kind {
            InlineKind::Text => {
                let start = self.job.text.len();
                self.push_src(span.clone(), fmt);
                if SPELL.with(std::cell::Cell::get) && !fmt.mono {
                    // Text nodes map byte-identically, so source ranges carry over.
                    let s = self.text.slice(span);
                    for r in inkra_text::spell::misspelled(&s) {
                        self.map.squiggles.push(start + r.start..start + r.end);
                    }
                }
            }
            InlineKind::Escape => {
                if active {
                    self.push_marker(span.start..span.start + 1, fmt);
                }
                self.push_src(span.start + 1..span.end, fmt);
            }
            InlineKind::Entity { text } => {
                if active {
                    self.push_src(span, fmt);
                } else {
                    self.push(text, fmt, Some(span), true);
                }
            }
            InlineKind::SoftBreak | InlineKind::HardBreak => self.push("\n", fmt, Some(span), true),
            InlineKind::Code { content, .. } => {
                let content = self.abs_range(content);
                let mut f = *fmt;
                f.mono = true;
                f.size = (fmt.size * 0.9).round();
                f.bg = style.code_bg;
                f.color = style.source.code;
                if active {
                    self.push_marker(span.start..content.start, &f);
                }
                if content.is_empty() {
                    self.push("", &f, Some(content.clone()), false);
                } else {
                    // Line breaks inside code spans display as spaces.
                    let s = self.text.slice(content.clone()).replace(['\n', '\r'], " ");
                    self.push(&s, &f, Some(content.clone()), false);
                }
                if active {
                    self.push_marker(content.end..span.end, &f);
                }
            }
            InlineKind::Emph { open, close }
            | InlineKind::Strong { open, close }
            | InlineKind::Strike { open, close }
            | InlineKind::Highlight { open, close }
            | InlineKind::Superscript { open, close }
            | InlineKind::Subscript { open, close } => {
                let mut f = *fmt;
                match i.kind {
                    InlineKind::Emph { .. } => f.italic = true,
                    InlineKind::Strong { .. } => f.bold = true,
                    InlineKind::Strike { .. } => f.strike = true,
                    InlineKind::Highlight { .. } => f.bg = style.highlight_bg,
                    InlineKind::Superscript { .. } => {
                        f.size = (fmt.size * 0.75).round();
                        f.valign = epaint::emath::Align::TOP;
                    }
                    _ => {
                        f.size = (fmt.size * 0.75).round();
                        f.valign = epaint::emath::Align::BOTTOM;
                    }
                }
                if active {
                    self.push_marker(self.abs_range(open), fmt);
                }
                self.inlines(&i.children, &f);
                if active {
                    self.push_marker(self.abs_range(close), fmt);
                }
            }
            InlineKind::Link { text, syntax, .. } => {
                let mut f = *fmt;
                f.color = style.accent;
                f.underline = true;
                let text_r = self.abs_range(text);
                match syntax {
                    LinkSyntax::Autolink if active => self.push_src(span, &f),
                    LinkSyntax::Autolink | LinkSyntax::Extended => self.inlines(&i.children, &f),
                    _ => {
                        if active {
                            self.push_marker(span.start..text_r.start, fmt);
                        }
                        self.inlines(&i.children, &f);
                        if active {
                            let mut u = *fmt;
                            u.color = style.source.url;
                            self.push_src(text_r.end..span.end, &u);
                        }
                    }
                }
            }
            InlineKind::Image { text, .. } => {
                if active {
                    let text_r = self.abs_range(text);
                    self.push_marker(span.start..text_r.start, fmt);
                    self.inlines(&i.children, fmt);
                    let mut u = *fmt;
                    u.color = style.source.url;
                    self.push_src(text_r.end..span.end, &u);
                } else {
                    let mut alt = String::new();
                    alt_text(self.text, self.delta, &i.children, &mut alt);
                    let mut f = *fmt;
                    f.color = style.accent;
                    self.push(&format!("🖼 {alt}"), &f, Some(span), true);
                }
            }
            InlineKind::Html => {
                let mut f = *fmt;
                f.color = style.source.html;
                f.mono = true;
                f.size = (fmt.size * 0.9).round();
                self.push_src(span, &f);
            }
            InlineKind::Math { content } => {
                if active {
                    let mut f = *fmt;
                    f.color = style.source.math;
                    self.push_src(span, &f);
                } else {
                    let tex = self.text.slice(self.abs_range(content)).into_owned();
                    let mut req =
                        inkra_math::Request::new(&tex, false, (fmt.size * MATH_SCALE).round());
                    if tex.contains("ref") {
                        req.refs = REFS.with(|r| r.borrow().clone());
                    }
                    self.push_object(math_scene(&req, fmt.color), fmt, span);
                }
            }
            InlineKind::FootnoteRef { label } => {
                if active {
                    self.push_marker(span, fmt);
                } else {
                    // MD-010: numbered by order of first reference.
                    let n = NOTES
                        .with(|n| n.borrow().iter().find(|(l, _)| l == label).map(|(_, n)| *n));
                    let shown = n.map_or_else(|| label.clone(), |n| n.to_string());
                    let mut f = *fmt;
                    f.size = (fmt.size * 0.75).round();
                    f.valign = epaint::emath::Align::TOP;
                    f.color = style.accent;
                    let start = self.job.text.len();
                    self.push(&shown, &f, Some(span), true);
                    let end = self.job.text.len();
                    self.map
                        .hits
                        .push((start..end, crate::block::HitAction::Footnote(label.clone())));
                }
            }
            InlineKind::Emoji { emoji } => {
                if active {
                    self.push_src(span, fmt);
                } else {
                    self.push(emoji, fmt, Some(span), true);
                }
            }
        }
    }
}

/// Plain text of inline nodes (image alt text).
pub fn alt_text(text: &Text, delta: isize, list: &[Inline], out: &mut String) {
    let abs =
        |r: &Range<usize>| (r.start as isize + delta) as usize..(r.end as isize + delta) as usize;
    for i in list {
        match &i.kind {
            InlineKind::Text => out.push_str(&text.slice(abs(&i.span))),
            InlineKind::Escape => out.push_str(&text.slice(abs(&(i.span.start + 1..i.span.end)))),
            InlineKind::Entity { text: t } => out.push_str(t),
            InlineKind::Code { text: t, .. } => out.push_str(t),
            InlineKind::SoftBreak | InlineKind::HardBreak => out.push(' '),
            _ => alt_text(text, delta, &i.children, out),
        }
    }
}

/// Absolute ranges of inline nodes on the caret's path (to key layouts of the
/// active leaf).
pub fn active_path(list: &[Inline], delta: isize, caret: usize, out: &mut Vec<Range<usize>>) {
    for i in list {
        let a = (i.span.start as isize + delta) as usize;
        let b = (i.span.end as isize + delta) as usize;
        if a <= caret && caret <= b {
            out.push(a..b);
            active_path(&i.children, delta, caret, out);
        }
    }
}
