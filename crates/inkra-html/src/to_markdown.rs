//! HTML → Markdown conversion (ED-016 rule 3, ED-016 rule 6, EX-020).

use std::cell::Ref;

use html5ever::tendril::TendrilSink;
use html5ever::{ParseOpts, parse_document};
use markup5ever_rcdom::{Handle, NodeData, RcDom};

/// Converts an HTML document or fragment to Markdown. Headings, paragraphs,
/// lists, tables, links, images, emphasis, code and quotes are converted;
/// unsupported tags contribute their text; scripts and styles are dropped.
pub fn html_to_markdown(html: &str) -> String {
    let html = strip_cf_html(html);
    let dom = parse_document(RcDom::default(), ParseOpts::default())
        .from_utf8()
        .read_from(&mut html.as_bytes());
    let dom = match dom {
        Ok(d) => d,
        Err(_) => return String::new(),
    };
    let mut c = Conv::default();
    let blocks = c.blocks(&dom.document);
    let mut out = blocks.join("\n\n");
    while out.ends_with('\n') {
        out.pop();
    }
    out
}

/// Windows `CF_HTML` clipboard data starts with a `Version:…StartFragment:…`
/// header; keep only the fragment (or, failing that, everything from the first
/// tag).
pub fn strip_cf_html(s: &str) -> &str {
    if !s.starts_with("Version:") {
        return s;
    }
    let offset = |key: &str| -> Option<usize> {
        let i = s.find(key)? + key.len();
        let digits: String = s[i..].chars().take_while(char::is_ascii_digit).collect();
        digits.parse().ok()
    };
    if let (Some(a), Some(b)) = (offset("StartFragment:"), offset("EndFragment:"))
        && a <= b
        && b <= s.len()
        && s.is_char_boundary(a)
        && s.is_char_boundary(b)
    {
        return &s[a..b];
    }
    s.find('<').map_or(s, |i| &s[i..])
}

#[derive(Default)]
struct Conv {
    /// Depth of `<pre>`.
    pre: usize,
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

fn children(h: &Handle) -> Ref<'_, Vec<Handle>> {
    h.children.borrow()
}

const BLOCK_TAGS: &[&str] = &[
    "address",
    "article",
    "aside",
    "blockquote",
    "body",
    "center",
    "dd",
    "details",
    "dialog",
    "div",
    "dl",
    "dt",
    "fieldset",
    "figcaption",
    "figure",
    "footer",
    "form",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "header",
    "hr",
    "html",
    "li",
    "main",
    "nav",
    "ol",
    "p",
    "pre",
    "section",
    "summary",
    "table",
    "tbody",
    "thead",
    "tfoot",
    "tr",
    "ul",
];

const SKIP_TAGS: &[&str] = &[
    "script", "style", "head", "title", "meta", "link", "noscript", "template", "button",
];

fn is_block(h: &Handle) -> bool {
    tag(h).is_some_and(|t| BLOCK_TAGS.contains(&t.as_str()))
}

fn is_math(h: &Handle) -> Option<bool> {
    let t = tag(h)?;
    let class = attr(h, "class").unwrap_or_default();
    if t == "math" {
        return Some(attr(h, "display").as_deref() == Some("block"));
    }
    if class.split_whitespace().any(|c| c == "katex-display") {
        return Some(true);
    }
    if class
        .split_whitespace()
        .any(|c| c == "katex" || c == "MathJax")
    {
        return Some(false);
    }
    None
}

fn tex_annotation(h: &Handle) -> Option<String> {
    if tag(h).as_deref() == Some("annotation")
        && attr(h, "encoding").as_deref() == Some("application/x-tex")
    {
        let mut s = String::new();
        collect_text(h, &mut s);
        return Some(s.trim().to_owned());
    }
    children(h).iter().find_map(tex_annotation)
}

fn collect_text(h: &Handle, out: &mut String) {
    match &h.data {
        NodeData::Text { contents } => out.push_str(&contents.borrow()),
        _ => {
            for c in children(h).iter() {
                collect_text(c, out);
            }
        }
    }
}

/// Escapes Markdown-significant characters in text.
fn escape_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(c, '\\' | '*' | '_' | '`' | '[' | ']' | '<') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Escapes characters that would start a block at the beginning of a line.
fn escape_line_start(s: &str) -> String {
    let t = s.trim_start();
    let lead = &s[..s.len() - t.len()];
    let needs = t.starts_with('#')
        || t.starts_with("> ")
        || t.starts_with("- ")
        || t.starts_with("+ ")
        || t.starts_with("* ")
        || t.split_once(". ")
            .is_some_and(|(n, _)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()));
    if needs {
        match t.find(['#', '>', '-', '+', '*', '.']) {
            Some(i) if t.as_bytes()[i] == b'.' => format!("{lead}{}\\{}", &t[..i], &t[i..]),
            _ => format!("{lead}\\{t}"),
        }
    } else {
        s.to_owned()
    }
}

fn collapse_ws(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut ws = false;
    for c in s.chars() {
        if c.is_whitespace() {
            ws = true;
        } else {
            if ws {
                out.push(' ');
            }
            ws = false;
            out.push(c);
        }
    }
    if ws {
        out.push(' ');
    }
    out
}

fn prefix_lines(s: &str, first: &str, rest: &str) -> String {
    let mut out = String::new();
    for (i, line) in s.lines().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let p = if i == 0 { first } else { rest };
        if line.is_empty() {
            out.push_str(p.trim_end());
        } else {
            out.push_str(p);
            out.push_str(line);
        }
    }
    out
}

impl Conv {
    /// Converts children of `h` to Markdown blocks.
    fn blocks(&mut self, h: &Handle) -> Vec<String> {
        let mut out = Vec::new();
        let mut inline_buf = String::new();
        let flush = |buf: &mut String, out: &mut Vec<String>| {
            let t = buf.trim();
            if !t.is_empty() {
                let lines: Vec<String> = t
                    .lines()
                    .map(|l| escape_line_start(l.trim_start()))
                    .collect();
                out.push(lines.join("\n"));
            }
            buf.clear();
        };
        for c in children(h).iter() {
            if let Some(display) = is_math(c)
                && let Some(tex) = tex_annotation(c)
            {
                if display {
                    flush(&mut inline_buf, &mut out);
                    out.push(format!("$$\n{tex}\n$$"));
                } else {
                    inline_buf.push_str(&format!("${tex}$"));
                }
                continue;
            }
            match &c.data {
                NodeData::Element { .. } if is_block(c) => {
                    flush(&mut inline_buf, &mut out);
                    out.extend(self.block(c));
                }
                NodeData::Element { .. } => {
                    let t = tag(c).unwrap_or_default();
                    if SKIP_TAGS.contains(&t.as_str()) {
                        continue;
                    }
                    if t == "br" {
                        inline_buf.push_str("\\\n");
                        continue;
                    }
                    inline_buf.push_str(&self.inline(c));
                }
                NodeData::Text { contents } => {
                    let s = contents.borrow();
                    inline_buf.push_str(&escape_text(&collapse_ws(&s)));
                }
                NodeData::Document => out.extend(self.blocks(c)),
                _ => {}
            }
        }
        flush(&mut inline_buf, &mut out);
        out
    }

    fn block(&mut self, h: &Handle) -> Vec<String> {
        let t = tag(h).unwrap_or_default();
        match t.as_str() {
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                let level = t.as_bytes()[1] - b'0';
                let text = self.inline_children(h).replace('\n', " ");
                let text = text.trim();
                if text.is_empty() {
                    return Vec::new();
                }
                vec![format!("{} {}", "#".repeat(level as usize), text)]
            }
            "p" => {
                let text = self.inline_children(h);
                let text = text.trim();
                if text.is_empty() {
                    return Vec::new();
                }
                vec![
                    text.lines()
                        .map(|l| escape_line_start(l.trim_start()))
                        .collect::<Vec<_>>()
                        .join("\n"),
                ]
            }
            "hr" => vec!["---".to_owned()],
            "pre" => {
                let mut code = String::new();
                collect_text(h, &mut code);
                let lang = children(h)
                    .iter()
                    .find(|c| tag(c).as_deref() == Some("code"))
                    .and_then(|c| attr(c, "class"))
                    .or_else(|| attr(h, "class"))
                    .and_then(|cls| {
                        cls.split_whitespace().find_map(|c| {
                            c.strip_prefix("language-")
                                .or_else(|| c.strip_prefix("lang-"))
                                .map(str::to_owned)
                        })
                    })
                    .unwrap_or_default();
                let code = code.strip_suffix('\n').unwrap_or(&code);
                let fence = if code.contains("```") { "~~~" } else { "```" };
                vec![format!("{fence}{lang}\n{code}\n{fence}")]
            }
            "blockquote" => {
                let inner = self.blocks(h).join("\n\n");
                if inner.trim().is_empty() {
                    return Vec::new();
                }
                vec![prefix_lines(&inner, "> ", "> ")]
            }
            "ul" | "ol" => vec![self.list(h, t == "ol")],
            "table" => self.table(h).into_iter().collect(),
            "dl" => {
                let mut out = Vec::new();
                for c in children(h).iter() {
                    match tag(c).as_deref() {
                        Some("dt") => out.push(format!("**{}**", self.inline_children(c).trim())),
                        Some("dd") => out.extend(self.blocks(c)),
                        _ => {}
                    }
                }
                out
            }
            "li" | "tr" | "tbody" | "thead" | "tfoot" => self.blocks(h),
            _ => self.blocks(h),
        }
    }

    fn list(&mut self, h: &Handle, ordered: bool) -> String {
        let start: u64 = attr(h, "start").and_then(|s| s.parse().ok()).unwrap_or(1);
        let mut items = Vec::new();
        let mut n = start;
        for c in children(h).iter() {
            if tag(c).as_deref() != Some("li") {
                continue;
            }
            let marker = if ordered {
                format!("{n}.")
            } else {
                "-".to_owned()
            };
            n += 1;
            let mut inner = self.blocks(c).join("\n");
            // GitHub task lists: <input type=checkbox [checked]>
            if let Some(cb) = find_checkbox(c) {
                let mark = if cb { "[x] " } else { "[ ] " };
                inner = format!("{mark}{}", inner.trim_start());
            }
            let pad = " ".repeat(marker.len() + 1);
            items.push(prefix_lines(
                if inner.is_empty() { " " } else { &inner },
                &format!("{marker} "),
                &pad,
            ));
        }
        items.join("\n")
    }

    fn table(&mut self, h: &Handle) -> Option<String> {
        let mut rows: Vec<(Vec<String>, Vec<&'static str>)> = Vec::new();
        fn collect_rows(h: &Handle, out: &mut Vec<Handle>) {
            for c in children(h).iter() {
                match tag(c).as_deref() {
                    Some("tr") => out.push(c.clone()),
                    Some("thead" | "tbody" | "tfoot") => collect_rows(c, out),
                    _ => {}
                }
            }
        }
        let mut trs = Vec::new();
        collect_rows(h, &mut trs);
        for tr in &trs {
            let mut cells = Vec::new();
            let mut aligns = Vec::new();
            for c in children(tr).iter() {
                if !matches!(tag(c).as_deref(), Some("td" | "th")) {
                    continue;
                }
                let text = self
                    .inline_children(c)
                    .replace('\n', " ")
                    .replace('|', "\\|");
                cells.push(text.trim().to_owned());
                let style = attr(c, "style").unwrap_or_default().replace(' ', "");
                let al = attr(c, "align").unwrap_or_default();
                aligns.push(if al == "center" || style.contains("text-align:center") {
                    ":-:"
                } else if al == "right" || style.contains("text-align:right") {
                    "--:"
                } else if al == "left" || style.contains("text-align:left") {
                    ":--"
                } else {
                    "---"
                });
            }
            if !cells.is_empty() {
                rows.push((cells, aligns));
            }
        }
        let cols = rows.iter().map(|r| r.0.len()).max()?;
        let mut out = String::new();
        for (i, (cells, aligns)) in rows.iter().enumerate() {
            let mut line = String::from("|");
            for k in 0..cols {
                line.push(' ');
                line.push_str(cells.get(k).map_or("", String::as_str));
                line.push_str(" |");
            }
            out.push_str(&line);
            out.push('\n');
            if i == 0 {
                let mut d = String::from("|");
                for k in 0..cols {
                    d.push(' ');
                    d.push_str(aligns.get(k).copied().unwrap_or("---"));
                    d.push_str(" |");
                }
                out.push_str(&d);
                out.push('\n');
            }
        }
        Some(out.trim_end().to_owned())
    }

    fn inline_children(&mut self, h: &Handle) -> String {
        let mut s = String::new();
        for c in children(h).iter() {
            match &c.data {
                NodeData::Text { contents } => {
                    let t = contents.borrow();
                    if self.pre > 0 {
                        s.push_str(&t);
                    } else {
                        s.push_str(&escape_text(&collapse_ws(&t)));
                    }
                }
                NodeData::Element { .. } => {
                    if let Some(display) = is_math(c)
                        && let Some(tex) = tex_annotation(c)
                    {
                        s.push_str(&if display {
                            format!("$${tex}$$")
                        } else {
                            format!("${tex}$")
                        });
                        continue;
                    }
                    if is_block(c) {
                        let b = self.block(c).join("\n");
                        if !s.ends_with('\n') && !s.is_empty() {
                            s.push('\n');
                        }
                        s.push_str(&b);
                        s.push('\n');
                    } else {
                        s.push_str(&self.inline(c));
                    }
                }
                _ => {}
            }
        }
        s
    }

    fn wrap(&mut self, h: &Handle, mark: &str) -> String {
        let inner = self.inline_children(h);
        let t = inner.trim();
        if t.is_empty() {
            return inner;
        }
        let lead = &inner[..inner.len() - inner.trim_start().len()];
        let trail = &inner[inner.trim_end().len()..];
        format!("{lead}{mark}{t}{mark}{trail}")
    }

    fn inline(&mut self, h: &Handle) -> String {
        let t = tag(h).unwrap_or_default();
        if SKIP_TAGS.contains(&t.as_str()) {
            return String::new();
        }
        match t.as_str() {
            "strong" | "b" => self.wrap(h, "**"),
            "em" | "i" | "cite" | "dfn" => self.wrap(h, "*"),
            "del" | "s" | "strike" => self.wrap(h, "~~"),
            "code" | "kbd" | "samp" | "tt" => {
                let mut raw = String::new();
                collect_text(h, &mut raw);
                let raw = raw.replace('\n', " ");
                if raw.is_empty() {
                    return String::new();
                }
                let ticks = if raw.contains('`') { "``" } else { "`" };
                let pad = if raw.starts_with('`') || raw.ends_with('`') {
                    " "
                } else {
                    ""
                };
                if t == "kbd" {
                    format!("<kbd>{raw}</kbd>")
                } else {
                    format!("{ticks}{pad}{raw}{pad}{ticks}")
                }
            }
            "a" => {
                let text = self.inline_children(h);
                match attr(h, "href") {
                    Some(href) if !href.starts_with("javascript:") => {
                        let text = text.trim();
                        let text = if text.is_empty() { href.as_str() } else { text };
                        let href = if href.contains(' ') {
                            format!("<{href}>")
                        } else {
                            href.clone()
                        };
                        match attr(h, "title") {
                            Some(t) => format!("[{text}]({href} \"{}\")", t.replace('"', "\\\"")),
                            None => format!("[{text}]({href})"),
                        }
                    }
                    _ => text,
                }
            }
            "img" => {
                let src = attr(h, "src").unwrap_or_default();
                if src.is_empty() {
                    return String::new();
                }
                let alt = attr(h, "alt").unwrap_or_default().replace(['[', ']'], "");
                let src = if src.contains(' ') {
                    format!("<{src}>")
                } else {
                    src
                };
                format!("![{alt}]({src})")
            }
            "u" | "ins" => format!("<u>{}</u>", self.inline_children(h)),
            "sup" | "sub" | "mark" => format!("<{t}>{}</{t}>", self.inline_children(h)),
            "br" => "\\\n".to_owned(),
            "input" => String::new(),
            "pre" => {
                self.pre += 1;
                let s = self.inline_children(h);
                self.pre -= 1;
                s
            }
            _ => self.inline_children(h),
        }
    }
}

fn find_checkbox(li: &Handle) -> Option<bool> {
    for c in children(li).iter() {
        if tag(c).as_deref() == Some("input") && attr(c, "type").as_deref() == Some("checkbox") {
            return Some(attr(c, "checked").is_some());
        }
        if tag(c).as_deref() == Some("p")
            && let Some(b) = find_checkbox(c)
        {
            return Some(b);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_common_structures() {
        let html = r#"<h2>Title</h2><p>Some <b>bold</b>, <i>it</i>, <code>x</code> and <a href="https://e.com">link</a>.</p>
<ul><li>one</li><li>two<ul><li>nested</li></ul></li></ul><ol start="3"><li>three</li></ol>
<blockquote><p>quote</p></blockquote><pre><code class="language-rust">fn main() {}
</code></pre><table><tr><th>a</th><th align="right">b</th></tr><tr><td>1</td><td>2|3</td></tr></table>
<p>pic <img src="a.png" alt="alt"></p><script>alert(1)</script>"#;
        let md = html_to_markdown(html);
        let expected = "## Title\n\nSome **bold**, *it*, `x` and [link](https://e.com).\n\n- one\n- two\n  - nested\n\n3. three\n\n> quote\n\n```rust\nfn main() {}\n```\n\n| a | b |\n| --- | --: |\n| 1 | 2\\|3 |\n\npic ![alt](a.png)";
        assert_eq!(md, expected);
    }

    #[test]
    fn escapes_and_math() {
        assert_eq!(
            html_to_markdown("<p>1. not a list *x*</p>"),
            "1\\. not a list \\*x\\*"
        );
        let katex = r#"<p>E = <span class="katex"><math><semantics><annotation encoding="application/x-tex">mc^2</annotation></semantics></math></span></p>"#;
        assert_eq!(html_to_markdown(katex), "E = $mc^2$");
        assert_eq!(html_to_markdown("plain <b>text</b>"), "plain **text**");
    }

    #[test]
    fn windows_cf_html_header() {
        let frag = "<b>x</b>";
        let head = "Version:0.9\r\nStartHTML:000000097\r\nEndHTML:000000140\r\nStartFragment:000000131\r\nEndFragment:000000139\r\n";
        let body =
            format!("<html><body><!--StartFragment-->{frag}<!--EndFragment--></body></html>");
        let s = format!("{head}{body}");
        let a = s.find(frag).unwrap();
        let s = s
            .replace("000000131", &format!("{a:09}"))
            .replace("000000139", &format!("{:09}", a + frag.len()));
        assert_eq!(html_to_markdown(&s), "**x**");
        assert_eq!(html_to_markdown("Version:0.9 junk <p>y</p>"), "y");
    }

    #[test]
    fn tasks() {
        let html = r#"<ul><li><input type="checkbox" checked> done</li><li><input type="checkbox"> todo</li></ul>"#;
        assert_eq!(html_to_markdown(html), "- [x] done\n- [ ] todo");
    }
}
