//! The renderable HTML subset (MD-014, MD-040): an HTML block is flattened to
//! paragraphs of styled spans, images, rules, `<details>`, simple tables and
//! placeholder cards for media. Everything else contributes its text.

use html5ever::tendril::TendrilSink;
use html5ever::{ParseOpts, parse_document};
use markup5ever_rcdom::{Handle, NodeData, RcDom};

/// Horizontal alignment (`align=`, `style="text-align"`, `<center>`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Align {
    /// Left.
    #[default]
    Left,
    /// Center.
    Center,
    /// Right.
    Right,
}

/// A run of text (or an inline image) with its style.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Span {
    /// Text (empty for images).
    pub text: String,
    /// Bold.
    pub bold: bool,
    /// Italic.
    pub italic: bool,
    /// Underlined.
    pub underline: bool,
    /// Struck through.
    pub strike: bool,
    /// Monospace.
    pub code: bool,
    /// Keyboard key style.
    pub kbd: bool,
    /// Highlighted.
    pub mark: bool,
    /// Superscript.
    pub sup: bool,
    /// Subscript.
    pub sub: bool,
    /// Text color.
    pub color: Option<[u8; 3]>,
    /// Background color (`style="background-color"`).
    pub bg: Option<[u8; 3]>,
    /// Link target.
    pub link: Option<String>,
    /// Relative font size.
    pub size: Option<f32>,
    /// An inline image: source and requested size.
    pub image: Option<(String, Option<f32>, Option<f32>)>,
    /// `style="zoom: 50%"` of an image (ED-090), as a factor.
    pub zoom: Option<f32>,
}

/// A block of the flattened HTML.
#[derive(Debug, Clone, PartialEq)]
pub enum HBlock {
    /// A paragraph-like block.
    Para {
        /// Alignment.
        align: Align,
        /// Content.
        spans: Vec<Span>,
        /// `<h1>`…`<h6>`.
        heading: Option<u8>,
        /// List marker (`•`, `1.`).
        marker: Option<String>,
        /// Nesting (lists, blockquotes).
        indent: u8,
    },
    /// Preformatted text.
    Pre(String),
    /// `<hr>`.
    Rule,
    /// `<details>`: summary, whether open, and body.
    Details {
        /// Summary.
        summary: Vec<Span>,
        /// `open` attribute.
        open: bool,
        /// Content.
        body: Vec<HBlock>,
    },
    /// A table: rows of cells.
    Table(Vec<Vec<Vec<Span>>>),
    /// `<video>`, `<audio>`, `<iframe>`: a card with the source.
    Media {
        /// Element name.
        kind: String,
        /// Source.
        src: String,
    },
}

fn tag(h: &Handle) -> Option<String> {
    match &h.data {
        NodeData::Element { name, .. } => Some(name.local.to_string()),
        _ => None,
    }
}

fn attr(h: &Handle, key: &str) -> Option<String> {
    match &h.data {
        NodeData::Element { attrs, .. } => attrs
            .borrow()
            .iter()
            .find(|a| &*a.name.local == key)
            .map(|a| a.value.to_string()),
        _ => None,
    }
}

fn style_prop(h: &Handle, prop: &str) -> Option<String> {
    let s = attr(h, "style")?;
    s.split(';').find_map(|d| {
        let (k, v) = d.split_once(':')?;
        (k.trim().eq_ignore_ascii_case(prop)).then(|| v.trim().to_owned())
    })
}

/// A CSS `zoom` value (`50%`, `0.5`) as a factor.
pub fn zoom_factor(v: &str) -> Option<f32> {
    let v = v.trim();
    let f = match v.strip_suffix('%') {
        Some(p) => p.trim().parse::<f32>().ok()? / 100.0,
        None => v.parse::<f32>().ok()?,
    };
    (f > 0.0 && f < 20.0).then_some(f)
}

/// The `zoom` of a tag's inline style.
pub fn tag_zoom(src: &str) -> Option<f32> {
    let style = tag_attr(src, "style")?;
    style.split(';').find_map(|d| {
        let (k, v) = d.split_once(':')?;
        (k.trim().eq_ignore_ascii_case("zoom"))
            .then(|| zoom_factor(v))
            .flatten()
    })
}

/// CSS / HTML color: `#rgb`, `#rrggbb`, `rgb(r,g,b)` or a basic name.
pub fn parse_color(s: &str) -> Option<[u8; 3]> {
    let s = s.trim().to_ascii_lowercase();
    if let Some(h) = s.strip_prefix('#') {
        let v = |i: usize, n: usize| u8::from_str_radix(h.get(i..i + n)?, 16).ok();
        return match h.len() {
            3 => Some([v(0, 1)? * 17, v(1, 1)? * 17, v(2, 1)? * 17]),
            6 => Some([v(0, 2)?, v(2, 2)?, v(4, 2)?]),
            _ => None,
        };
    }
    if let Some(inner) = s.strip_prefix("rgb(").and_then(|x| x.strip_suffix(')')) {
        let v: Vec<u8> = inner
            .split(',')
            .filter_map(|x| x.trim().parse().ok())
            .collect();
        return (v.len() == 3).then(|| [v[0], v[1], v[2]]);
    }
    Some(match s.as_str() {
        "red" => [220, 38, 38],
        "green" => [22, 163, 74],
        "blue" => [37, 99, 235],
        "orange" => [234, 88, 12],
        "purple" => [147, 51, 234],
        "gray" | "grey" => [128, 128, 128],
        "black" => [0, 0, 0],
        "white" => [255, 255, 255],
        "yellow" => [202, 138, 4],
        "pink" => [219, 39, 119],
        "brown" => [146, 64, 14],
        "teal" => [13, 148, 136],
        _ => return None,
    })
}

fn align_of(h: &Handle) -> Option<Align> {
    let a = attr(h, "align").or_else(|| style_prop(h, "text-align"))?;
    Some(match a.trim().to_ascii_lowercase().as_str() {
        "center" | "middle" => Align::Center,
        "right" => Align::Right,
        _ => Align::Left,
    })
}

fn px(v: Option<String>) -> Option<f32> {
    let v = v?;
    v.trim().trim_end_matches("px").parse().ok()
}

const BLOCKS: &[&str] = &[
    "p",
    "div",
    "center",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "ul",
    "ol",
    "li",
    "blockquote",
    "pre",
    "hr",
    "table",
    "details",
    "summary",
    "section",
    "article",
    "header",
    "footer",
    "figure",
    "figcaption",
    "dl",
    "dt",
    "dd",
    "video",
    "audio",
    "iframe",
    "picture",
];

struct Flat {
    out: Vec<HBlock>,
    spans: Vec<Span>,
    align: Align,
    heading: Option<u8>,
    marker: Option<String>,
    indent: u8,
}

impl Flat {
    fn new() -> Self {
        Self {
            out: Vec::new(),
            spans: Vec::new(),
            align: Align::Left,
            heading: None,
            marker: None,
            indent: 0,
        }
    }

    fn flush(&mut self) {
        // Drop whitespace-only paragraphs.
        let empty = self
            .spans
            .iter()
            .all(|s| s.image.is_none() && s.text.trim().is_empty());
        if !empty {
            // Trim the ends of the paragraph.
            if let Some(f) = self.spans.first_mut() {
                f.text = f.text.trim_start().to_owned();
            }
            if let Some(l) = self.spans.last_mut() {
                l.text = l.text.trim_end().to_owned();
            }
            self.out.push(HBlock::Para {
                align: self.align,
                spans: std::mem::take(&mut self.spans),
                heading: self.heading,
                marker: self.marker.take(),
                indent: self.indent,
            });
        }
        self.spans.clear();
    }

    fn text(&mut self, t: &str, st: &Span) {
        // Collapse whitespace as browsers do.
        let mut s = String::with_capacity(t.len());
        let mut ws = self.spans.last().is_some_and(|l| l.text.ends_with(' '));
        for c in t.chars() {
            if c.is_whitespace() {
                if !ws {
                    s.push(' ');
                }
                ws = true;
            } else {
                s.push(c);
                ws = false;
            }
        }
        if s.is_empty() {
            return;
        }
        let mut sp = st.clone();
        sp.text = s;
        self.spans.push(sp);
    }

    fn node(&mut self, h: &Handle, st: &Span, list: &mut Option<(bool, u32)>) {
        match &h.data {
            NodeData::Text { contents } => {
                let t = contents.borrow().to_string();
                self.text(&t, st);
            }
            NodeData::Element { .. } => {
                let t = tag(h).unwrap_or_default();
                if matches!(
                    t.as_str(),
                    "script" | "style" | "head" | "title" | "meta" | "template" | "noscript"
                ) {
                    return;
                }
                let block = BLOCKS.contains(&t.as_str());
                if block {
                    self.flush();
                }
                let saved = (self.align, self.heading, self.indent);
                if let Some(a) = align_of(h) {
                    self.align = a;
                }
                let mut s = st.clone();
                match t.as_str() {
                    "b" | "strong" => s.bold = true,
                    "i" | "em" | "cite" | "var" => s.italic = true,
                    "u" | "ins" => s.underline = true,
                    "s" | "del" | "strike" => s.strike = true,
                    "code" | "tt" | "samp" => s.code = true,
                    "kbd" => s.kbd = true,
                    "mark" => s.mark = true,
                    "sup" => s.sup = true,
                    "sub" => s.sub = true,
                    "small" => s.size = Some(0.85),
                    "big" => s.size = Some(1.2),
                    "center" => self.align = Align::Center,
                    "a" => s.link = attr(h, "href"),
                    "font" => {
                        if let Some(c) = attr(h, "color").and_then(|c| parse_color(&c)) {
                            s.color = Some(c);
                        }
                    }
                    "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                        self.heading = t[1..].parse().ok();
                        s.bold = true;
                    }
                    "blockquote" => self.indent += 1,
                    "br" => {
                        let mut b = s.clone();
                        b.text = "\n".into();
                        self.spans.push(b);
                        return;
                    }
                    "hr" => {
                        self.out.push(HBlock::Rule);
                        return;
                    }
                    "img" => {
                        let src = attr(h, "src").unwrap_or_default();
                        let w = px(attr(h, "width")).or_else(|| px(style_prop(h, "width")));
                        let hh = px(attr(h, "height")).or_else(|| px(style_prop(h, "height")));
                        let mut i = s.clone();
                        i.text = String::new();
                        i.image = Some((src, w, hh));
                        i.zoom = style_prop(h, "zoom").as_deref().and_then(zoom_factor);
                        self.spans.push(i);
                        return;
                    }
                    "video" | "audio" | "iframe" => {
                        let src = attr(h, "src")
                            .or_else(|| {
                                h.children
                                    .borrow()
                                    .iter()
                                    .find(|c| tag(c).as_deref() == Some("source"))
                                    .and_then(|c| attr(c, "src"))
                            })
                            .unwrap_or_default();
                        self.out.push(HBlock::Media {
                            kind: t.clone(),
                            src,
                        });
                        return;
                    }
                    "pre" => {
                        let mut txt = String::new();
                        collect(h, &mut txt);
                        self.out
                            .push(HBlock::Pre(txt.trim_end_matches('\n').to_owned()));
                        return;
                    }
                    "details" => {
                        let open = attr(h, "open").is_some();
                        let mut summary = Vec::new();
                        let mut inner = Flat::new();
                        for c in h.children.borrow().iter() {
                            if tag(c).as_deref() == Some("summary") {
                                let mut sf = Flat::new();
                                for cc in c.children.borrow().iter() {
                                    sf.node(cc, &s, &mut None);
                                }
                                summary = sf.spans;
                            } else {
                                inner.node(c, &s, &mut None);
                            }
                        }
                        inner.flush();
                        self.out.push(HBlock::Details {
                            summary,
                            open,
                            body: inner.out,
                        });
                        return;
                    }
                    "table" => {
                        let mut rows = Vec::new();
                        rows_of(h, &s, &mut rows);
                        self.out.push(HBlock::Table(rows));
                        return;
                    }
                    "ul" | "ol" => {
                        self.indent += 1;
                        let mut l = Some((
                            t == "ol",
                            attr(h, "start").and_then(|x| x.parse().ok()).unwrap_or(1),
                        ));
                        for c in h.children.borrow().iter() {
                            self.node(c, &s, &mut l);
                        }
                        self.flush();
                        (self.align, self.heading, self.indent) = saved;
                        return;
                    }
                    "li" => {
                        self.marker = Some(match list {
                            Some((true, n)) => {
                                let m = format!("{n}.");
                                *n += 1;
                                m
                            }
                            _ => "•".into(),
                        });
                    }
                    _ => {}
                }
                if let Some(c) = style_prop(h, "color").and_then(|c| parse_color(&c)) {
                    s.color = Some(c);
                }
                // MD-014: `background-color` and `font-size` of the style subset.
                if let Some(c) = style_prop(h, "background-color")
                    .or_else(|| style_prop(h, "background"))
                    .and_then(|c| parse_color(&c))
                {
                    s.bg = Some(c);
                }
                if let Some((k, relative)) =
                    style_prop(h, "font-size").and_then(|v| font_size_factor(&v))
                {
                    s.size = Some(if relative {
                        s.size.unwrap_or(1.0) * k
                    } else {
                        k
                    });
                }
                if style_prop(h, "font-weight")
                    .is_some_and(|w| w == "bold" || w.parse::<u32>().is_ok_and(|n| n >= 600))
                {
                    s.bold = true;
                }
                if style_prop(h, "font-style").is_some_and(|w| w == "italic") {
                    s.italic = true;
                }
                let mut nested_list = None;
                for c in h.children.borrow().iter() {
                    self.node(c, &s, &mut nested_list);
                }
                if block {
                    self.flush();
                }
                (self.align, self.heading, self.indent) = saved;
            }
            NodeData::Document => {
                for c in h.children.borrow().iter() {
                    self.node(c, st, list);
                }
            }
            _ => {}
        }
    }
}

/// A CSS `font-size` as a factor of the body size (16 px), and whether it is
/// relative to the parent (`em`, `%`).
pub fn font_size_factor(v: &str) -> Option<(f32, bool)> {
    let v = v.trim().to_ascii_lowercase();
    let num = |s: &str| s.trim().parse::<f32>().ok().filter(|x| *x > 0.0);
    let (k, relative) = if let Some(n) = v.strip_suffix("px") {
        (num(n)? / 16.0, false)
    } else if let Some(n) = v.strip_suffix("pt") {
        (num(n)? * 4.0 / 3.0 / 16.0, false)
    } else if let Some(n) = v.strip_suffix("rem") {
        (num(n)?, false)
    } else if let Some(n) = v.strip_suffix("em") {
        (num(n)?, true)
    } else if let Some(n) = v.strip_suffix('%') {
        (num(n)? / 100.0, true)
    } else {
        let k = match v.as_str() {
            "xx-small" => 0.6,
            "x-small" => 0.75,
            "small" | "smaller" => 0.89,
            "medium" => 1.0,
            "large" | "larger" => 1.2,
            "x-large" => 1.5,
            "xx-large" => 2.0,
            _ => return None,
        };
        (k, false)
    };
    Some((k.clamp(0.4, 4.0), relative))
}

fn collect(h: &Handle, out: &mut String) {
    match &h.data {
        NodeData::Text { contents } => out.push_str(&contents.borrow()),
        _ => {
            if tag(h).as_deref() == Some("br") {
                out.push('\n');
            }
            for c in h.children.borrow().iter() {
                collect(c, out);
            }
        }
    }
}

fn rows_of(h: &Handle, st: &Span, rows: &mut Vec<Vec<Vec<Span>>>) {
    for c in h.children.borrow().iter() {
        match tag(c).as_deref() {
            Some("tr") => {
                let mut row = Vec::new();
                for cell in c.children.borrow().iter() {
                    if matches!(tag(cell).as_deref(), Some("td" | "th")) {
                        let mut f = Flat::new();
                        let mut s = st.clone();
                        s.bold |= tag(cell).as_deref() == Some("th");
                        for cc in cell.children.borrow().iter() {
                            f.node(cc, &s, &mut None);
                        }
                        let mut spans = f.spans;
                        for b in f.out {
                            if let HBlock::Para { spans: sp, .. } = b {
                                spans.extend(sp);
                            }
                        }
                        row.push(spans);
                    }
                }
                rows.push(row);
            }
            Some(_) => rows_of(c, st, rows),
            None => {}
        }
    }
}

/// Whether the HTML must be shown as source (scripts and styles never run).
pub fn needs_source(html: &str) -> bool {
    let l = html.to_ascii_lowercase();
    l.contains("<script") || l.contains("<style")
}

/// Flattens an HTML fragment.
pub fn flatten(html: &str) -> Vec<HBlock> {
    let Ok(dom) = parse_document(RcDom::default(), ParseOpts::default())
        .from_utf8()
        .read_from(&mut html.as_bytes())
    else {
        return Vec::new();
    };
    let mut f = Flat::new();
    f.node(&dom.document, &Span::default(), &mut None);
    f.flush();
    f.out
}

/// The style an inline HTML tag applies to following text (MD-040):
/// `Some((name, is_closing))` for the supported formatting tags.
pub fn inline_tag(src: &str) -> Option<(String, bool)> {
    let s = src.trim();
    let inner = s.strip_prefix('<')?.strip_suffix('>')?;
    let (closing, inner) = match inner.strip_prefix('/') {
        Some(r) => (true, r),
        None => (false, inner),
    };
    let name: String = inner
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase();
    if name.is_empty() {
        return None;
    }
    Some((name, closing))
}

/// An attribute of a single tag's source.
pub fn tag_attr(src: &str, key: &str) -> Option<String> {
    let l = src.to_ascii_lowercase();
    let i = l.find(&format!("{key}="))? + key.len() + 1;
    let rest = &src[i..];
    let (q, body) = match rest.chars().next()? {
        c @ ('"' | '\'') => (Some(c), &rest[1..]),
        _ => (None, rest),
    };
    let end = match q {
        Some(c) => body.find(c)?,
        None => body.find([' ', '>', '/']).unwrap_or(body.len()),
    };
    Some(body[..end].to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flattens_common_readme_html() {
        let b = flatten(
            r#"<p align="center"><img src="logo.png" width="120"><br><b>Inkra</b> is <i>fast</i></p><details><summary>More</summary><p>hidden</p></details><hr><video src="a.mp4"></video>"#,
        );
        match &b[0] {
            HBlock::Para {
                align: Align::Center,
                spans,
                ..
            } => {
                assert_eq!(spans[0].image, Some(("logo.png".into(), Some(120.0), None)));
                assert!(spans.iter().any(|s| s.bold && s.text == "Inkra"));
                assert!(spans.iter().any(|s| s.italic && s.text == "fast"));
            }
            x => panic!("{x:?}"),
        }
        assert!(matches!(&b[1], HBlock::Details { open: false, body, .. } if body.len() == 1));
        assert_eq!(b[2], HBlock::Rule);
        assert!(matches!(&b[3], HBlock::Media { kind, src } if kind == "video" && src == "a.mp4"));
        let t =
            flatten("<table><tr><th>A</th><th>B</th></tr><tr><td>1</td><td>2</td></tr></table>");
        assert!(matches!(&t[0], HBlock::Table(rows) if rows.len() == 2 && rows[0][0][0].bold));
        let l = flatten("<ol><li>one</li><li>two</li></ol>");
        assert!(matches!(&l[1], HBlock::Para { marker: Some(m), .. } if m == "2."));
        assert!(needs_source("<script>x</script>"));
        assert_eq!(inline_tag("<kbd>"), Some(("kbd".into(), false)));
        assert_eq!(inline_tag("</SPAN>"), Some(("span".into(), true)));
        assert_eq!(
            tag_attr(r#"<span style="color: red">"#, "style").as_deref(),
            Some("color: red")
        );
        assert_eq!(parse_color("#f00"), Some([255, 0, 0]));
    }
}
