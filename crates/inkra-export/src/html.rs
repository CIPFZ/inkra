//! HTML export (EX-001) and the HTML used by "copy as HTML" (EX-006).

use std::fmt::Write;
use std::path::{Path, PathBuf};

use inkra_layout::Style;
use inkra_layout::epaint::Color32;
use inkra_layout::fonts::FontKind;
use inkra_markdown::html::{HtmlHooks, HtmlOptions};

use crate::svg;

/// How image paths are written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ImagePaths {
    /// As written in the document.
    #[default]
    Keep,
    /// Absolute `file://` URLs.
    Absolute,
    /// Embedded as base64 data URLs (self-contained file).
    Embed,
}

/// How formulas are written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MathOutput {
    /// Inline SVG (default).
    #[default]
    Svg,
    /// The TeX source in `$…$` / `$$…$$` for pages that run MathJax/KaTeX.
    Tex,
    /// MathML presentation markup.
    MathMl,
}

/// HTML export options.
#[derive(Debug, Clone, PartialEq)]
pub struct HtmlExport {
    /// A complete document with the theme's styles (`false`: a fragment, EX-001
    /// "HTML（不含样式）").
    pub styled: bool,
    /// Style the export after this theme.
    pub style: Style,
    /// Image paths.
    pub images: ImagePaths,
    /// Formulas.
    pub math: MathOutput,
    /// Syntax-highlight code blocks with inline colors.
    pub highlight: bool,
    /// Render ```` ```mermaid ```` blocks as SVG.
    pub diagrams: bool,
    /// The document file (for relative paths and the default title).
    pub doc_path: Option<PathBuf>,
    /// Extra HTML in `<head>`, after the styles.
    pub head_html: String,
    /// Extra HTML before `</body>`.
    pub foot_html: String,
    /// Extra CSS.
    pub custom_css: String,
    /// Soft breaks become `<br>` (MD-079).
    pub soft_break_as_br: bool,
}

impl Default for HtmlExport {
    fn default() -> Self {
        Self {
            styled: true,
            style: Style::default(),
            images: ImagePaths::Keep,
            math: MathOutput::Svg,
            highlight: true,
            diagrams: true,
            doc_path: None,
            head_html: String::new(),
            foot_html: String::new(),
            custom_css: String::new(),
            soft_break_as_br: false,
        }
    }
}

struct Hooks<'a> {
    o: &'a HtmlExport,
}

fn css_color(c: Color32) -> String {
    if c.a() == 255 {
        format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b())
    } else {
        // Color32 is premultiplied.
        let a = f32::from(c.a()) / 255.0;
        let un = |v: u8| {
            if a > 0.0 {
                ((f32::from(v) / a).round() as u32).min(255)
            } else {
                0
            }
        };
        format!("rgba({},{},{},{:.3})", un(c.r()), un(c.g()), un(c.b()), a)
    }
}

fn escape(s: &str) -> String {
    let mut out = String::new();
    inkra_markdown::html::escape(s, &mut out);
    out
}

/// Base64 encoding (RFC 4648).
pub fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (u32::from(c[0]) << 16)
            | (u32::from(*c.get(1).unwrap_or(&0)) << 8)
            | u32::from(*c.get(2).unwrap_or(&0));
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 {
            T[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if c.len() > 2 {
            T[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

fn mime_of(p: &Path) -> &'static str {
    match p
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("svg") => "image/svg+xml",
        Some("bmp") => "image/bmp",
        _ => "application/octet-stream",
    }
}

/// Resolves an image destination against the document folder (local files only).
pub fn resolve_image(doc: Option<&Path>, dest: &str) -> Option<PathBuf> {
    if dest.contains("://") && !dest.starts_with("file://") || dest.starts_with("data:") {
        return None;
    }
    let d = dest.strip_prefix("file://").unwrap_or(dest);
    let d = percent_decode(d);
    let p = PathBuf::from(&d);
    if p.is_absolute() {
        return Some(p);
    }
    // `file:///C:/x` on Windows.
    if d.len() > 2 && d.as_bytes()[0] == b'/' && d.as_bytes()[2] == b':' {
        return Some(PathBuf::from(&d[1..]));
    }
    Some(doc.and_then(Path::parent).unwrap_or(Path::new(".")).join(p))
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && i + 2 < b.len()
            && let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16)
        {
            out.push(v);
            i += 3;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

impl HtmlHooks for Hooks<'_> {
    fn code_block(&self, lang: &str, code: &str) -> Option<String> {
        if self.o.diagrams && lang.eq_ignore_ascii_case("mermaid") {
            let pal = diagram_palette(&self.o.style);
            return Some(match inkra_diagram::render(code, &pal).as_ref() {
                Ok(sc) => format!(
                    "<figure class=\"diagram\">{}</figure>",
                    svg::diagram_svg(sc)
                ),
                Err(e) => format!(
                    "<pre class=\"diagram-error\"><code>{}</code></pre><p class=\"diagram-error\">{}</p>",
                    escape(code),
                    escape(&e.to_string())
                ),
            });
        }
        if !self.o.highlight || lang.is_empty() {
            return None;
        }
        // Exports run off the UI thread or in the CLI: wait for the syntaxes.
        inkra_highlight::load_now();
        let spans = inkra_highlight::highlight(code, lang, &self.o.style.syntax_theme)?;
        let mut out = format!("<pre><code class=\"language-{}\">", escape(lang));
        let mut at = 0;
        for s in spans.iter() {
            if s.range.start > at {
                out.push_str(&escape(&code[at..s.range.start]));
            }
            let [r, g, b, _] = s.color.0;
            let mut st = format!("color:#{r:02x}{g:02x}{b:02x}");
            if s.bold {
                st.push_str(";font-weight:bold");
            }
            if s.italic {
                st.push_str(";font-style:italic");
            }
            let _ = write!(
                out,
                "<span style=\"{st}\">{}</span>",
                escape(&code[s.range.clone()])
            );
            at = s.range.end;
        }
        if at < code.len() {
            out.push_str(&escape(&code[at..]));
        }
        out.push_str("</code></pre>");
        Some(out)
    }

    fn math(&self, tex: &str, display: bool) -> Option<String> {
        match self.o.math {
            MathOutput::Tex => Some(if display {
                format!("$${}$$", escape(tex))
            } else {
                format!("${}$", escape(tex))
            }),
            MathOutput::MathMl => Some(inkra_math::mathml::to_mathml(tex, display)),
            MathOutput::Svg => {
                let r = inkra_math::render(&inkra_math::Request::new(
                    tex,
                    display,
                    if display { 19.0 } else { 17.0 },
                ));
                let mut s = svg::math_svg(&r.scene, tex);
                if display && let Some(t) = &r.tag {
                    s = format!(
                        "<span class=\"math-body\">{s}</span><span class=\"math-tag\">{}</span>",
                        svg::math_svg(t, "")
                    );
                }
                Some(s)
            }
        }
    }

    fn image_src(&self, dest: &str) -> Option<String> {
        match self.o.images {
            ImagePaths::Keep => None,
            ImagePaths::Absolute => {
                let p = resolve_image(self.o.doc_path.as_deref(), dest)?;
                let s = p.to_string_lossy().replace('\\', "/");
                Some(if s.starts_with('/') {
                    format!("file://{s}")
                } else {
                    format!("file:///{s}")
                })
            }
            ImagePaths::Embed => {
                let p = resolve_image(self.o.doc_path.as_deref(), dest)?;
                let bytes = std::fs::read(&p).ok()?;
                Some(format!("data:{};base64,{}", mime_of(&p), base64(&bytes)))
            }
        }
    }

    fn toc(&self, headings: &[(u8, String, String)]) -> Option<String> {
        if headings.is_empty() {
            return Some(String::new());
        }
        let min = headings.iter().map(|h| h.0).min().unwrap_or(1);
        let mut out = String::from("<nav class=\"toc\"><ul>");
        let mut level = min;
        let mut first = true;
        for (l, text, anchor) in headings {
            while level < *l {
                out.push_str("<ul>");
                level += 1;
            }
            while level > *l {
                out.push_str("</li></ul>");
                level -= 1;
            }
            if !first {
                out.push_str("</li>");
            }
            first = false;
            let _ = write!(
                out,
                "<li><a href=\"#{}\">{}</a>",
                escape(anchor),
                escape(text)
            );
        }
        while level > min {
            out.push_str("</li></ul>");
            level -= 1;
        }
        out.push_str("</li></ul></nav>");
        Some(out)
    }
}

/// Diagram colors from a theme (MD-102).
pub fn diagram_palette(style: &Style) -> inkra_diagram::Palette {
    let c = |c: Color32| [c.r(), c.g(), c.b(), 255];
    let mix = |a: Color32, b: Color32, t: f32| {
        let m = |x: u8, y: u8| (f32::from(x) * (1.0 - t) + f32::from(y) * t).round() as u8;
        [m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()), 255]
    };
    let mut p = if style.dark {
        inkra_diagram::Palette::dark()
    } else {
        inkra_diagram::Palette::light()
    };
    p.fg = c(style.fg);
    p.bg = c(style.bg);
    p.node_fill = mix(style.bg, style.accent, if style.dark { 0.28 } else { 0.1 });
    p.node_stroke = mix(style.bg, style.accent, 0.75);
    p.edge = mix(style.bg, style.fg, 0.8);
    p.lifeline = c(style.fg_muted);
    p.cluster_fill = c(style.code_bg);
    p.cluster_stroke = c(style.code_border);
    p.label_bg = c(style.code_bg);
    p.pie_text = c(style.fg);
    let [primary, border, line, text] = style.diagram_colors;
    if let Some(v) = primary {
        p.node_fill = c(v);
    }
    if let Some(v) = border {
        p.node_stroke = c(v);
    }
    if let Some(v) = line {
        p.edge = c(v);
        p.lifeline = c(v);
    }
    if let Some(v) = text {
        p.fg = c(v);
        p.pie_text = c(v);
    }
    p
}

/// The CSS font stack of a face; installed families come first by name.
fn font_stack(k: FontKind, name: Option<&str>) -> String {
    let base = match k {
        FontKind::System(_) => font_stack_of(FontKind::Sans),
        k => font_stack_of(k),
    };
    match name {
        Some(n) => format!("\"{}\", {base}", n.replace('"', "")),
        None => base.to_owned(),
    }
}

fn font_stack_of(k: FontKind) -> &'static str {
    match k {
        FontKind::System(_) => font_stack_of(FontKind::Sans),
        FontKind::Sans => {
            r#"Inter, system-ui, -apple-system, "Segoe UI", "PingFang SC", "Microsoft YaHei", "Noto Sans CJK SC", sans-serif"#
        }
        FontKind::Serif => {
            r#""Source Serif 4", Georgia, "Songti SC", "SimSun", "Noto Serif CJK SC", serif"#
        }
        FontKind::Mono => r#""JetBrains Mono", ui-monospace, Menlo, Consolas, monospace"#,
    }
}

/// The theme as CSS for exported documents.
pub fn theme_css(s: &Style) -> String {
    let c = css_color;
    let mut css = String::new();
    let _ = write!(
        css,
        r#"
:root {{ color-scheme: {scheme}; }}
html {{ background: {bg}; }}
body {{ margin: 0 auto; max-width: {maxw}px; padding: {py}px {px}px; color: {fg}; background: {bg}; font-family: {body}; font-size: {size}px; line-height: {lh}; }}
h1, h2, h3, h4, h5, h6 {{ font-family: {heading}; line-height: 1.3; }}
a {{ color: {accent}; }}
code, pre, kbd {{ font-family: {mono}; font-size: 0.88em; }}
code {{ background: {code_bg}; padding: 0.1em 0.3em; border-radius: 4px; }}
pre {{ background: {code_bg}; border: 1px solid {code_border}; border-radius: {radius}px; padding: {cpad}px; overflow-x: auto; line-height: 1.45; }}
pre code {{ background: none; padding: 0; font-size: 1em; }}
kbd {{ border: 1px solid {code_border}; border-bottom-width: 2px; border-radius: 4px; padding: 0 0.3em; background: {code_bg}; }}
blockquote {{ margin: 1em 0; padding: 0 1em; color: {quote_fg}; border-left: 4px solid {quote_border}; }}
hr {{ border: none; border-top: 2px solid {hr}; margin: 1.6em 0; }}
table {{ border-collapse: collapse; margin: 1em 0; display: block; overflow-x: auto; }}
th, td {{ border: 1px solid {table_border}; padding: 6px 12px; }}
th {{ background: {table_header_bg}; }}
tr:nth-child(2n) td {{ background: {table_alt}; }}
img {{ max-width: 100%; border-radius: {img_radius}px; }}
mark {{ background: {highlight}; }}
ul.contains-task-list, li.task-list-item {{ list-style: none; }}
.math-display {{ text-align: center; margin: 1em 0; overflow-x: auto; display: flex; justify-content: center; align-items: center; position: relative; }}
.math-display .math-tag {{ position: absolute; right: 0; }}
figure.diagram {{ margin: 1em 0; text-align: center; overflow-x: auto; }}
.diagram-error {{ color: #d73a49; }}
nav.toc ul {{ list-style: none; padding-left: 1.2em; }}
nav.toc > ul {{ padding-left: 0; }}
.footnotes {{ font-size: 0.9em; color: {muted}; }}
.markdown-alert {{ padding: 0.5em 1em; margin: 1em 0; border-left: 4px solid; border-radius: 4px; }}
.markdown-alert-title {{ font-weight: 600; margin: 0.2em 0; }}
"#,
        scheme = if s.dark { "dark" } else { "light" },
        bg = c(s.bg),
        fg = c(s.fg),
        maxw = s.max_width.round(),
        py = s.padding_y.round(),
        px = s.padding_x.round(),
        body = font_stack(s.body_font, s.font_names[0].as_deref()),
        heading = font_stack(s.heading_font, s.font_names[1].as_deref()),
        mono = font_stack(s.mono_font, s.font_names[2].as_deref()),
        size = s.size,
        lh = s.line_height,
        accent = c(s.accent),
        code_bg = c(s.code_bg),
        code_border = c(s.code_border),
        radius = s.code_radius,
        cpad = s.code_padding,
        quote_fg = c(s.quote_fg),
        quote_border = c(s.quote_border),
        hr = c(s.hr),
        table_border = c(s.table_border),
        table_header_bg = c(s.table_header_bg),
        table_alt = c(s.table_row_alt_bg),
        highlight = c(s.highlight_bg),
        muted = c(s.fg_muted),
        img_radius = s.image_radius,
    );
    for (i, name) in ["note", "tip", "important", "warning", "caution"]
        .iter()
        .enumerate()
    {
        let col = c(s.alerts[i]);
        let _ = writeln!(
            css,
            ".markdown-alert-{name} {{ border-left-color: {col}; }} .markdown-alert-{name} .markdown-alert-title {{ color: {col}; }}"
        );
    }
    for (i, h) in s.headings.iter().enumerate() {
        let lvl = i + 1;
        let _ = writeln!(
            css,
            "h{lvl} {{ font-size: {:.3}em; margin: {:.2}em 0 {:.2}em; {}{} }}",
            h.size,
            h.margin_top,
            h.margin_bottom,
            if h.border_bottom {
                format!(
                    "border-bottom: 1px solid {}; padding-bottom: 0.3em;",
                    c(s.hr)
                )
            } else {
                String::new()
            },
            h.color
                .map(|col| format!(" color: {};", c(col)))
                .unwrap_or_default(),
        );
    }
    css
}

/// Keeps only the rules the body needs (EX-001 "仅输出文档实际用到的样式").
fn used_css(css: &str, body: &str) -> String {
    let needs = |line: &str| -> bool {
        let sel = line.trim_start();
        let sel = sel.split('{').next().unwrap_or("").trim();
        let first = sel.split([',', ' ', ':', '.', '>']).next().unwrap_or("");
        let want: Option<String> = if let Some(rest) = sel.strip_prefix(".markdown-alert-") {
            Some(format!(
                "markdown-alert-{}",
                rest.split([' ', '{', '.']).next().unwrap_or("")
            ))
        } else if sel.starts_with(".markdown-alert") {
            Some("markdown-alert".into())
        } else if sel.starts_with(".math") {
            Some("math-display".into())
        } else if sel.starts_with("figure.diagram") || sel.starts_with(".diagram") {
            Some("diagram".into())
        } else if sel.starts_with("nav.toc") {
            Some("class=\"toc\"".into())
        } else if sel.starts_with(".footnotes") {
            Some("footnotes".into())
        } else if sel.starts_with("ul.contains-task-list") {
            Some("task-list".into())
        } else {
            match first {
                "table" | "th" | "tr" => Some("<table".into()),
                "blockquote" => Some("<blockquote".into()),
                "hr" => Some("<hr".into()),
                "img" => Some("<img".into()),
                "mark" => Some("<mark".into()),
                "kbd" => Some("<kbd".into()),
                "pre" => Some("<pre".into()),
                h if h.len() == 2 && h.starts_with('h') && sel.len() > 2 && !sel.contains(',') => {
                    Some(format!("<{h}"))
                }
                _ => None,
            }
        };
        want.is_none_or(|w| body.contains(&w))
    };
    css.lines()
        .filter(|l| needs(l))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The document title: front matter `title:`, else the first heading, else the
/// file name.
pub fn title_of(src: &str, doc_path: Option<&Path>) -> String {
    if let Some(rest) = src.strip_prefix("---")
        && let Some(end) = rest.find("\n---")
    {
        for l in rest[..end].lines() {
            if let Some(t) = l.strip_prefix("title:") {
                let t = t.trim().trim_matches(['"', '\'']);
                if !t.is_empty() {
                    return t.to_owned();
                }
            }
        }
    }
    let p = inkra_markdown::parse(src, &Default::default());
    if let Some(h) = inkra_markdown::walk::headings(src, &p.blocks)
        .into_iter()
        .next()
    {
        return h.text;
    }
    doc_path
        .and_then(|p| p.file_stem())
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Untitled".into())
}

/// Renders `src` to HTML.
pub fn export_html(src: &str, parse: &inkra_markdown::ParseOptions, o: &HtmlExport) -> String {
    let hooks = Hooks { o };
    let ho = HtmlOptions {
        heading_ids: true,
        soft_break_as_br: o.soft_break_as_br,
        tagfilter: true,
        hooks: Some(&hooks),
    };
    let body = inkra_markdown::html::to_html_with(src, parse, &ho);
    if !o.styled {
        return body;
    }
    let title = title_of(src, o.doc_path.as_deref());
    let mut out = String::new();
    let _ = write!(
        out,
        "<!DOCTYPE html>\n<html>\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n<meta name=\"generator\" content=\"Inkra {}\">\n<title>{}</title>\n<style>{}{}</style>\n{}</head>\n<body>\n<article class=\"markdown-body\">\n{}</article>\n{}</body>\n</html>\n",
        env!("CARGO_PKG_VERSION"),
        escape(&title),
        used_css(&theme_css(&o.style), &body),
        o.custom_css,
        o.head_html,
        body,
        o.foot_html
    );
    out
}

/// HTML for the clipboard (EX-006): a fragment with highlighting and SVG math.
pub fn clipboard_html(
    src: &str,
    parse: &inkra_markdown::ParseOptions,
    style: &Style,
    doc_path: Option<&Path>,
) -> String {
    let o = HtmlExport {
        styled: false,
        style: style.clone(),
        doc_path: doc_path.map(Path::to_owned),
        images: ImagePaths::Absolute,
        ..Default::default()
    };
    export_html(src, parse, &o)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exports_structure_math_code_and_toc() {
        inkra_highlight::load_now();
        let src = "---\ntitle: My Doc\n---\n# Intro\n\n[TOC]\n\n## Part A\n\n$$\nx^2\n$$\n\n```rust\nfn main() {}\n```\n\n```mermaid\npie\n\"a\" : 1\n```\n\n> [!NOTE]\n> Hi\n\nText[^1] $a$.\n\n[^1]: Note.\n";
        let parse = inkra_markdown::ParseOptions {
            inline_math: true,
            ..Default::default()
        };
        let html = export_html(src, &parse, &HtmlExport::default());
        assert!(html.starts_with("<!DOCTYPE html>"));
        assert!(html.contains("<title>My Doc</title>"));
        assert!(html.contains("<h1 id=\"intro\">"));
        assert!(html.contains("<nav class=\"toc\">") && html.contains("href=\"#part-a\""));
        assert!(
            html.contains("class=\"inkra-math\""),
            "display and inline math as SVG"
        );
        assert!(html.contains("<span style=\"color:#"), "highlighted code");
        assert!(html.contains("class=\"inkra-diagram\""));
        assert!(html.contains("markdown-alert-note"));
        assert!(html.contains("footnote"));
        assert!(
            !html.contains("title: My Doc"),
            "front matter is not exported"
        );
        let plain = export_html(
            src,
            &parse,
            &HtmlExport {
                styled: false,
                ..Default::default()
            },
        );
        assert!(!plain.contains("<style>"));
        let mml = export_html(
            src,
            &parse,
            &HtmlExport {
                math: MathOutput::MathMl,
                ..Default::default()
            },
        );
        assert!(
            mml.contains("<math xmlns=\"http://www.w3.org/1998/Math/MathML\" display=\"block\">")
        );
        let tex = export_html(
            src,
            &parse,
            &HtmlExport {
                math: MathOutput::Tex,
                ..Default::default()
            },
        );
        assert!(tex.contains("$$x^2$$"));
    }

    #[test]
    fn images_embed_and_absolute() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("a.png"), [137, 80, 78, 71]).unwrap();
        let doc = d.path().join("doc.md");
        let src = "![x](a.png)";
        let o = HtmlExport {
            styled: false,
            images: ImagePaths::Embed,
            doc_path: Some(doc.clone()),
            ..Default::default()
        };
        assert!(
            export_html(src, &Default::default(), &o)
                .contains("src=\"data:image/png;base64,iVBORw==\"")
        );
        let o = HtmlExport {
            styled: false,
            images: ImagePaths::Absolute,
            doc_path: Some(doc),
            ..Default::default()
        };
        assert!(export_html(src, &Default::default(), &o).contains("file://"));
        assert_eq!(base64(b"Man"), "TWFu");
        assert_eq!(base64(b"Ma"), "TWE=");
    }
}
