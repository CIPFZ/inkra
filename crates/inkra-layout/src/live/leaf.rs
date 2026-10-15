//! Layout of live items: leaf blocks and their container decorations.

use std::ops::Range;

use epaint::text::{FontId, FontsView, LayoutJob};
use epaint::{Color32, Pos2, Rect, Stroke, pos2, vec2};
use inkra_doc::Doc;
use inkra_markdown::ast::{Align, Block, BlockKind, InlineKind};

use super::inline::{Builder, Fmt};
use super::{Frame, LeafKind, LiveItem, Marker};
use crate::block::{BlockLayout, Deco, Hit, HitAction, Overlay};
use crate::run::{RunSpec, SourceMap};
use crate::style::Style;

/// Display-only facts known to the host that are not in the source: loaded
/// image sizes (ED-090) and table column widths set by dragging (ED-084).
pub trait ImageSizes {
    /// Intrinsic size of the image `src` (as written in the document), if loaded.
    fn size(&self, src: &str) -> Option<[f32; 2]>;

    /// Column widths of the table starting at `table_start`, if the user resized it.
    fn column_widths(&self, table_start: usize) -> Option<Vec<f32>> {
        let _ = table_start;
        None
    }
}

/// No images known.
pub struct NoImages;

impl ImageSizes for NoImages {
    fn size(&self, _: &str) -> Option<[f32; 2]> {
        None
    }
}

/// Inputs of live layout.
pub struct LiveCx<'a> {
    /// The document.
    pub doc: &'a Doc,
    /// Style.
    pub style: &'a Style,
    /// Column width.
    pub width: f32,
    /// Caret (absolute).
    pub caret: usize,
    /// Space above the item.
    pub space_before: f32,
    /// Whether the previous item shares the outermost quote (bars stay continuous).
    pub quote_continues: usize,
    /// Image sizes.
    pub images: &'a dyn ImageSizes,
    /// syntect theme for code blocks.
    pub syntax_theme: &'a str,
    /// Automatic equation numbers by math block start (MD-008).
    pub math_numbers: &'a std::collections::HashMap<usize, u32>,
    /// Label → equation number for `\ref`/`\eqref`.
    pub math_refs: &'a [(String, String)],
    /// Footnote numbers and TOC headings.
    pub doc_info: &'a super::DocInfo,
}

const PLACEHOLDER: [f32; 2] = [240.0, 135.0];

fn galley_height(fonts: &mut FontsView<'_>, job: &LayoutJob) -> (f32, f32) {
    let g = fonts.layout_job(job.clone());
    let first = g.rows.first().map_or(g.size().y, |r| r.max_y() - r.min_y());
    (g.size().y, first)
}

/// The single image of a paragraph that contains only an image.
fn sole_image(b: &Block) -> Option<&inkra_markdown::Inline> {
    match b.inlines.as_slice() {
        [i] if matches!(i.kind, InlineKind::Image { .. }) => Some(i),
        _ => None,
    }
}

/// Lays out one live item.
pub fn layout(item: &LiveItem, cx: &LiveCx<'_>, fonts: &mut FontsView<'_>) -> BlockLayout {
    super::inline::set_math_refs(cx.math_refs);
    super::inline::set_footnotes(&cx.doc_info.footnotes);
    super::inline::set_spellcheck(cx.style.spellcheck);
    let style = cx.style;
    let text = cx.doc.text();
    let base = item.src.start;
    let em = style.size;
    let mut out = BlockLayout::default();
    let mut decos_after: Vec<Deco> = Vec::new();

    // Container indentation.
    let mut x = 0.0f32;
    let mut quote_xs = Vec::new();
    let mut marker_slots: Vec<(f32, &Frame)> = Vec::new();
    let mut fmt = Fmt::body(style);
    for f in &item.frames {
        match f {
            Frame::Quote { alert, .. } => {
                quote_xs.push((x, alert.as_ref().map(|(k, _)| *k)));
                x += style.quote_indent * em + 4.0;
                fmt.color = if alert.is_some() {
                    style.fg
                } else {
                    style.quote_fg
                };
            }
            Frame::Item { task, .. } => {
                marker_slots.push((x, f));
                x += style.list_indent * em;
                // MD-006: completed tasks are muted and/or struck through.
                if task.as_ref().is_some_and(|t| t.1) {
                    if style.task_done != 1 {
                        fmt.color = style.fg_muted;
                    }
                    if style.task_done >= 1 {
                        fmt.strike = true;
                    }
                }
            }
            Frame::Footnote { .. } => {
                marker_slots.push((x, f));
                x += 2.0 * em;
            }
        }
    }
    let w = (cx.width - x).max(40.0);
    let mut y = cx.space_before;
    let mut content_h;
    let first_row_h;

    // Alert title (first leaf of an alert).
    for f in &item.frames {
        if let Frame::Quote {
            alert: Some((kind, marker)),
            first: true,
        } = f
        {
            let idx = alert_index(*kind);
            let mut tf = fmt;
            tf.bold = true;
            tf.color = style.alerts[idx];
            let mut b = Builder::new(text, base, 0, Some(cx.caret), style, w);
            if cx.caret >= marker.start && cx.caret <= marker.end {
                b.push_src(marker.clone(), &tf);
            } else {
                let name = alert_title(*kind);
                b.push(
                    &format!("{} {name}", alert_icon(*kind)),
                    &tf,
                    Some(marker.clone()),
                    true,
                );
            }
            let (h, _) = galley_height(fonts, &b.job);
            out.runs.push(RunSpec {
                job: b.job,
                pos: vec2(x, y),
                map: b.map,
            });
            y += h + 0.25 * em;
        }
    }

    let leaf = item.block(cx.doc);
    // Inline `<img>` tags size themselves from the loaded images.
    if let Some((b, _)) = &leaf {
        let mut sizes = Vec::new();
        for i in &b.inlines {
            i.walk(&mut |n| {
                if matches!(n.kind, InlineKind::Html) {
                    let span = (n.span.start as isize + leaf.as_ref().map_or(0, |l| l.1)) as usize
                        ..(n.span.end as isize + leaf.as_ref().map_or(0, |l| l.1)) as usize;
                    let src = text.slice(span.start.min(text.len())..span.end.min(text.len()));
                    if src.starts_with("<img")
                        && let Some(s) = inkra_html::render::tag_attr(&src, "src")
                        && let Some(sz) = cx.images.size(&s)
                    {
                        sizes.push((s, sz));
                    }
                }
            });
        }
        super::inline::set_image_sizes(sizes);
    }
    match (item.kind, leaf) {
        (LeafKind::EmptyLine, _) | (_, None) => {
            let mut b = Builder::new(text, base, 0, Some(cx.caret), style, w);
            b.finish_empty(item.src.start, &fmt);
            let (h, fr) = galley_height(fonts, &b.job);
            content_h = h;
            first_row_h = fr;
            out.caret_range = Some(0..0);
            out.runs.push(RunSpec {
                job: b.job,
                pos: vec2(x, y),
                map: b.map,
            });
        }
        (LeafKind::Block, Some((b, delta))) => {
            let abs = |r: &Range<usize>| {
                (r.start as isize + delta) as usize..(r.end as isize + delta) as usize
            };
            let active = cx.caret >= item.src.start && cx.caret <= item.src.end;
            match &b.kind {
                BlockKind::Heading {
                    level,
                    marker,
                    closing,
                    setext,
                } => {
                    let hs = style.headings[(*level as usize).clamp(1, 6) - 1];
                    let mut hf = fmt;
                    hf.kind = style.heading_font;
                    hf.size = (em * hs.size).round();
                    hf.bold = hs.bold;
                    hf.line_height = (hf.size * 1.3).round();
                    if let Some(c) = hs.color {
                        hf.color = c;
                    }
                    let mut bl = Builder::new(text, base, delta, Some(cx.caret), style, w);
                    let content = b
                        .lines
                        .first()
                        .map(|l| abs(&l.span))
                        .unwrap_or(item.src.end..item.src.end);
                    // MD-002: heading numbers are drawn, never written; they
                    // precede the marker so the caret stays next to the text.
                    if let Some(n) = cx
                        .doc_info
                        .heading_number(item.src.start)
                        .or_else(|| cx.doc_info.heading_number(abs(&b.span).start))
                    {
                        let mut nf = hf;
                        nf.color = style.fg_muted;
                        bl.push(&format!("{n} "), &nf, None, true);
                    }
                    if active && !setext {
                        // `# ` marker in muted color (ED-002).
                        bl.push_marker(abs(marker).start..content.start.max(abs(marker).end), &hf);
                    }
                    bl.inlines(&b.inlines, &hf);
                    if active
                        && !setext
                        && let Some(c) = closing
                    {
                        let c = abs(c);
                        bl.push_marker(content.end.min(c.start)..c.end, &hf);
                    }
                    if *setext && active {
                        bl.push("\n", &hf, None, false);
                        bl.push_marker(abs(marker), &hf);
                    }
                    let content_end = b.lines.last().map_or(content.end, |l| abs(&l.span).end);
                    bl.finish_empty(content.start, &hf);
                    let (h, fr) = galley_height(fonts, &bl.job);
                    content_h = h;
                    first_row_h = fr;
                    out.caret_range = Some(content.start - base..content_end - base);
                    out.runs.push(RunSpec {
                        job: bl.job,
                        pos: vec2(x, y),
                        map: bl.map,
                    });
                    if hs.border_bottom {
                        let ly = y + content_h + 0.3 * em;
                        decos_after.push(Deco::Line {
                            a: pos2(x, ly),
                            b: pos2(x + w, ly),
                            stroke: Stroke::new(1.0, style.hr),
                        });
                        content_h += 0.3 * em + 1.0;
                    }
                }
                BlockKind::Paragraph | BlockKind::LinkDef { .. } | BlockKind::Toc => {
                    let mut pf = fmt;
                    if matches!(b.kind, BlockKind::LinkDef { .. } | BlockKind::Toc) {
                        pf.color = style.fg_muted;
                        pf.size = (em * 0.9).round();
                    }
                    if let Some(img) =
                        sole_image(b).filter(|_| matches!(b.kind, BlockKind::Paragraph))
                    {
                        let InlineKind::Image { dest, title, .. } = &img.kind else {
                            unreachable!()
                        };
                        let size = cx.images.size(dest).unwrap_or(PLACEHOLDER);
                        let scale = if style.image_fit_column {
                            (w / size[0]).min(1.0)
                        } else {
                            1.0
                        };
                        let r =
                            Rect::from_min_size(pos2(x, y), vec2(size[0] * scale, size[1] * scale));
                        out.decos.push(Deco::Image {
                            rect: r,
                            src: dest.clone(),
                        });
                        content_h = r.height();
                        first_row_h = content_h;
                        if style.image_titles
                            && let Some(t) = title.as_ref().filter(|t| !t.trim().is_empty())
                        {
                            // The title as a caption under the image.
                            let mut cf = fmt;
                            cf.size = (em * 0.85).round();
                            cf.color = style.fg_muted;
                            cf.italic = true;
                            let mut cb = Builder::new(
                                text,
                                base,
                                delta,
                                None,
                                style,
                                r.width().max(w.min(240.0)),
                            );
                            cb.push(t, &cf, None, true);
                            let (ch, _) = galley_height(fonts, &cb.job);
                            out.runs.push(RunSpec {
                                job: cb.job,
                                pos: vec2(x, y + content_h + 4.0),
                                map: SourceMap::default(),
                            });
                            content_h += ch + 4.0;
                        }
                        // ED-002: the source is editable below the image when the
                        // caret is on its line.
                        let mut bl = Builder::new(text, base, delta, Some(cx.caret), style, w);
                        if active {
                            let mut sf = fmt;
                            sf.mono = true;
                            sf.size = (em * 0.85).round();
                            sf.color = style.fg_muted;
                            bl.push_src(item.src.clone(), &sf);
                            let (h, _) = galley_height(fonts, &bl.job);
                            out.runs.push(RunSpec {
                                job: bl.job,
                                pos: vec2(x, y + content_h + 4.0),
                                map: bl.map,
                            });
                            content_h += h + 4.0;
                        } else {
                            // Clicking the image puts the caret after it.
                            let mut m = SourceMap::default();
                            m.atomic(0..0, item.src.len()..item.src.len());
                            out.runs.push(RunSpec {
                                job: empty_job(&fmt, w),
                                pos: vec2(r.max.x, r.max.y - fmt.line_height),
                                map: m,
                            });
                        }
                        out.caret_range = Some(0..item.src.len());
                    } else {
                        let mut bl = Builder::new(text, base, delta, Some(cx.caret), style, w);
                        if matches!(b.kind, BlockKind::Toc) {
                            // MD-013: the document's headings, each a link.
                            let depth = style.toc_depth.clamp(1, 6);
                            let heads: Vec<_> = cx
                                .doc_info
                                .headings
                                .iter()
                                .filter(|h| h.0 <= depth)
                                .collect();
                            let min = heads.iter().map(|h| h.0).min().unwrap_or(1);
                            let mut f = pf;
                            f.color = style.accent;
                            for (k, (lvl, t, anchor)) in heads.iter().enumerate() {
                                if k > 0 {
                                    bl.push("\n", &f, None, true);
                                }
                                let indent = "\u{2003}".repeat(usize::from(lvl - min));
                                bl.push(&indent, &f, None, true);
                                let s0 = bl.job.text.len();
                                bl.push(t, &f, None, true);
                                let s1 = bl.job.text.len();
                                bl.map
                                    .hits
                                    .push((s0..s1, HitAction::Link(format!("#{anchor}"))));
                            }
                            if heads.is_empty() {
                                let mut mf = pf;
                                mf.color = style.fg_muted;
                                bl.push("[TOC]", &mf, None, true);
                            }
                            // The caret enters the block at its end.
                            let end = item.src.end;
                            bl.push("", &pf, Some(end..end), true);
                        } else if matches!(b.kind, BlockKind::LinkDef { .. } | BlockKind::Toc) {
                            bl.push_src(item.src.clone(), &pf);
                        } else {
                            bl.inlines(&b.inlines, &pf);
                        }
                        let start = b
                            .lines
                            .first()
                            .map_or(item.src.start, |l| abs(&l.span).start);
                        bl.finish_empty(start, &pf);
                        // PR-034: body paragraphs (not list items or quotes' first
                        // lines after markers) start indented by two characters.
                        if style.first_line_indent
                            && matches!(b.kind, BlockKind::Paragraph)
                            && !item.frames.iter().any(|f| {
                                matches!(f, super::Frame::Item { .. } | super::Frame::Quote { .. })
                            })
                            && let Some(sec) = bl.job.sections.first_mut()
                        {
                            sec.leading_space = 2.0 * em;
                        }
                        let (h, fr) = galley_height(fonts, &bl.job);
                        content_h = h;
                        first_row_h = fr;
                        let end = b.lines.last().map_or(item.src.end, |l| abs(&l.span).end);
                        // ED-005: the caret never enters a TOC (edit it in source mode).
                        out.caret_range = if matches!(b.kind, BlockKind::Toc) {
                            None
                        } else {
                            Some(start - base..end - base)
                        };
                        out.runs.push(RunSpec {
                            job: bl.job,
                            pos: vec2(x, y),
                            map: bl.map,
                        });
                    }
                }
                BlockKind::CodeBlock { fence } => {
                    let lang = fence
                        .as_ref()
                        .and_then(|f| f.info.as_ref())
                        .map(|i| {
                            text.slice(abs(i))
                                .split_whitespace()
                                .next()
                                .unwrap_or_default()
                                .to_owned()
                        })
                        .unwrap_or_default();
                    let pad = style.code_padding;
                    // MD-071: a gutter of line numbers; MD-072: unwrapped code
                    // makes the box as wide as its longest line (it scrolls).
                    let gutter = if style.code_line_numbers {
                        let digits = b.lines.len().max(1).to_string().len() as f32;
                        (digits + 1.5) * style.mono_size * 0.6
                    } else {
                        0.0
                    };
                    let wrap_w = if style.code_wrap {
                        w - 2.0 * pad - gutter
                    } else {
                        f32::INFINITY
                    };
                    let (job, map, body) = code_job(cx, b, delta, &lang, wrap_w, base);
                    let is_diagram = style.diagrams && lang.eq_ignore_ascii_case("mermaid");
                    if is_diagram && !active {
                        // MD-100: the diagram instead of its source.
                        let src = text.slice(body.clone()).into_owned();
                        let (shown, err, ext, pending) = diagram(&src, style, w, item.src.start);
                        if pending {
                            out.pending_diagram = Some(item.src.start);
                        }
                        let top = y + 0.3 * em;
                        let mut h = 0.6 * em;
                        if let Some(p) = ext {
                            h += external_image(&p, cx, x, top, w, &mut out);
                        }
                        if let Some(ms) = shown {
                            let sc = ms.scene.clone();
                            let fx = x + ((w - sc.width) / 2.0).max(0.0);
                            out.decos.push(Deco::Math {
                                rect: Rect::from_min_size(pos2(fx, top), vec2(sc.width, sc.height)),
                                math: ms,
                            });
                            h += sc.height;
                        }
                        if let Some(e) = err {
                            let mut ef = fmt;
                            ef.size = (em * 0.8).round();
                            ef.color = Color32::from_rgb(220, 38, 38);
                            let mut bl = Builder::new(text, base, delta, None, style, w);
                            bl.push(&e, &ef, None, true);
                            let (eh, _) = galley_height(fonts, &bl.job);
                            out.runs.push(RunSpec {
                                job: bl.job,
                                pos: vec2(x, top + h - 0.3 * em),
                                map: bl.map,
                            });
                            h += eh + 4.0;
                        }
                        // Clicking puts the caret at the end of the code, which
                        // opens the source with a live preview.
                        let at = body.end - base;
                        let mut m = SourceMap::default();
                        m.atomic(0..0, at..at);
                        out.runs.push(RunSpec {
                            job: empty_job(&fmt, w),
                            pos: vec2(x + w - 1.0, top),
                            map: m,
                        });
                        content_h = h;
                        first_row_h = h;
                        out.caret_range = Some(at..at);
                    } else {
                        let (h, fr) = galley_height(fonts, &job);
                        let g = fonts.layout_job(job.clone());
                        let box_w = if style.code_wrap {
                            w
                        } else {
                            w.max(g.size().x + 2.0 * pad + gutter + 4.0)
                        };
                        let rect = Rect::from_min_size(pos2(x, y), vec2(box_w, h + 2.0 * pad));
                        if gutter > 0.0 {
                            let mut nums = String::new();
                            let mut n = 0;
                            let mut new_line = true;
                            for (k, r) in g.rows.iter().enumerate() {
                                if k > 0 {
                                    nums.push('\n');
                                }
                                if new_line {
                                    n += 1;
                                    nums.push_str(&n.to_string());
                                }
                                new_line = r.ends_with_newline;
                            }
                            let mut nf = Fmt::body(style);
                            nf.mono = true;
                            nf.size = style.mono_size;
                            nf.line_height = (style.mono_size * 1.5).round();
                            nf.color = style.line_numbers_fg;
                            let mut nb = Builder::new(text, base, delta, None, style, gutter);
                            nb.push(&nums, &nf, None, true);
                            let mut job = nb.job;
                            job.halign = epaint::emath::Align::RIGHT;
                            out.runs.push(RunSpec {
                                job,
                                pos: vec2(x + pad + gutter - 0.6 * style.mono_size, y + pad),
                                map: SourceMap::default(),
                            });
                        }
                        out.decos.push(Deco::Rect {
                            rect,
                            fill: style.code_bg,
                            stroke: Stroke::new(1.0, style.code_border),
                            radius: style.code_radius,
                        });
                        out.overlays.push(Overlay::Code {
                            rect,
                            lang,
                            info: fence.as_ref().and_then(|f| f.info.as_ref().map(abs)),
                            body: body.clone(),
                        });
                        out.runs.push(RunSpec {
                            job,
                            pos: vec2(x + pad + gutter, y + pad),
                            map,
                        });
                        content_h = rect.height();
                        first_row_h = fr + pad;
                        out.caret_range = Some(body.start - base..body.end - base);
                        if is_diagram {
                            // Live preview below the source while editing (MD-100).
                            let src = text.slice(body.clone()).into_owned();
                            let (shown, err, ext, pending) =
                                diagram(&src, style, w, item.src.start);
                            if pending {
                                out.pending_diagram = Some(item.src.start);
                            }
                            let top = y + content_h + 0.4 * em;
                            if let Some(p) = ext {
                                content_h += external_image(&p, cx, x, top, w, &mut out) + 0.6 * em;
                            }
                            if let Some(ms) = shown {
                                let sc = ms.scene.clone();
                                let fx = x + ((w - sc.width) / 2.0).max(0.0);
                                out.decos.push(Deco::Math {
                                    rect: Rect::from_min_size(
                                        pos2(fx, top),
                                        vec2(sc.width, sc.height),
                                    ),
                                    math: ms,
                                });
                                content_h += sc.height + 0.6 * em;
                            }
                            if let Some(e) = err {
                                let mut ef = fmt;
                                ef.size = (em * 0.8).round();
                                ef.color = Color32::from_rgb(220, 38, 38);
                                let mut bl = Builder::new(text, base, delta, None, style, w);
                                bl.push(&e, &ef, None, true);
                                let (eh, _) = galley_height(fonts, &bl.job);
                                out.runs.push(RunSpec {
                                    job: bl.job,
                                    pos: vec2(x, y + content_h + 2.0),
                                    map: bl.map,
                                });
                                content_h += eh + 6.0;
                            }
                        }
                    }
                }
                BlockKind::ThematicBreak => {
                    let ly = y + 0.6 * em;
                    out.decos.push(Deco::Line {
                        a: pos2(x, ly),
                        b: pos2(x + w, ly),
                        stroke: Stroke::new(2.0, style.hr),
                    });
                    content_h = 1.2 * em;
                    first_row_h = content_h;
                    out.caret_range = None;
                }
                BlockKind::MathBlock { open, close } if !active => {
                    // MD-008: the formula, centered, with its equation number.
                    let tex = math_source(text, abs(open), close.as_ref().map(abs), item.src.end);
                    let mut req = inkra_math::Request::new(
                        &tex,
                        true,
                        (em * super::inline::MATH_SCALE).round(),
                    );
                    req.number = cx.math_numbers.get(&item.src.start).copied();
                    req.refs = cx.math_refs.to_vec();
                    let r = inkra_math::render(&req);
                    let ms = super::inline::math_scene(&req, fmt.color);
                    let sc = &r.scene;
                    let top = y + 0.35 * em;
                    let fx = if sc.width <= w {
                        x + (w - sc.width) / 2.0
                    } else {
                        x
                    };
                    out.decos.push(Deco::Math {
                        rect: Rect::from_min_size(pos2(fx, top), vec2(sc.width, sc.height)),
                        math: ms,
                    });
                    if let Some(tag) = &r.tag {
                        let tx = x + w - tag.width;
                        let ty = top + sc.baseline - tag.baseline;
                        let key = ms_key(&req) ^ 0x7a67;
                        out.decos.push(Deco::Math {
                            rect: Rect::from_min_size(
                                pos2(tx.max(fx + sc.width + em), ty),
                                vec2(tag.width, tag.height),
                            ),
                            math: crate::run::MathScene {
                                scene: tag.clone(),
                                key,
                                color: fmt.color,
                            },
                        });
                    }
                    let mut h = sc.height + 0.7 * em;
                    if let Some(e) = r.errors.first() {
                        // Unsupported commands are shown in red in the formula; the
                        // first problem is spelled out below it.
                        let mut ef = fmt;
                        ef.size = (em * 0.8).round();
                        ef.color = Color32::from_rgb(220, 38, 38);
                        let mut bl = Builder::new(text, base, delta, None, style, w);
                        bl.push(e, &ef, None, true);
                        let (eh, _) = galley_height(fonts, &bl.job);
                        out.runs.push(RunSpec {
                            job: bl.job,
                            pos: vec2(x, top + sc.height + 2.0),
                            map: bl.map,
                        });
                        h += eh + 2.0;
                    }
                    // Clicking the formula puts the caret at the end of the block,
                    // which opens its source (ED-002).
                    let mut m = SourceMap::default();
                    m.atomic(0..0, item.src.len()..item.src.len());
                    out.runs.push(RunSpec {
                        job: empty_job(&fmt, w),
                        pos: vec2(x + w - 1.0, top),
                        map: m,
                    });
                    content_h = h;
                    first_row_h = h;
                    out.caret_range = Some(0..item.src.len());
                }
                BlockKind::HtmlBlock { .. }
                    if (!active || sole_img(&text.slice(item.src.clone())))
                        && !inkra_html::render::needs_source(&text.slice(item.src.clone())) =>
                {
                    // MD-014: the renderable HTML subset.
                    let html = text.slice(item.src.clone());
                    let blocks = inkra_html::render::flatten(&html);
                    // `<details>` keys start from the block's text (stable across
                    // edits elsewhere).
                    let mut seen = {
                        use std::hash::{Hash, Hasher};
                        let mut h = std::collections::hash_map::DefaultHasher::new();
                        html.hash(&mut h);
                        (h.finish() as usize) & 0xffff_ffff
                    };
                    let h = html_blocks(
                        &blocks,
                        cx,
                        fmt,
                        x,
                        y + 0.2 * em,
                        w,
                        fonts,
                        &mut out,
                        &mut seen,
                    );
                    // A closing tag alone (`</details>`) takes almost no room.
                    content_h = if blocks.is_empty() {
                        0.5 * em
                    } else {
                        h.max(fmt.line_height) + 0.4 * em
                    };
                    if active {
                        // An `<img>` alone (ED-093): the picture stays, its source
                        // is edited below it, like a Markdown image.
                        let mut sf = fmt;
                        sf.mono = true;
                        sf.size = (em * 0.85).round();
                        sf.color = style.fg_muted;
                        let mut bl = Builder::new(text, base, delta, Some(cx.caret), style, w);
                        bl.push_src(item.src.clone(), &sf);
                        let (sh, _) = galley_height(fonts, &bl.job);
                        out.runs.push(RunSpec {
                            job: bl.job,
                            pos: vec2(x, y + content_h),
                            map: bl.map,
                        });
                        content_h += sh + 4.0;
                    } else {
                        let mut m = SourceMap::default();
                        m.atomic(0..0, item.src.len()..item.src.len());
                        out.runs.push(RunSpec {
                            job: empty_job(&fmt, w),
                            pos: vec2(x + w - 1.0, y),
                            map: m,
                        });
                    }
                    first_row_h = fmt.line_height.min(content_h);
                    out.caret_range = Some(0..item.src.len());
                }
                BlockKind::HtmlBlock { .. }
                | BlockKind::FrontMatter { .. }
                | BlockKind::MathBlock { .. } => {
                    // Shown as their source in a quiet box (HTML rendering is MD-014,
                    // math rendering MD-008).
                    let mut sf = fmt;
                    sf.mono = true;
                    sf.size = (em * 0.88).round();
                    sf.line_height = (sf.size * 1.5).round();
                    sf.color = match b.kind {
                        BlockKind::MathBlock { .. } => style.source.math,
                        BlockKind::HtmlBlock { .. } => style.source.html,
                        _ => style.fg_muted,
                    };
                    let pad = style.code_padding * 0.75;
                    let mut bl =
                        Builder::new(text, base, delta, Some(cx.caret), style, w - 2.0 * pad);
                    let r = item.src.clone();
                    // Map line by line so caret movement works across line breaks.
                    let mut pos = r.start;
                    let s = text.slice(r.clone()).into_owned();
                    for (k, line) in s.split('\n').enumerate() {
                        if k > 0 {
                            bl.push("\n", &sf, Some(pos - 1..pos), true);
                        }
                        let l = line.strip_suffix('\r').unwrap_or(line);
                        bl.push_src(pos..pos + l.len(), &sf);
                        pos += line.len() + 1;
                    }
                    bl.finish_empty(r.start, &sf);
                    let (h, fr) = galley_height(fonts, &bl.job);
                    let rect = Rect::from_min_size(pos2(x, y), vec2(w, h + 2.0 * pad));
                    let fill = if matches!(b.kind, BlockKind::FrontMatter { .. }) {
                        style.code_bg
                    } else {
                        Color32::TRANSPARENT
                    };
                    out.decos.push(Deco::Rect {
                        rect,
                        fill,
                        stroke: Stroke::new(1.0, style.code_border),
                        radius: style.code_radius,
                    });
                    out.runs.push(RunSpec {
                        job: bl.job,
                        pos: vec2(x + pad, y + pad),
                        map: bl.map,
                    });
                    content_h = rect.height();
                    first_row_h = fr + pad;
                    out.caret_range = Some(0..item.src.len());
                    // While editing a formula its rendering is previewed below the
                    // source (MD-008).
                    if let BlockKind::MathBlock { open, close } = &b.kind {
                        let tex =
                            math_source(text, abs(open), close.as_ref().map(abs), item.src.end);
                        let mut req = inkra_math::Request::new(
                            &tex,
                            true,
                            (em * super::inline::MATH_SCALE).round(),
                        );
                        req.number = cx.math_numbers.get(&item.src.start).copied();
                        req.refs = cx.math_refs.to_vec();
                        let ms = super::inline::math_scene(&req, fmt.color);
                        let sc = ms.scene.clone();
                        let top = y + content_h + 0.4 * em;
                        let fx = if sc.width <= w {
                            x + (w - sc.width) / 2.0
                        } else {
                            x
                        };
                        out.decos.push(Deco::Math {
                            rect: Rect::from_min_size(pos2(fx, top), vec2(sc.width, sc.height)),
                            math: ms,
                        });
                        content_h += sc.height + 0.8 * em;
                    }
                }
                BlockKind::Table { aligns, .. } => {
                    let (h, fr, cols) = table(
                        cx,
                        b,
                        delta,
                        aligns,
                        x,
                        y,
                        w,
                        base,
                        item.src.start,
                        fonts,
                        &mut out,
                    );
                    let grid_w = out
                        .decos
                        .iter()
                        .filter_map(|d| match d {
                            Deco::Rect { rect, .. } => Some(rect.max.x),
                            _ => None,
                        })
                        .fold(x, f32::max);
                    out.overlays.push(Overlay::Table {
                        rect: Rect::from_min_max(pos2(x, y), pos2(grid_w, y + h)),
                        span: item.src.clone(),
                        cols: cols.0,
                        rows: cols.1,
                    });
                    content_h = h;
                    first_row_h = fr;
                }
                _ => {
                    let mut bl = Builder::new(text, base, delta, Some(cx.caret), style, w);
                    bl.push_src(item.src.clone(), &fmt);
                    bl.finish_empty(item.src.start, &fmt);
                    let (h, fr) = galley_height(fonts, &bl.job);
                    content_h = h;
                    first_row_h = fr;
                    out.caret_range = Some(0..item.src.len());
                    out.runs.push(RunSpec {
                        job: bl.job,
                        pos: vec2(x, y),
                        map: bl.map,
                    });
                }
            }
        }
    }

    // Container decorations.
    let height = y + content_h;
    for (k, (qx, alert)) in quote_xs.iter().enumerate() {
        let top = if k < cx.quote_continues {
            0.0
        } else {
            cx.space_before
        };
        let color = alert.map_or(style.quote_border, |a| style.alerts[alert_index(a)]);
        out.decos.push(Deco::Rect {
            rect: Rect::from_min_max(pos2(*qx, top), pos2(*qx + 3.0, height)),
            fill: color,
            stroke: Stroke::NONE,
            radius: 1.5,
        });
    }
    let center_y = cx.space_before + first_row_h * 0.5;
    for (mx, f) in marker_slots {
        match f {
            Frame::Item {
                marker,
                task,
                first: true,
                ..
            } => {
                if let Some((span, checked)) = task {
                    let s = (em * 0.85).round();
                    let r = Rect::from_center_size(pos2(mx + s * 0.6, center_y), vec2(s, s));
                    out.decos.push(Deco::Rect {
                        rect: r,
                        fill: if *checked {
                            style.accent
                        } else {
                            Color32::TRANSPARENT
                        },
                        stroke: Stroke::new(
                            1.5,
                            if *checked {
                                style.accent
                            } else {
                                style.fg_muted
                            },
                        ),
                        radius: 3.0,
                    });
                    if *checked {
                        out.decos.push(Deco::Line {
                            a: pos2(r.left() + s * 0.22, r.center().y),
                            b: pos2(r.left() + s * 0.42, r.bottom() - s * 0.25),
                            stroke: Stroke::new(2.0, style.bg),
                        });
                        out.decos.push(Deco::Line {
                            a: pos2(r.left() + s * 0.42, r.bottom() - s * 0.25),
                            b: pos2(r.right() - s * 0.2, r.top() + s * 0.25),
                            stroke: Stroke::new(2.0, style.bg),
                        });
                    }
                    out.hits.push(Hit {
                        rect: r.expand(3.0),
                        action: HitAction::ToggleTask(span.start),
                    });
                    // Shift the bullet left of the checkbox would crowd it: tasks show only the box.
                    continue;
                }
                match marker {
                    Marker::Bullet(depth) => {
                        let c = pos2(mx + em * 0.55, center_y);
                        let r = em * 0.17;
                        match depth % 3 {
                            0 => out.decos.push(Deco::Circle {
                                center: c,
                                radius: r,
                                fill: style.fg,
                                stroke: Stroke::NONE,
                            }),
                            1 => out.decos.push(Deco::Circle {
                                center: c,
                                radius: r,
                                fill: Color32::TRANSPARENT,
                                stroke: Stroke::new(1.2, style.fg),
                            }),
                            _ => out.decos.push(Deco::Rect {
                                rect: Rect::from_center_size(c, vec2(r * 1.8, r * 1.8)),
                                fill: style.fg,
                                stroke: Stroke::NONE,
                                radius: 0.0,
                            }),
                        }
                    }
                    Marker::Number(n, d) => {
                        let job = LayoutJob::simple_singleline(
                            format!("{n}{d}"),
                            FontId::new(
                                fmt.size,
                                crate::fonts::family_of(style.body_font, false, false),
                            ),
                            style.fg,
                        );
                        let g = fonts.layout_job(job.clone());
                        let nx = mx + style.list_indent * em - g.size().x - em * 0.35;
                        let ny = center_y - g.size().y * 0.5;
                        out.runs.push(RunSpec {
                            job,
                            pos: vec2(nx.max(mx), ny),
                            map: SourceMap::default(),
                        });
                    }
                }
            }
            Frame::Footnote {
                label, first: true, ..
            } => {
                let job = LayoutJob::simple_singleline(
                    match cx.doc_info.footnote(label) {
                        Some(n) => format!("{n}."),
                        None => format!("[^{label}]"),
                    },
                    FontId::new(
                        (fmt.size * 0.85).round(),
                        crate::fonts::family_of(style.body_font, false, false),
                    ),
                    style.fg_muted,
                );
                out.runs.push(RunSpec {
                    job,
                    pos: vec2(mx, center_y - fmt.size * 0.5),
                    map: SourceMap::default(),
                });
            }
            _ => {}
        }
    }
    out.decos.extend(decos_after);
    out.height = height;
    place_objects(&mut out, fonts);
    out
}

/// Text format of an HTML span.
fn span_fmt(sp: &inkra_html::render::Span, base: Fmt, style: &Style) -> Fmt {
    let mut f = base;
    f.bold |= sp.bold;
    f.italic |= sp.italic;
    f.underline |= sp.underline;
    f.strike |= sp.strike;
    if sp.code || sp.kbd {
        f.mono = true;
        f.size = (base.size * 0.88).round();
        f.bg = style.code_bg;
    }
    if sp.mark {
        f.bg = style.highlight_bg;
    }
    if sp.sup || sp.sub {
        f.size = (base.size * 0.75).round();
        f.valign = if sp.sup {
            epaint::emath::Align::TOP
        } else {
            epaint::emath::Align::BOTTOM
        };
    }
    if let Some(s) = sp.size {
        f.size = (base.size * s).round();
        f.line_height = f.line_height.max((f.size * 1.3).round());
    }
    if let Some([r, g, b]) = sp.color {
        f.color = Color32::from_rgb(r, g, b);
    }
    if let Some([r, g, b]) = sp.bg {
        f.bg = Color32::from_rgb(r, g, b);
    }
    if sp.link.is_some() {
        f.color = style.accent;
        f.underline = true;
    }
    f
}

/// Lays out flattened HTML blocks from `y`; returns the height used.
#[allow(clippy::too_many_arguments)]
fn html_blocks(
    blocks: &[inkra_html::render::HBlock],
    cx: &LiveCx<'_>,
    fmt: Fmt,
    x: f32,
    y: f32,
    w: f32,
    fonts: &mut FontsView<'_>,
    out: &mut BlockLayout,
    details_seen: &mut usize,
) -> f32 {
    use inkra_html::render::{Align as HAlign, HBlock};
    let style = cx.style;
    let em = style.size;
    let text = cx.doc.text();
    let mut cy = y;
    let spans_into =
        |bl: &mut Builder<'_>, spans: &[inkra_html::render::Span], base: Fmt, max_w: f32| {
            for sp in spans {
                let f = span_fmt(sp, base, style);
                if let Some((src, ww, hh)) = &sp.image {
                    let natural = cx.images.size(src);
                    let zoomed = sp
                        .zoom
                        .zip(natural)
                        .map(|(z, n)| (Some(n[0] * z), Some(n[1] * z)));
                    let (ww, hh) = zoomed.as_ref().map_or((ww, hh), |(a, b)| (a, b));
                    let (iw, ih) = match (ww, hh, natural) {
                        (Some(a), Some(b), _) => (*a, *b),
                        (Some(a), None, Some(n)) => (*a, a * n[1] / n[0].max(1.0)),
                        (None, Some(b), Some(n)) => (b * n[0] / n[1].max(1.0), *b),
                        (Some(a), None, None) => (*a, *a * 0.56),
                        (None, Some(b), None) => (*b * 1.6, *b),
                        (None, None, Some(n)) => (n[0], n[1]),
                        (None, None, None) => (96.0, 20.0),
                    };
                    let s = if iw > max_w { max_w / iw } else { 1.0 };
                    bl.push_image(src, iw * s, ih * s, &f);
                    continue;
                }
                let start = bl.job.text.len();
                bl.push(&sp.text, &f, None, true);
                if let Some(href) = &sp.link {
                    let end = bl.job.text.len();
                    bl.map
                        .hits
                        .push((start..end, HitAction::Link(href.clone())));
                }
            }
        };
    for b in blocks {
        match b {
            HBlock::Para {
                align,
                spans,
                heading,
                marker,
                indent,
            } => {
                let ix = f32::from(*indent) * 1.5 * em;
                let mut f = fmt;
                if let Some(l) = heading {
                    let hs = style.headings[usize::from((*l).clamp(1, 6)) - 1];
                    f.size = (em * hs.size).round();
                    f.bold = true;
                    f.kind = style.heading_font;
                    f.line_height = (f.size * 1.3).round();
                }
                let pw = (w - ix).max(40.0);
                let mut bl = Builder::new(text, 0, 0, None, style, pw);
                if let Some(m) = marker {
                    bl.push(&format!("{m} "), &f, None, true);
                }
                spans_into(&mut bl, spans, f, pw);
                let mut job = bl.job;
                let px = match align {
                    HAlign::Left => x + ix,
                    HAlign::Center => {
                        job.halign = epaint::emath::Align::Center;
                        x + ix + pw / 2.0
                    }
                    HAlign::Right => {
                        job.halign = epaint::emath::Align::RIGHT;
                        x + ix + pw
                    }
                };
                let (h, _) = galley_height(fonts, &job);
                out.runs.push(RunSpec {
                    job,
                    pos: vec2(px, cy),
                    map: bl.map,
                });
                cy += h + 0.4 * em;
            }
            HBlock::Pre(t) => {
                let mut f = fmt;
                f.mono = true;
                f.size = (em * 0.88).round();
                let mut bl = Builder::new(text, 0, 0, None, style, w - 2.0 * style.code_padding);
                bl.push(t, &f, None, true);
                let (h, _) = galley_height(fonts, &bl.job);
                let r = Rect::from_min_size(pos2(x, cy), vec2(w, h + 2.0 * style.code_padding));
                out.decos.push(Deco::Rect {
                    rect: r,
                    fill: style.code_bg,
                    stroke: Stroke::new(1.0, style.code_border),
                    radius: style.code_radius,
                });
                out.runs.push(RunSpec {
                    job: bl.job,
                    pos: vec2(x + style.code_padding, cy + style.code_padding),
                    map: bl.map,
                });
                cy += r.height() + 0.4 * em;
            }
            HBlock::Rule => {
                out.decos.push(Deco::Line {
                    a: pos2(x, cy + 0.5 * em),
                    b: pos2(x + w, cy + 0.5 * em),
                    stroke: Stroke::new(2.0, style.hr),
                });
                cy += em;
            }
            HBlock::Details {
                summary,
                open,
                body,
            } => {
                let mut f = fmt;
                f.bold = true;
                // MD-014: the summary folds and unfolds the body in the view.
                let key =
                    super::details_key(summary.iter().map(|s| s.text.as_str()), *details_seen);
                *details_seen += 1;
                let open = *open != super::details_toggled(key);
                let mut bl = Builder::new(text, 0, 0, None, style, w);
                bl.push(if open { "▼ " } else { "▶ " }, &f, None, true);
                spans_into(&mut bl, summary, f, w);
                let end = bl.job.text.len();
                bl.map.hits.push((0..end, HitAction::ToggleDetails(key)));
                let (h, _) = galley_height(fonts, &bl.job);
                out.runs.push(RunSpec {
                    job: bl.job,
                    pos: vec2(x, cy),
                    map: bl.map,
                });
                cy += h + 0.3 * em;
                if open {
                    cy += html_blocks(
                        body,
                        cx,
                        fmt,
                        x + 1.2 * em,
                        cy,
                        w - 1.2 * em,
                        fonts,
                        out,
                        details_seen,
                    );
                }
            }
            HBlock::Table(rows) => {
                for (ri, row) in rows.iter().enumerate() {
                    let mut bl = Builder::new(text, 0, 0, None, style, w);
                    for (ci, cell) in row.iter().enumerate() {
                        if ci > 0 {
                            let mut sep = fmt;
                            sep.color = style.table_border;
                            bl.push("  │  ", &sep, None, true);
                        }
                        let mut f = fmt;
                        f.bold |= ri == 0;
                        spans_into(&mut bl, cell, f, w);
                    }
                    let (h, _) = galley_height(fonts, &bl.job);
                    out.runs.push(RunSpec {
                        job: bl.job,
                        pos: vec2(x, cy),
                        map: bl.map,
                    });
                    cy += h + 0.2 * em;
                }
                cy += 0.2 * em;
            }
            HBlock::Media { kind, src } => {
                let r = Rect::from_min_size(pos2(x, cy), vec2(w.min(420.0), 2.6 * em));
                out.decos.push(Deco::Rect {
                    rect: r,
                    fill: style.code_bg,
                    stroke: Stroke::new(1.0, style.code_border),
                    radius: 6.0,
                });
                let mut f = fmt;
                f.color = style.fg_muted;
                let mut bl = Builder::new(text, 0, 0, None, style, r.width() - 24.0);
                bl.push(&format!("▶  {kind}  {src}"), &f, None, true);
                if !src.is_empty() {
                    bl.map
                        .hits
                        .push((0..bl.job.text.len(), HitAction::Link(src.clone())));
                }
                out.runs.push(RunSpec {
                    job: bl.job,
                    pos: vec2(x + 12.0, cy + 0.8 * em),
                    map: bl.map,
                });
                cy += r.height() + 0.4 * em;
            }
        }
    }
    cy - y
}

/// Diagram colors from the theme (MD-102).
fn diagram_palette(style: &Style) -> inkra_diagram::Palette {
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
    // MD-102: the theme's `[diagram]` colors win.
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

/// Converts a diagram to a paintable scene, scaled down to `max_w`.
fn to_math_scene(
    d: &inkra_diagram::Scene,
    max_w: f32,
    key: u64,
    dim: Option<Color32>,
) -> crate::run::MathScene {
    let s = if d.width > max_w && d.width > 0.0 {
        max_w / d.width
    } else {
        1.0
    };
    let ts = tiny_skia_path::Transform::from_scale(s, s);
    let fills = d
        .fills
        .iter()
        .filter_map(|f| {
            let path = f.path.clone().transform(ts)?;
            let [r, g, b, _] = f.color;
            let color = match dim {
                // A failed edit shows the last good diagram faded toward the page.
                Some(bg) => {
                    let m = |x: u8, y: u8| ((u16::from(x) + u16::from(y)) / 2) as u8;
                    [m(r, bg.r()), m(g, bg.g()), m(b, bg.b())]
                }
                None => [r, g, b],
            };
            Some(inkra_math::Fill {
                path,
                color: Some(color),
            })
        })
        .collect();
    let scene = inkra_math::Scene {
        width: d.width * s,
        height: d.height * s,
        baseline: 0.0,
        fills,
    };
    crate::run::MathScene {
        scene: std::sync::Arc::new(scene),
        key: key ^ u64::from(dim.is_some()),
        color: Color32::BLACK,
    }
}

type LastGood =
    std::sync::Mutex<std::collections::HashMap<usize, (u64, std::sync::Arc<inkra_diagram::Scene>)>>;
static LAST_GOOD: std::sync::OnceLock<LastGood> = std::sync::OnceLock::new();

/// The diagram of `src` and its error. On a parse error the last successful
/// rendering of the block (keyed by its position) is shown faded (MD-100).
fn diagram(
    src: &str,
    style: &Style,
    max_w: f32,
    block: usize,
) -> (
    Option<crate::run::MathScene>,
    Option<String>,
    Option<String>,
    bool,
) {
    let Some((shown, err)) = diagram_builtin(src, style, max_w, block) else {
        // AD-5: laid out in the background; the previous rendering of the
        // block meanwhile.
        let prev = LAST_GOOD
            .get_or_init(Default::default)
            .lock()
            .ok()
            .and_then(|m| m.get(&block).cloned());
        return match prev {
            Some((k, sc)) => (Some(to_math_scene(&sc, max_w, k, None)), None, None, true),
            None => (None, Some("Drawing the diagram…".to_owned()), None, true),
        };
    };
    // Types the built-in renderer lacks go to `mmdc` when configured.
    if shown.is_none()
        && let Some(e) = &err
        && e.contains("is not supported")
        && let Some(st) = inkra_diagram::external::request(src, style.dark)
    {
        return match st {
            inkra_diagram::external::Status::Ready(p) => {
                (None, None, Some(p.to_string_lossy().into_owned()), false)
            }
            inkra_diagram::external::Status::Pending => {
                (None, Some("Rendering with mmdc…".to_owned()), None, false)
            }
            inkra_diagram::external::Status::Failed(e) => {
                (None, Some(format!("mmdc: {e}")), None, false)
            }
        };
    }
    (shown, err, None, false)
}

/// An image rendered by an external tool, scaled to the column; returns its height.
fn external_image(
    path: &str,
    cx: &LiveCx<'_>,
    x: f32,
    top: f32,
    w: f32,
    out: &mut BlockLayout,
) -> f32 {
    let size = cx.images.size(path).unwrap_or([w.min(480.0), 240.0]);
    let s = if size[0] > w { w / size[0] } else { 1.0 };
    let (iw, ih) = (size[0] * s, size[1] * s);
    out.decos.push(Deco::Image {
        rect: Rect::from_min_size(pos2(x + (w - iw) / 2.0, top), vec2(iw, ih)),
        src: path.to_owned(),
    });
    ih
}

/// `None` while the diagram is laid out in the background (AD-5).
fn diagram_builtin(
    src: &str,
    style: &Style,
    max_w: f32,
    block: usize,
) -> Option<(Option<crate::run::MathScene>, Option<String>)> {
    use std::hash::{Hash, Hasher};
    let pal = diagram_palette(style);
    let r = match inkra_diagram::background::poll(block, src, &pal) {
        inkra_diagram::background::Poll::Ready(r) => r,
        inkra_diagram::background::Poll::Pending => return None,
    };
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (src, &pal, max_w.to_bits()).hash(&mut h);
    let key = h.finish();
    let last = LAST_GOOD.get_or_init(Default::default);
    match r.as_ref() {
        Ok(sc) => {
            let arc = std::sync::Arc::new(sc.clone());
            if let Ok(mut m) = last.lock() {
                if m.len() > 256 {
                    m.clear();
                }
                m.insert(block, (key, arc));
            }
            Some((Some(to_math_scene(sc, max_w, key, None)), None))
        }
        Err(e) => {
            let prev = last.lock().ok().and_then(|m| m.get(&block).cloned());
            let shown = prev.map(|(k, sc)| to_math_scene(&sc, max_w, k, Some(style.bg)));
            Some((shown, Some(e.to_string())))
        }
    }
}

/// Whether an HTML block is a single `<img>` tag.
fn sole_img(src: &str) -> bool {
    let t = src.trim();
    t.starts_with("<img") && t.ends_with('>') && t.matches('<').count() == 1
}

/// Height of the galley row containing char `ci`.
fn gl_row_h(g: &epaint::Galley, ci: usize) -> f32 {
    let mut k = 0usize;
    for r in &g.rows {
        let n = r.row.glyphs.len() + usize::from(r.ends_with_newline);
        if ci < k + n {
            return r.row.size.y;
        }
        k += n;
    }
    0.0
}

/// The TeX source between `$$` delimiters.
fn math_source(
    text: &inkra_text::Text,
    open: Range<usize>,
    close: Option<Range<usize>>,
    end: usize,
) -> String {
    let stop = close.map_or(end, |c| c.start);
    if stop <= open.end {
        return String::new();
    }
    text.slice(open.end..stop).trim().to_owned()
}

fn ms_key(req: &inkra_math::Request) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    req.hash(&mut h);
    h.finish()
}

/// Turns the inline objects of every run into decorations at the positions
/// the text layout reserved for them.
pub(crate) fn place_objects(out: &mut BlockLayout, fonts: &mut FontsView<'_>) {
    let mut decos = Vec::new();
    let mut hits = Vec::new();
    for run in &out.runs {
        if run.map.objects.is_empty() && run.map.hits.is_empty() && run.map.squiggles.is_empty() {
            continue;
        }
        let g = fonts.layout_job(run.job.clone());
        // Inline hit regions: the union of their glyph boxes.
        for (range, action) in &run.map.hits {
            let (a, b) = (
                crate::run::char_index(&run.job.text, range.start),
                crate::run::char_index(&run.job.text, range.end),
            );
            let mut k = 0usize;
            let mut rect = Rect::NOTHING;
            for r in &g.rows {
                for gl in &r.row.glyphs {
                    if k >= a && k < b {
                        let min = pos2(run.pos.x + r.pos.x + gl.pos.x, run.pos.y + r.pos.y);
                        rect = rect.union(Rect::from_min_size(
                            min,
                            vec2(gl.advance_width.max(4.0), r.row.size.y),
                        ));
                    }
                    k += 1;
                }
                if r.ends_with_newline {
                    k += 1;
                }
            }
            if rect.is_positive() {
                hits.push(Hit {
                    rect: rect.expand(2.0),
                    action: action.clone(),
                });
            }
        }
        // Misspellings: a wavy line under each row part of the word.
        for range in &run.map.squiggles {
            let (a, b) = (
                crate::run::char_index(&run.job.text, range.start),
                crate::run::char_index(&run.job.text, range.end),
            );
            let mut k = 0usize;
            for r in &g.rows {
                let mut seg: Option<(f32, f32, f32)> = None;
                for gl in &r.row.glyphs {
                    if k >= a && k < b {
                        let x0 = run.pos.x + r.pos.x + gl.pos.x;
                        let y = run.pos.y + r.pos.y + gl.pos.y + 2.5;
                        seg = Some(match seg {
                            None => (x0, x0 + gl.advance_width, y),
                            Some((s, _, yy)) => (s, x0 + gl.advance_width, yy),
                        });
                    }
                    k += 1;
                }
                if r.ends_with_newline {
                    k += 1;
                }
                if let Some((x0, x1, y)) = seg {
                    decos.push(Deco::Squiggle {
                        a: pos2(x0, y),
                        b: pos2(x1, y),
                        color: Color32::from_rgb(215, 58, 73),
                    });
                }
            }
        }
        for obj in &run.map.objects {
            let ci = crate::run::char_index(&run.job.text, obj.at);
            // Walk rows: each glyph is one char; a row ending in `\n` skips it.
            let mut k = 0usize;
            let mut found = None;
            'rows: for r in &g.rows {
                let mut prev_y = None;
                for gl in &r.row.glyphs {
                    if k == ci {
                        found = Some((r.pos, gl.pos, prev_y));
                        break 'rows;
                    }
                    prev_y = Some(gl.pos.y);
                    k += 1;
                }
                if r.ends_with_newline {
                    k += 1;
                }
            }
            let Some((rp, gp, prev_y)) = found else {
                continue;
            };
            let w = f32::from_bits(obj.width_bits);
            // The tall reserving glyph has its own baseline near the row top; the
            // text's baseline is the one of the plain joiner before it.
            let text_y = match &obj.kind {
                crate::run::ObjectKind::Math(_) => prev_y.unwrap_or(gp.y),
                crate::run::ObjectKind::Image { .. } => gp.y,
            };
            let baseline = run.pos.y + rp.y + text_y;
            let left = run.pos.x + rp.x + gp.x - w;
            match &obj.kind {
                crate::run::ObjectKind::Math(m) => {
                    let sc = &m.scene;
                    decos.push(Deco::Math {
                        rect: Rect::from_min_size(
                            pos2(left, baseline - sc.baseline),
                            vec2(sc.width, sc.height),
                        ),
                        math: m.clone(),
                    });
                }
                crate::run::ObjectKind::Image { src, size } => {
                    // Bottom-aligned in its row (the row grew to fit it).
                    let (iw, ih) = (f32::from_bits(size[0]), f32::from_bits(size[1]));
                    let bottom = (run.pos.y + rp.y + gl_row_h(&g, ci)).max(baseline);
                    decos.push(Deco::Image {
                        rect: Rect::from_min_size(pos2(left, bottom - ih - 1.0), vec2(iw, ih)),
                        src: src.clone(),
                    });
                }
            }
        }
    }
    out.decos.extend(decos);
    out.hits.extend(hits);
}

fn empty_job(fmt: &Fmt, w: f32) -> LayoutJob {
    let text = inkra_text::Text::new();
    let style = Style::default();
    let mut b = Builder::new(&text, 0, 0, None, &style, w);
    b.finish_empty(0, fmt);
    b.job
}

fn alert_index(k: inkra_markdown::ast::AlertKind) -> usize {
    use inkra_markdown::ast::AlertKind::*;
    match k {
        Note => 0,
        Tip => 1,
        Important => 2,
        Warning => 3,
        Caution => 4,
    }
}

fn alert_title(k: inkra_markdown::ast::AlertKind) -> &'static str {
    use inkra_markdown::ast::AlertKind::*;
    match k {
        Note => "Note",
        Tip => "Tip",
        Important => "Important",
        Warning => "Warning",
        Caution => "Caution",
    }
}

fn alert_icon(k: inkra_markdown::ast::AlertKind) -> &'static str {
    use inkra_markdown::ast::AlertKind::*;
    match k {
        Note => "ℹ",
        Tip => "💡",
        Important => "❗",
        Warning => "⚠",
        Caution => "⛔",
    }
}

/// Code block job: highlighted with syntect when the language is known.
fn code_job(
    cx: &LiveCx<'_>,
    b: &Block,
    delta: isize,
    lang: &str,
    w: f32,
    base: usize,
) -> (LayoutJob, SourceMap, Range<usize>) {
    let style = cx.style;
    let text = cx.doc.text();
    let abs =
        |r: &Range<usize>| (r.start as isize + delta) as usize..(r.end as isize + delta) as usize;
    let mut f = Fmt::body(style);
    f.mono = true;
    f.size = style.mono_size;
    f.line_height = (style.mono_size * 1.5).round();
    f.color = style.fg;
    let mut bl = Builder::new(text, base, delta, None, style, w);
    // Code text and per-line source positions.
    let mut code = String::new();
    let mut lines = Vec::new();
    for (k, l) in b.lines.iter().enumerate() {
        if k > 0 {
            code.push('\n');
        }
        let r = abs(&l.span);
        lines.push((code.len(), r.clone()));
        code.push_str(&text.slice(r));
    }
    let body = match (b.lines.first(), b.lines.last()) {
        (Some(a), Some(z)) => abs(&a.span).start..abs(&z.span).end,
        _ => {
            // Empty code block: the caret sits on the line after the opening fence.
            let open_end = match &b.kind {
                BlockKind::CodeBlock { fence: Some(fe) } => abs(&fe.open).end,
                _ => abs(&b.span).end,
            };
            let at = text
                .line_range(text.byte_to_line(open_end))
                .end
                .min(abs(&b.span).end.max(open_end));
            at..at
        }
    };
    let spans = inkra_highlight::highlight(&code, lang, cx.syntax_theme);
    for (k, (cstart, r)) in lines.iter().enumerate() {
        if k > 0 {
            let prev_end = lines[k - 1].1.end;
            bl.push("\n", &f, Some(prev_end..r.start), true);
        }
        let line_text = &code[*cstart..*cstart + r.len()];
        match &spans {
            Some(sp) => {
                // Split the line by highlight spans.
                let mut at = 0;
                for s in sp
                    .iter()
                    .filter(|s| s.range.end > *cstart && s.range.start < cstart + r.len())
                {
                    let a = s.range.start.max(*cstart) - cstart;
                    let z = s.range.end.min(cstart + r.len()) - cstart;
                    if a > at {
                        bl.push(
                            &line_text[at..a],
                            &f,
                            Some(r.start + at..r.start + a),
                            false,
                        );
                    }
                    let mut sf = f;
                    sf.color = Color32::from_rgba_unmultiplied(
                        s.color.0[0],
                        s.color.0[1],
                        s.color.0[2],
                        s.color.0[3],
                    );
                    sf.bold = s.bold;
                    sf.italic = s.italic;
                    let seg = line_text[a..z].trim_end_matches('\n');
                    bl.push(seg, &sf, Some(r.start + a..r.start + a + seg.len()), false);
                    at = a + seg.len();
                }
                if at < line_text.len() {
                    bl.push(&line_text[at..], &f, Some(r.start + at..r.end), false);
                }
            }
            None => bl.push(line_text, &f, Some(r.clone()), false),
        }
    }
    bl.finish_empty(body.start, &f);
    (bl.job, bl.map, body)
}

/// A simple grid for GFM tables (full table editing is ED-080..085, M4).
#[allow(clippy::too_many_arguments)]
fn table(
    cx: &LiveCx<'_>,
    b: &Block,
    delta: isize,
    aligns: &[Align],
    x: f32,
    y: f32,
    w: f32,
    base: usize,
    table_start: usize,
    fonts: &mut FontsView<'_>,
    out: &mut BlockLayout,
) -> (f32, f32, (Vec<f32>, Vec<f32>)) {
    let style = cx.style;
    let text = cx.doc.text();
    let ncols = aligns.len().max(1);
    let pad_x = 8.0;
    let pad_y = 4.0;
    let fmt = Fmt::body(style);
    // Lay out every cell without wrapping to measure columns.
    let mut cells: Vec<Vec<(LayoutJob, SourceMap, f32, f32)>> = Vec::new();
    let mut col_w = vec![60.0f32; ncols];
    for (ri, row) in b.children.iter().enumerate() {
        let mut out_row = Vec::new();
        for (ci, cw) in col_w.iter_mut().enumerate() {
            let mut cf = fmt;
            cf.bold = ri == 0;
            let mut bl = Builder::new(text, base, delta, Some(cx.caret), style, f32::INFINITY);
            let at = match row.children.get(ci) {
                Some(cell) => {
                    bl.inlines(&cell.inlines, &cf);
                    (cell.span.start as isize + delta) as usize
                }
                None => (row.span.end as isize + delta) as usize,
            };
            bl.finish_empty(at, &cf);
            let g = fonts.layout_job(bl.job.clone());
            *cw = cw.max(g.size().x + 2.0 * pad_x);
            out_row.push((bl.job, bl.map, g.size().x, g.size().y));
        }
        cells.push(out_row);
    }
    // ED-080: a column is at most as wide as the editor column (its cells wrap);
    // a table wider than the column scrolls horizontally in the view. Widths set
    // by dragging a column border win (ED-084, display only).
    match cx
        .images
        .column_widths(table_start)
        .filter(|v| v.len() == col_w.len())
    {
        Some(v) => {
            for (c, d) in col_w.iter_mut().zip(v) {
                *c = d.clamp(40.0, w.max(60.0) * 3.0);
            }
        }
        None => {
            for c in &mut col_w {
                *c = c.min(w.max(60.0));
            }
        }
    }
    // Wrapped cells are taller: measure them at their column width.
    for row in &mut cells {
        for (ci, cell) in row.iter_mut().enumerate() {
            let inner = col_w[ci] - 2.0 * pad_x;
            if cell.2 > inner + 0.5 {
                let mut job = cell.0.clone();
                job.wrap.max_width = inner;
                let g = fonts.layout_job(job);
                cell.2 = g.size().x;
                cell.3 = g.size().y;
            }
        }
    }
    let mut ty = y;
    let mut row_hs = Vec::new();
    let mut first_row = 0.0;
    for (ri, row) in cells.into_iter().enumerate() {
        let row_h = row.iter().map(|c| c.3).fold(fmt.line_height, f32::max) + 2.0 * pad_y;
        row_hs.push(row_h);
        if ri > 0 && ri % 2 == 0 {
            out.decos.push(Deco::Rect {
                rect: Rect::from_min_size(pos2(x, ty), vec2(col_w.iter().sum(), row_h)),
                fill: style.table_row_alt_bg,
                stroke: Stroke::NONE,
                radius: 0.0,
            });
        }
        if ri == 0 {
            first_row = row_h;
            out.decos.push(Deco::Rect {
                rect: Rect::from_min_size(pos2(x, ty), vec2(col_w.iter().sum(), row_h)),
                fill: style.table_header_bg,
                stroke: Stroke::NONE,
                radius: 0.0,
            });
        }
        let mut cx_ = x;
        for (ci, (mut job, map, tw, _)) in row.into_iter().enumerate() {
            let cw = col_w[ci];
            job.wrap.max_width = cw - 2.0 * pad_x;
            let off = match aligns.get(ci) {
                Some(Align::Center) => ((cw - 2.0 * pad_x - tw) / 2.0).max(0.0),
                Some(Align::Right) => (cw - 2.0 * pad_x - tw).max(0.0),
                _ => 0.0,
            };
            out.runs.push(RunSpec {
                job,
                pos: vec2(cx_ + pad_x + off, ty + pad_y),
                map,
            });
            cx_ += cw;
        }
        ty += row_h;
        out.decos.push(Deco::Line {
            a: pos2(x, ty),
            b: pos2(x + col_w.iter().sum::<f32>(), ty),
            stroke: Stroke::new(1.0, style.table_border),
        });
    }
    // Grid lines.
    let tw: f32 = col_w.iter().sum();
    out.decos.push(Deco::Rect {
        rect: Rect::from_min_max(pos2(x, y), pos2(x + tw, ty)),
        fill: Color32::TRANSPARENT,
        stroke: Stroke::new(1.0, style.table_border),
        radius: style.radius.min(4.0),
    });
    let mut gx = x;
    for c in &col_w[..col_w.len() - 1] {
        gx += c;
        out.decos.push(Deco::Line {
            a: pos2(gx, y),
            b: pos2(gx, ty),
            stroke: Stroke::new(1.0, style.table_border),
        });
    }
    out.caret_range = Some(0..(b.span.end - b.span.start));
    let _ = Pos2::ZERO;
    (ty - y, first_row, (col_w, row_hs))
}
