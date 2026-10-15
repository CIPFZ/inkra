//! Source-mode layout (ED-121): one item per source line, monospace, syntax
//! highlighted from the Markdown AST.

use std::hash::{Hash, Hasher};
use std::ops::Range;

use epaint::text::{FontId, LayoutJob, LayoutSection, TextFormat, TextWrapping};
use epaint::{Color32, Stroke, vec2};
use inkra_doc::Doc;
use inkra_markdown::tokens::{TokenClass, tokens_in};

use crate::block::BlockLayout;
use crate::run::{RunSpec, SourceMap};
use crate::style::Style;

/// Formatting attributes contributed by a token (applied outer to inner).
#[derive(Debug, Clone, Copy, Default)]
struct Attr {
    color: Option<Color32>,
    bold: Option<bool>,
    italic: Option<bool>,
    strike: bool,
    bg: Option<Color32>,
}

fn attr(class: TokenClass, s: &Style) -> Attr {
    let c = &s.source;
    match class {
        TokenClass::HeadingMarker
        | TokenClass::Marker
        | TokenClass::ListMarker
        | TokenClass::QuoteMarker => Attr {
            color: Some(c.marker),
            ..Default::default()
        },
        TokenClass::Table | TokenClass::Rule => Attr {
            color: Some(c.marker),
            ..Default::default()
        },
        TokenClass::Heading(_) => Attr {
            color: Some(c.heading),
            bold: Some(true),
            ..Default::default()
        },
        TokenClass::Emphasis => Attr {
            italic: Some(true),
            ..Default::default()
        },
        TokenClass::Strong => Attr {
            bold: Some(true),
            ..Default::default()
        },
        TokenClass::StrongEmphasis => Attr {
            bold: Some(true),
            italic: Some(true),
            ..Default::default()
        },
        TokenClass::Strike => Attr {
            strike: true,
            ..Default::default()
        },
        TokenClass::Highlight => Attr {
            bg: Some(s.highlight_bg),
            ..Default::default()
        },
        TokenClass::Code | TokenClass::CodeBlock => Attr {
            color: Some(c.code),
            ..Default::default()
        },
        TokenClass::Fence => Attr {
            color: Some(c.marker),
            ..Default::default()
        },
        TokenClass::LinkText => Attr {
            color: Some(c.link),
            ..Default::default()
        },
        TokenClass::Url => Attr {
            color: Some(c.url),
            ..Default::default()
        },
        TokenClass::Html => Attr {
            color: Some(c.html),
            ..Default::default()
        },
        TokenClass::Entity => Attr {
            color: Some(c.html),
            ..Default::default()
        },
        TokenClass::Math => Attr {
            color: Some(c.math),
            ..Default::default()
        },
        TokenClass::FrontMatter | TokenClass::Definition | TokenClass::FootnoteRef => Attr {
            color: Some(c.meta),
            ..Default::default()
        },
    }
}

/// Row height of source mode.
pub fn row_height(style: &Style) -> f32 {
    (style.mono_size * 1.55).round()
}

/// Tokens of one line, relative to the line start, plus quote markers.
pub fn line_tokens(doc: &Doc, line: Range<usize>, text: &str) -> Vec<(Range<usize>, TokenClass)> {
    let mut out: Vec<(Range<usize>, TokenClass)> = tokens_in(doc.ast(), line.clone())
        .into_iter()
        .map(|t| (t.span.start - line.start..t.span.end - line.start, t.class))
        .collect();
    // `>` container prefixes are not stored in the AST: mark leading `>` runs of
    // lines that lie inside a block quote.
    let in_quote = in_block_quote(doc, line.start);
    if in_quote > 0 {
        let b = text.as_bytes();
        let mut i = 0;
        let mut seen = 0;
        while i < b.len() && seen < in_quote {
            match b[i] {
                b' ' | b'\t' => i += 1,
                b'>' => {
                    out.push((i..i + 1, TokenClass::QuoteMarker));
                    seen += 1;
                    i += 1;
                }
                _ => break,
            }
        }
    }
    out
}

fn in_block_quote(doc: &Doc, pos: usize) -> usize {
    let ast = doc.ast();
    let Some(i) = ast.index_at(pos) else { return 0 };
    let Some(r) = ast.get(i) else { return 0 };
    let rel = r.rel(pos);
    let mut depth = 0;
    for b in inkra_markdown::walk::path_at(std::slice::from_ref(r.block), rel) {
        if matches!(b.kind, inkra_markdown::BlockKind::BlockQuote { .. }) {
            depth += 1;
        }
    }
    depth
}

/// Hash key of a source line layout.
pub fn line_key(
    text: &str,
    tokens: &[(Range<usize>, TokenClass)],
    width: f32,
    wrap: bool,
    style: &Style,
) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    tokens.hash(&mut h);
    width.to_bits().hash(&mut h);
    wrap.hash(&mut h);
    style.version.hash(&mut h);
    if style.spellcheck {
        inkra_text::spell::generation().hash(&mut h);
    }
    h.finish()
}

/// Lays out one source line.
pub fn layout_line(
    text: &str,
    tokens: &[(Range<usize>, TokenClass)],
    width: f32,
    wrap: bool,
    style: &Style,
) -> BlockLayout {
    let mut bounds: Vec<usize> = vec![0, text.len()];
    for (r, _) in tokens {
        bounds.push(r.start.min(text.len()));
        bounds.push(r.end.min(text.len()));
    }
    bounds.sort_unstable();
    bounds.dedup();
    let rh = row_height(style);
    let mut job = LayoutJob {
        text: text.to_owned(),
        wrap: TextWrapping {
            max_width: if wrap { width.max(40.0) } else { f32::INFINITY },
            ..Default::default()
        },
        break_on_newline: true,
        ..Default::default()
    };
    for w in bounds.windows(2) {
        let (a, b) = (w[0], w[1]);
        if a >= b || !text.is_char_boundary(a) || !text.is_char_boundary(b) {
            continue;
        }
        let mut at = Attr::default();
        for (r, class) in tokens {
            if r.start <= a && b <= r.end {
                let x = attr(*class, style);
                at.color = x.color.or(at.color);
                at.bold = x.bold.or(at.bold);
                at.italic = x.italic.or(at.italic);
                at.strike |= x.strike;
                at.bg = x.bg.or(at.bg);
            }
        }
        let color = at.color.unwrap_or(style.fg);
        let format = TextFormat {
            font_id: FontId::new(
                style.mono_size,
                crate::fonts::family_of(
                    style.mono_font,
                    at.bold.unwrap_or(false),
                    at.italic.unwrap_or(false),
                ),
            ),
            color,
            background: at.bg.unwrap_or(Color32::TRANSPARENT),
            line_height: Some(rh),
            strikethrough: if at.strike {
                Stroke::new(1.0, color)
            } else {
                Stroke::NONE
            },
            ..Default::default()
        };
        job.sections.push(LayoutSection {
            leading_space: 0.0,
            byte_range: epaint::text::ByteIndex(a)..epaint::text::ByteIndex(b),
            format,
        });
    }
    if job.sections.is_empty() {
        // Empty line: one empty section so the row has the right height.
        job.sections.push(LayoutSection {
            leading_space: 0.0,
            byte_range: epaint::text::ByteIndex(0)..epaint::text::ByteIndex(0),
            format: TextFormat {
                font_id: FontId::new(
                    style.mono_size,
                    crate::fonts::family_of(style.mono_font, false, false),
                ),
                color: style.fg,
                line_height: Some(rh),
                ..Default::default()
            },
        });
    }
    let mut map = SourceMap::default();
    map.text(0..text.len(), 0..text.len());
    // ED-121: spelling in source mode too, outside code, URLs, math and HTML.
    if style.spellcheck {
        for r in inkra_text::spell::misspelled(text) {
            let skip = tokens.iter().any(|(t, c)| {
                t.start < r.end
                    && r.start < t.end
                    && matches!(
                        c,
                        TokenClass::Code
                            | TokenClass::Fence
                            | TokenClass::CodeBlock
                            | TokenClass::Url
                            | TokenClass::Html
                            | TokenClass::Math
                            | TokenClass::FrontMatter
                            | TokenClass::Entity
                            | TokenClass::Definition
                            | TokenClass::FootnoteRef
                    )
            });
            if !skip {
                map.squiggles.push(r);
            }
        }
    }
    BlockLayout {
        height: rh,
        runs: vec![RunSpec {
            job,
            pos: vec2(0.0, 0.0),
            map,
        }],
        decos: Vec::new(),
        hits: Vec::new(),
        caret_range: Some(0..text.len()),
        overlays: Vec::new(),
        pending_diagram: None,
    }
}
