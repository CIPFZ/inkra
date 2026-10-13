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

use crate::run::{InlineObject, MathScene, ObjectKind, SourceMap};

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

thread_local! {
    /// Natural sizes of the `<img>` images of the block being laid out.
    static IMG_SIZES: std::cell::RefCell<Vec<(String, [f32; 2])>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Sets the natural sizes of inline `<img>` images for this thread's layout.
pub fn set_image_sizes(sizes: Vec<(String, [f32; 2])>) {
    IMG_SIZES.with(|s| *s.borrow_mut() = sizes);
}

fn image_size(src: &str) -> Option<[f32; 2]> {
    IMG_SIZES.with(|s| s.borrow().iter().find(|(k, _)| k == src).map(|(_, v)| *v))
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

/// The label of the first `\ref{…}` / `\eqref{…}` in TeX source.
pub(crate) fn first_ref(tex: &str) -> Option<&str> {
    let i = tex
        .find("\\eqref{")
        .map(|i| i + 7)
        .or_else(|| tex.find("\\ref{").map(|i| i + 5))?;
    let rest = &tex[i..];
    rest.find('}')
        .map(|j| rest[..j].trim())
        .filter(|l| !l.is_empty())
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
    /// Typeface when monospace.
    pub mono_kind: crate::fonts::FontKind,
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
            mono_kind: style.mono_font,
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
                    if self.mono { self.mono_kind } else { self.kind },
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
        // A boundary with the previous text counts too.
        let prev = self.job.text.chars().next_back();
        self.job.text.push_str(s);
        let end = self.job.text.len();
        if start < end {
            if self.style.cjk_latin_spacing && !fmt.mono {
                self.push_spaced(start, prev, fmt);
            } else {
                self.job.sections.push(LayoutSection {
                    leading_space: 0.0,
                    byte_range: ByteIndex(start)..ByteIndex(end),
                    format: fmt.text_format(),
                });
            }
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

    /// Sections for text from `start` with a quarter em of extra space at each
    /// boundary between CJK and Latin letters or digits (theme
    /// `fonts.cjk_latin_spacing`); the source text is not changed.
    fn push_spaced(&mut self, start: usize, prev: Option<char>, fmt: &Fmt) {
        fn cjk(c: char) -> bool {
            matches!(c as u32, 0x3040..=0x30FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF | 0xAC00..=0xD7AF)
        }
        fn latin(c: char) -> bool {
            c.is_ascii_alphanumeric()
        }
        let end = self.job.text.len();
        let gap = fmt.size * 0.25;
        let mut seg = start;
        let mut lead = 0.0;
        let mut last = prev;
        let boundaries: Vec<usize> = self.job.text[start..]
            .char_indices()
            .filter_map(|(k, c)| {
                let b = last.is_some_and(|p| (cjk(p) && latin(c)) || (latin(p) && cjk(c)));
                last = Some(c);
                b.then_some(start + k)
            })
            .collect();
        for at in boundaries {
            if at > seg {
                self.job.sections.push(LayoutSection {
                    leading_space: lead,
                    byte_range: ByteIndex(seg)..ByteIndex(at),
                    format: fmt.text_format(),
                });
            }
            seg = at;
            lead = gap;
        }
        self.job.sections.push(LayoutSection {
            leading_space: lead,
            byte_range: ByteIndex(seg)..ByteIndex(end),
            format: fmt.text_format(),
        });
    }

    /// Reserves space for an object (inline math) mapped atomically to `src`:
    /// two invisible word joiners, the second carrying the object's width as
    /// letter spacing and a line height tall enough for it.
    pub fn push_object(&mut self, math: MathScene, fmt: &Fmt, src: Range<usize>) {
        let (w, h) = (math.scene.width, math.scene.height);
        self.push_reserved(w, h, ObjectKind::Math(math), fmt, Some(src));
    }

    /// Reserves `w × h` for an image whose bottom sits on the baseline.
    pub fn push_image(&mut self, src_name: &str, w: f32, h: f32, fmt: &Fmt) {
        let kind = ObjectKind::Image {
            src: src_name.to_owned(),
            size: [w.to_bits(), h.to_bits()],
        };
        self.push_reserved(w, h, kind, fmt, None);
    }

    fn push_reserved(
        &mut self,
        width: f32,
        height: f32,
        kind: ObjectKind,
        fmt: &Fmt,
        src: Option<Range<usize>>,
    ) {
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
        f1.extra_letter_spacing = width;
        f1.line_height = Some(fmt.line_height.max(height + 2.0));
        self.job.text.push_str(WJ);
        let end = self.job.text.len();
        self.job.sections.push(LayoutSection {
            leading_space: 0.0,
            byte_range: ByteIndex(mid)..ByteIndex(end),
            format: f1,
        });
        if let Some(src) = src {
            let rel = src.start.saturating_sub(self.base)..src.end.saturating_sub(self.base);
            self.map.atomic(start..end, rel);
        }
        self.map.objects.push(InlineObject {
            at: mid,
            width_bits: width.to_bits(),
            kind,
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

    /// Appends inline nodes. Inline HTML formatting tags (MD-040) style the
    /// nodes between them; the tags show as muted source only when the caret
    /// is on them.
    pub fn inlines(&mut self, list: &[Inline], fmt: &Fmt) {
        let mut stack: Vec<(String, Fmt)> = Vec::new();
        let mut cur = *fmt;
        for i in list {
            if matches!(i.kind, InlineKind::Html) {
                let span = self.abs_range(&i.span);
                let src = self.text.slice(span.clone()).into_owned();
                if src.starts_with("<!--") {
                    if self.style.print {
                        // Not printed (MD-040).
                        self.push("", &cur, Some(span.start..span.start), true);
                        continue;
                    }
                    // Comments are faint (MD-040).
                    let mut f = cur;
                    f.color = self.style.mark_fg;
                    self.push_src(span, &f);
                    continue;
                }
                if let Some((name, closing)) = inkra_html::render::inline_tag(&src) {
                    let active = self.active(&i.span);
                    let styled = matches!(
                        name.as_str(),
                        "b" | "strong"
                            | "i"
                            | "em"
                            | "u"
                            | "ins"
                            | "s"
                            | "del"
                            | "strike"
                            | "kbd"
                            | "code"
                            | "mark"
                            | "sup"
                            | "sub"
                            | "span"
                            | "font"
                            | "small"
                            | "big"
                    );
                    if name == "img" && !closing {
                        if active {
                            self.push_marker(span, &cur);
                        } else {
                            let s = inkra_html::render::tag_attr(&src, "src").unwrap_or_default();
                            let px = |k: &str| {
                                inkra_html::render::tag_attr(&src, k)
                                    .and_then(|v| v.trim_end_matches("px").parse::<f32>().ok())
                            };
                            // ED-090: width/height attributes, `style="zoom"`, else
                            // the natural size.
                            let natural = image_size(&s);
                            let zoom = inkra_html::render::tag_zoom(&src);
                            let (iw, ih) = match (px("width"), px("height"), natural) {
                                (Some(a), Some(b), _) => (a, b),
                                (Some(a), None, Some(n)) => (a, a * n[1] / n[0].max(1.0)),
                                (None, Some(b), Some(n)) => (b * n[0] / n[1].max(1.0), b),
                                (None, None, Some(n)) => {
                                    let z = zoom.unwrap_or(1.0);
                                    (n[0] * z, n[1] * z)
                                }
                                (Some(a), None, None) => (a, a * 0.6),
                                (None, Some(b), None) => (b * 1.6, b),
                                _ => (cur.line_height * 1.6, cur.line_height),
                            };
                            let max_w = self.job.wrap.max_width;
                            let (iw, ih) = if iw > max_w && max_w.is_finite() {
                                (max_w, ih * max_w / iw)
                            } else {
                                (iw, ih)
                            };
                            let start = self.job.text.len();
                            self.push_image(&s, iw, ih, &cur);
                            let end = self.job.text.len();
                            let rel = span.start.saturating_sub(self.base)
                                ..span.end.saturating_sub(self.base);
                            self.map.atomic(start..end, rel);
                        }
                        continue;
                    }
                    if matches!(name.as_str(), "video" | "audio" | "iframe") {
                        if active {
                            self.push_marker(span, &cur);
                        } else if !closing {
                            let s = inkra_html::render::tag_attr(&src, "src").unwrap_or_default();
                            let mut f = cur;
                            f.bg = self.style.code_bg;
                            f.color = self.style.fg_muted;
                            self.push(&format!(" ▶ {name} {s} "), &f, Some(span), true);
                        } else {
                            self.push("", &cur, Some(span.start..span.start), true);
                        }
                        continue;
                    }
                    if name == "br" {
                        if active {
                            self.push_marker(span.clone(), &cur);
                        }
                        self.push("\n", &cur, Some(span.end..span.end), true);
                        continue;
                    }
                    if styled {
                        if active {
                            self.push_marker(span, &cur);
                        } else {
                            // Hidden, but the caret can still stop before it.
                            self.push("", &cur, Some(span.start..span.start), true);
                        }
                        if closing {
                            if let Some(k) = stack.iter().rposition(|(n, _)| *n == name) {
                                cur = stack[k].1;
                                stack.truncate(k);
                            }
                        } else {
                            stack.push((name.clone(), cur));
                            cur = html_fmt(&name, &src, cur, self.style);
                        }
                        continue;
                    }
                }
            }
            self.inline(i, &cur);
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
            InlineKind::Link {
                text, syntax, dest, ..
            } => {
                let mut f = *fmt;
                f.color = style.accent;
                f.underline = true;
                let text_r = self.abs_range(text);
                let hit_start = self.job.text.len();
                let dest = dest.clone();
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
                if !active {
                    let end = self.job.text.len();
                    if end > hit_start {
                        self.map
                            .hits
                            .push((hit_start..end, crate::block::HitAction::InlineLink(dest)));
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
                    let at = self.job.text.len();
                    self.push_object(math_scene(&req, fmt.color), fmt, span);
                    // MD-008: Ctrl+click on `\eqref{l}` jumps to the equation.
                    if let Some(l) = first_ref(&tex) {
                        let end = self.job.text.len();
                        self.map.hits.push((
                            at..end,
                            crate::block::HitAction::InlineLink(format!("#{l}")),
                        ));
                    }
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

/// The format inside an inline HTML tag (MD-040).
fn html_fmt(name: &str, src: &str, base: Fmt, style: &Style) -> Fmt {
    let mut f = base;
    match name {
        "b" | "strong" => f.bold = true,
        "i" | "em" => f.italic = true,
        "u" | "ins" => f.underline = true,
        "s" | "del" | "strike" => f.strike = true,
        "kbd" | "code" => {
            f.mono = true;
            f.size = (base.size * 0.88).round();
            f.bg = style.code_bg;
        }
        "mark" => f.bg = style.highlight_bg,
        "sup" | "sub" => {
            f.size = (base.size * 0.75).round();
            f.valign = if name == "sup" {
                epaint::emath::Align::TOP
            } else {
                epaint::emath::Align::BOTTOM
            };
        }
        "small" => f.size = (base.size * 0.85).round(),
        "big" => f.size = (base.size * 1.2).round(),
        _ => {}
    }
    let css = inkra_html::render::tag_attr(src, "style");
    let prop = |name: &str| {
        css.as_deref().and_then(|s| {
            s.split(';').find_map(|d| {
                d.split_once(':')
                    .filter(|(k, _)| k.trim().eq_ignore_ascii_case(name))
                    .map(|(_, v)| v.trim().to_owned())
            })
        })
    };
    let color = inkra_html::render::tag_attr(src, "color").or_else(|| prop("color"));
    if let Some([r, g, b]) = color.as_deref().and_then(inkra_html::render::parse_color) {
        f.color = Color32::from_rgb(r, g, b);
    }
    // MD-040 with the MD-014 style subset.
    let bg = prop("background-color").or_else(|| prop("background"));
    if let Some([r, g, b]) = bg.as_deref().and_then(inkra_html::render::parse_color) {
        f.bg = Color32::from_rgb(r, g, b);
    }
    if let Some((k, relative)) = prop("font-size")
        .as_deref()
        .and_then(inkra_html::render::font_size_factor)
    {
        f.size = if relative {
            base.size * k
        } else {
            style.size * k
        }
        .round();
        f.line_height = f.line_height.max((f.size * 1.3).round());
    }
    if prop("font-weight").is_some_and(|w| w == "bold" || w.parse::<u32>().is_ok_and(|n| n >= 600))
    {
        f.bold = true;
    }
    if prop("font-style").is_some_and(|w| w == "italic") {
        f.italic = true;
    }
    f
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
