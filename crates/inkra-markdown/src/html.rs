//! Reference HTML renderer producing CommonMark/GFM-compatible markup. It is used
//! by the conformance tests and as the semantic core of HTML export (EX-001).

use crate::Source;
use crate::ast::{Align, Block, BlockKind, Inline, InlineKind, ListKind};
use crate::inline::InlineContext;
use crate::refs::unescape;
use crate::walk;

/// Rendering options.
#[derive(Clone, Default)]
pub struct HtmlOptions<'h> {
    /// Add `id` anchors to headings.
    pub heading_ids: bool,
    /// Render soft breaks as `<br />` (MD-079).
    pub soft_break_as_br: bool,
    /// GFM tag filter: neutralize `<script>`, `<style>`, `<iframe>`… in raw HTML.
    pub tagfilter: bool,
    /// Export hooks (highlighting, math, diagrams, images, TOC); `None` renders
    /// the reference CommonMark output.
    pub hooks: Option<&'h dyn HtmlHooks>,
}

impl PartialEq for HtmlOptions<'_> {
    fn eq(&self, o: &Self) -> bool {
        self.heading_ids == o.heading_ids
            && self.soft_break_as_br == o.soft_break_as_br
            && self.tagfilter == o.tagfilter
    }
}

impl std::fmt::Debug for HtmlOptions<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HtmlOptions")
            .field("heading_ids", &self.heading_ids)
            .field("soft_break_as_br", &self.soft_break_as_br)
            .field("tagfilter", &self.tagfilter)
            .field("hooks", &self.hooks.is_some())
            .finish()
    }
}

/// Hooks used by HTML export (EX-001). Each returns the HTML to emit, or
/// `None` for the default rendering.
pub trait HtmlHooks {
    /// A fenced code block's contents (highlighting, diagrams).
    fn code_block(&self, lang: &str, code: &str) -> Option<String> {
        let _ = (lang, code);
        None
    }
    /// A formula.
    fn math(&self, tex: &str, display: bool) -> Option<String> {
        let _ = (tex, display);
        None
    }
    /// The `src` to write for an image destination.
    fn image_src(&self, dest: &str) -> Option<String> {
        let _ = dest;
        None
    }
    /// The table of contents for `[TOC]`: `(level, text, anchor)` of the headings.
    fn toc(&self, headings: &[(u8, String, String)]) -> Option<String> {
        let _ = headings;
        None
    }
}

const FILTERED_TAGS: &[&str] = &[
    "title",
    "textarea",
    "style",
    "xmp",
    "iframe",
    "noembed",
    "noframes",
    "script",
    "plaintext",
];

/// Applies the GFM tag filter to raw HTML.
pub fn tagfilter(raw: &str) -> String {
    let b = raw.as_bytes();
    let mut out = String::with_capacity(raw.len());
    let mut last = 0;
    for (i, _) in raw.match_indices('<') {
        let rest = &b[i + 1..];
        let rest = rest.strip_prefix(b"/").unwrap_or(rest);
        let hit = FILTERED_TAGS.iter().any(|t| {
            rest.len() >= t.len()
                && rest[..t.len()].eq_ignore_ascii_case(t.as_bytes())
                && rest.get(t.len()).is_none_or(|c| {
                    matches!(c, b' ' | b'\t' | b'\n' | b'\r' | b'\x0c' | b'>' | b'/')
                })
        });
        if hit {
            out.push_str(&raw[last..i]);
            out.push_str("&lt;");
            last = i + 1;
        }
    }
    out.push_str(&raw[last..]);
    out
}

/// Escapes text for HTML.
pub fn escape(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\0' => out.push('\u{FFFD}'),
            c => out.push(c),
        }
    }
}

/// Percent-encodes a URL like commonmark.js (`mdurl.encode`), keeping existing
/// `%XX` escapes, then HTML-escapes it.
pub fn escape_url(url: &str, out: &mut String) {
    const SAFE: &[u8] = b";/?:@&=+$,-_.!~*'()#";
    let b = url.as_bytes();
    let mut enc = String::with_capacity(url.len());
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c == b'%'
            && i + 2 < b.len()
            && b.get(i + 1).is_some_and(u8::is_ascii_hexdigit)
            && b.get(i + 2).is_some_and(u8::is_ascii_hexdigit)
        {
            enc.push_str(&url[i..i + 3]);
            i += 3;
            continue;
        }
        if c.is_ascii_alphanumeric() || SAFE.contains(&c) {
            enc.push(c as char);
        } else {
            enc.push_str(&format!("%{c:02X}"));
        }
        i += 1;
    }
    escape(&enc, out);
}

struct Renderer<'a, S: Source + ?Sized> {
    src: &'a S,
    o: &'a HtmlOptions<'a>,
    blocks: &'a [Block],
    out: String,
    slugs: crate::anchor::Slugger,
    footnotes: Vec<String>,
    _cx: &'a InlineContext,
}

/// Renders blocks to HTML.
pub fn render<S: Source + ?Sized>(
    src: &S,
    blocks: &[Block],
    cx: &InlineContext,
    o: &HtmlOptions<'_>,
) -> String {
    let mut r = Renderer {
        src,
        o,
        blocks,
        out: String::new(),
        slugs: Default::default(),
        footnotes: Vec::new(),
        _cx: cx,
    };
    for b in blocks {
        r.block(b, false);
    }
    r.footnote_section(blocks);
    r.out
}

impl<S: Source + ?Sized> Renderer<'_, S> {
    fn cr(&mut self) {
        if !self.out.is_empty() && !self.out.ends_with('\n') {
            self.out.push('\n');
        }
    }

    fn text(&mut self, s: &str) {
        escape(s, &mut self.out);
    }

    fn lines_raw(&self, b: &Block) -> String {
        let mut s = String::new();
        for l in &b.lines {
            for _ in 0..l.pad {
                s.push(' ');
            }
            s.push_str(&self.src.slice(l.span.clone()));
            s.push('\n');
        }
        s
    }

    fn block(&mut self, b: &Block, tight: bool) {
        match &b.kind {
            BlockKind::Paragraph => {
                if tight {
                    self.inlines(&b.inlines);
                } else {
                    self.cr();
                    self.out.push_str("<p>");
                    self.inlines(&b.inlines);
                    self.out.push_str("</p>");
                    self.cr();
                }
            }
            BlockKind::Toc => {
                self.cr();
                let toc = self.o.hooks.and_then(|h| {
                    let heads: Vec<(u8, String, String)> = walk::headings(self.src, self.blocks)
                        .into_iter()
                        .map(|h| (h.level, h.text, h.anchor))
                        .collect();
                    h.toc(&heads)
                });
                match toc {
                    Some(t) => self.out.push_str(&t),
                    None => self.out.push_str("<p>[TOC]</p>"),
                }
                self.cr();
            }
            BlockKind::Heading { level, .. } => {
                self.cr();
                if self.o.heading_ids {
                    let id = self.slugs.slug(&walk::block_text(self.src, b));
                    self.out.push_str(&format!("<h{level} id=\""));
                    escape(&id, &mut self.out);
                    self.out.push_str("\">");
                } else {
                    self.out.push_str(&format!("<h{level}>"));
                }
                self.inlines(&b.inlines);
                self.out.push_str(&format!("</h{level}>"));
                self.cr();
            }
            BlockKind::ThematicBreak => {
                self.cr();
                self.out.push_str("<hr />");
                self.cr();
            }
            BlockKind::CodeBlock { fence } => {
                self.cr();
                let lang = fence.as_ref().and_then(|f| f.info.clone()).map(|r| {
                    let info = unescape(&self.src.slice(r));
                    info.split([' ', '\t'])
                        .next()
                        .unwrap_or_default()
                        .to_owned()
                });
                let code = self.lines_raw(b);
                if let Some(h) = self.o.hooks
                    && let Some(html) = h.code_block(lang.as_deref().unwrap_or(""), &code)
                {
                    self.out.push_str(&html);
                    self.cr();
                    return;
                }
                match lang.filter(|l| !l.is_empty()) {
                    Some(l) => {
                        self.out.push_str("<pre><code class=\"language-");
                        escape(&l, &mut self.out);
                        self.out.push_str("\">");
                    }
                    None => self.out.push_str("<pre><code>"),
                }
                self.text(&code);
                self.out.push_str("</code></pre>");
                self.cr();
            }
            BlockKind::HtmlBlock { .. } => {
                self.cr();
                let raw = self.lines_raw(b).replace('\0', "\u{FFFD}");
                let raw = if self.o.tagfilter {
                    tagfilter(&raw)
                } else {
                    raw
                };
                self.out.push_str(&raw);
                self.cr();
            }
            BlockKind::LinkDef { .. } | BlockKind::FrontMatter { .. } => {}
            BlockKind::BlockQuote { alert } => {
                self.cr();
                match alert {
                    Some((k, _)) => {
                        let name = k.name();
                        let mut title = name.to_owned();
                        title[..1].make_ascii_uppercase();
                        self.out.push_str(&format!(
                            "<div class=\"markdown-alert markdown-alert-{name}\">\n<p class=\"markdown-alert-title\">{title}</p>"
                        ));
                        self.cr();
                        for c in &b.children {
                            self.block(c, false);
                        }
                        self.cr();
                        self.out.push_str("</div>");
                    }
                    None => {
                        self.out.push_str("<blockquote>");
                        self.cr();
                        for c in &b.children {
                            self.block(c, false);
                        }
                        self.cr();
                        self.out.push_str("</blockquote>");
                    }
                }
                self.cr();
            }
            BlockKind::List { kind, start, tight } => {
                self.cr();
                let tag = match kind {
                    ListKind::Bullet(_) => "ul",
                    ListKind::Ordered(_) => "ol",
                };
                if matches!(kind, ListKind::Ordered(_)) && *start != 1 {
                    self.out.push_str(&format!("<ol start=\"{start}\">"));
                } else {
                    self.out.push_str(&format!("<{tag}>"));
                }
                self.cr();
                for item in &b.children {
                    self.block(item, *tight);
                }
                self.cr();
                self.out.push_str(&format!("</{tag}>"));
                self.cr();
            }
            BlockKind::ListItem { task, .. } => {
                self.out.push_str("<li>");
                if let Some(t) = task {
                    if t.checked {
                        self.out
                            .push_str("<input checked=\"\" disabled=\"\" type=\"checkbox\"> ");
                    } else {
                        self.out
                            .push_str("<input disabled=\"\" type=\"checkbox\"> ");
                    }
                }
                for c in &b.children {
                    self.block(c, tight);
                }
                self.out.push_str("</li>");
                self.cr();
            }
            BlockKind::Table { aligns, .. } => {
                self.cr();
                self.out.push_str("<table>\n");
                let mut rows = b.children.iter();
                if let Some(head) = rows.next() {
                    self.out.push_str("<thead>\n");
                    self.row(head, aligns, "th");
                    self.out.push_str("</thead>\n");
                }
                let body: Vec<&Block> = rows.collect();
                if !body.is_empty() {
                    self.out.push_str("<tbody>\n");
                    for r in body {
                        self.row(r, aligns, "td");
                    }
                    self.out.push_str("</tbody>\n");
                }
                self.out.push_str("</table>");
                self.cr();
            }
            BlockKind::TableRow { .. } | BlockKind::TableCell => {}
            BlockKind::MathBlock { .. } => {
                self.cr();
                let tex = self.lines_raw(b);
                self.out.push_str("<div class=\"math math-display\">");
                match self.o.hooks.and_then(|h| h.math(tex.trim_end(), true)) {
                    Some(html) => self.out.push_str(&html),
                    None => self.text(tex.trim_end()),
                }
                self.out.push_str("</div>");
                self.cr();
            }
            BlockKind::FootnoteDef { .. } => {}
        }
    }

    fn row(&mut self, row: &Block, aligns: &[Align], tag: &str) {
        self.out.push_str("<tr>\n");
        for (i, align) in aligns.iter().enumerate() {
            let attr = match align {
                Align::None => "",
                Align::Left => " align=\"left\"",
                Align::Center => " align=\"center\"",
                Align::Right => " align=\"right\"",
            };
            self.out.push_str(&format!("<{tag}{attr}>"));
            if let Some(cell) = row.children.get(i) {
                self.inlines(&cell.inlines);
            }
            self.out.push_str(&format!("</{tag}>\n"));
        }
        self.out.push_str("</tr>\n");
    }

    fn footnote_section(&mut self, blocks: &[Block]) {
        if self.footnotes.is_empty() {
            return;
        }
        let mut defs = Vec::new();
        for b in blocks {
            b.walk(&mut |b| {
                if let BlockKind::FootnoteDef { label, .. } = &b.kind {
                    defs.push((label.clone(), b));
                }
            });
        }
        self.cr();
        self.out.push_str("<section class=\"footnotes\">\n<ol>\n");
        let order = std::mem::take(&mut self.footnotes);
        for (n, label) in order.iter().enumerate() {
            if let Some((_, def)) = defs.iter().find(|(l, _)| l == label) {
                self.out.push_str(&format!("<li id=\"fn-{}\">", n + 1));
                for c in &def.children {
                    self.block(c, false);
                }
                self.out.push_str(&format!(
                    "<a href=\"#fnref-{}\" class=\"footnote-backref\">↩</a></li>\n",
                    n + 1
                ));
            }
        }
        self.out.push_str("</ol>\n</section>\n");
    }

    fn inlines(&mut self, list: &[Inline]) {
        for i in list {
            self.inline(i);
        }
    }

    fn alt_text(&self, list: &[Inline], out: &mut String) {
        for i in list {
            match &i.kind {
                InlineKind::Text => out.push_str(&self.src.slice(i.span.clone())),
                InlineKind::Escape => out.push_str(&self.src.slice(i.span.start + 1..i.span.end)),
                InlineKind::Entity { text } => out.push_str(text),
                InlineKind::Code { text, .. } => out.push_str(text),
                InlineKind::SoftBreak | InlineKind::HardBreak => out.push('\n'),
                InlineKind::Html => out.push_str(&self.src.slice(i.span.clone())),
                InlineKind::Emoji { emoji } => out.push_str(emoji),
                _ => self.alt_text(&i.children, out),
            }
        }
    }

    fn inline(&mut self, i: &Inline) {
        match &i.kind {
            InlineKind::Text => {
                let t = self.src.slice(i.span.clone());
                self.text(&t);
            }
            InlineKind::Escape => {
                let t = self.src.slice(i.span.start + 1..i.span.end);
                self.text(&t);
            }
            InlineKind::Entity { text } => self.text(text),
            InlineKind::SoftBreak => {
                if self.o.soft_break_as_br {
                    self.out.push_str("<br />\n");
                } else {
                    self.out.push('\n');
                }
            }
            InlineKind::HardBreak => self.out.push_str("<br />\n"),
            InlineKind::Code { text, .. } => {
                self.out.push_str("<code>");
                self.text(text);
                self.out.push_str("</code>");
            }
            InlineKind::Emph { .. } => self.wrap("em", &i.children),
            InlineKind::Strong { .. } => self.wrap("strong", &i.children),
            InlineKind::Strike { .. } => self.wrap("del", &i.children),
            InlineKind::Highlight { .. } => self.wrap("mark", &i.children),
            InlineKind::Superscript { .. } => self.wrap("sup", &i.children),
            InlineKind::Subscript { .. } => self.wrap("sub", &i.children),
            InlineKind::Link { dest, title, .. } => {
                self.out.push_str("<a href=\"");
                escape_url(dest, &mut self.out);
                self.out.push('"');
                if let Some(t) = title {
                    self.out.push_str(" title=\"");
                    escape(t, &mut self.out);
                    self.out.push('"');
                }
                self.out.push('>');
                self.inlines(&i.children);
                self.out.push_str("</a>");
            }
            InlineKind::Image { dest, title, .. } => {
                self.out.push_str("<img src=\"");
                match self.o.hooks.and_then(|h| h.image_src(dest)) {
                    // Data URLs are written as they are.
                    Some(s) if s.starts_with("data:") => self.out.push_str(&s),
                    Some(s) => escape_url(&s, &mut self.out),
                    None => escape_url(dest, &mut self.out),
                }
                self.out.push_str("\" alt=\"");
                let mut alt = String::new();
                self.alt_text(&i.children, &mut alt);
                escape(&alt, &mut self.out);
                self.out.push('"');
                if let Some(t) = title {
                    self.out.push_str(" title=\"");
                    escape(t, &mut self.out);
                    self.out.push('"');
                }
                self.out.push_str(" />");
            }
            InlineKind::Html => {
                let t = self.src.slice(i.span.clone()).replace('\0', "\u{FFFD}");
                let t = if self.o.tagfilter { tagfilter(&t) } else { t };
                self.out.push_str(&t);
            }
            InlineKind::Math { content } => {
                self.out.push_str("<span class=\"math math-inline\">");
                let t = self.src.slice(content.clone()).into_owned();
                match self.o.hooks.and_then(|h| h.math(&t, false)) {
                    Some(html) => self.out.push_str(&html),
                    None => self.text(&t),
                }
                self.out.push_str("</span>");
            }
            InlineKind::FootnoteRef { label } => {
                let n = match self.footnotes.iter().position(|l| l == label) {
                    Some(p) => p + 1,
                    None => {
                        self.footnotes.push(label.clone());
                        self.footnotes.len()
                    }
                };
                self.out.push_str(&format!(
                    "<sup class=\"footnote-ref\"><a href=\"#fn-{n}\" id=\"fnref-{n}\">{n}</a></sup>"
                ));
            }
            InlineKind::Emoji { emoji } => self.out.push_str(emoji),
        }
    }

    fn wrap(&mut self, tag: &str, children: &[Inline]) {
        self.out.push_str(&format!("<{tag}>"));
        self.inlines(children);
        self.out.push_str(&format!("</{tag}>"));
    }
}

/// Convenience: parses and renders `src`.
pub fn to_html(src: &str, o: &crate::ParseOptions) -> String {
    to_html_with(src, o, &HtmlOptions::default())
}

/// Parses and renders `src` with rendering options.
pub fn to_html_with(src: &str, o: &crate::ParseOptions, ho: &HtmlOptions<'_>) -> String {
    let p = crate::parse(src, o);
    render(src, &p.blocks, &p.cx, ho)
}
