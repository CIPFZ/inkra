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
                if task.as_ref().is_some_and(|t| t.1) {
                    fmt.color = style.fg_muted;
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
                        let InlineKind::Image { dest, .. } = &img.kind else {
                            unreachable!()
                        };
                        let size = cx.images.size(dest).unwrap_or(PLACEHOLDER);
                        let scale = (w / size[0]).min(1.0);
                        let r =
                            Rect::from_min_size(pos2(x, y), vec2(size[0] * scale, size[1] * scale));
                        out.decos.push(Deco::Image {
                            rect: r,
                            src: dest.clone(),
                        });
                        content_h = r.height();
                        first_row_h = content_h;
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
                        if matches!(b.kind, BlockKind::LinkDef { .. } | BlockKind::Toc) {
                            bl.push_src(item.src.clone(), &pf);
                        } else {
                            bl.inlines(&b.inlines, &pf);
                        }
                        let start = b
                            .lines
                            .first()
                            .map_or(item.src.start, |l| abs(&l.span).start);
                        bl.finish_empty(start, &pf);
                        let (h, fr) = galley_height(fonts, &bl.job);
                        content_h = h;
                        first_row_h = fr;
                        let end = b.lines.last().map_or(item.src.end, |l| abs(&l.span).end);
                        out.caret_range = Some(start - base..end - base);
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
                    let (job, map, body) = code_job(cx, b, delta, &lang, w - 2.0 * pad, base);
                    let is_diagram = style.diagrams && lang.eq_ignore_ascii_case("mermaid");
                    if is_diagram && !active {
                        // MD-100: the diagram instead of its source.
                        let src = text.slice(body.clone()).into_owned();
                        let (shown, err) = diagram(&src, style, w, item.src.start);
                        let top = y + 0.3 * em;
                        let mut h = 0.6 * em;
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
                        let rect = Rect::from_min_size(pos2(x, y), vec2(w, h + 2.0 * pad));
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
                            pos: vec2(x + pad, y + pad),
                            map,
                        });
                        content_h = rect.height();
                        first_row_h = fr + pad;
                        out.caret_range = Some(body.start - base..body.end - base);
                        if is_diagram {
                            // Live preview below the source while editing (MD-100).
                            let src = text.slice(body.clone()).into_owned();
                            let (shown, err) = diagram(&src, style, w, item.src.start);
                            let top = y + content_h + 0.4 * em;
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
                    format!("[^{label}]"),
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
) -> (Option<crate::run::MathScene>, Option<String>) {
    use std::hash::{Hash, Hasher};
    let pal = diagram_palette(style);
    let r = inkra_diagram::render(src, &pal);
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
            (Some(to_math_scene(sc, max_w, key, None)), None)
        }
        Err(e) => {
            let prev = last.lock().ok().and_then(|m| m.get(&block).cloned());
            let shown = prev.map(|(k, sc)| to_math_scene(&sc, max_w, k, Some(style.bg)));
            (shown, Some(e.to_string()))
        }
    }
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
fn place_objects(out: &mut BlockLayout, fonts: &mut FontsView<'_>) {
    let mut decos = Vec::new();
    for run in &out.runs {
        if run.map.objects.is_empty() {
            continue;
        }
        let g = fonts.layout_job(run.job.clone());
        for obj in &run.map.objects {
            let ci = crate::run::char_index(&run.job.text, obj.at);
            // Walk rows: each glyph is one char; a row ending in `\n` skips it.
            let mut k = 0usize;
            let mut found = None;
            'rows: for r in &g.rows {
                for gl in &r.row.glyphs {
                    if k == ci {
                        found = Some((r.pos, gl.pos));
                        break 'rows;
                    }
                    k += 1;
                }
                if r.ends_with_newline {
                    k += 1;
                }
            }
            let Some((rp, gp)) = found else { continue };
            let w = f32::from_bits(obj.width_bits);
            let sc = &obj.math.scene;
            let baseline = run.pos.y + rp.y + gp.y;
            let left = run.pos.x + rp.x + gp.x - w;
            decos.push(Deco::Math {
                rect: Rect::from_min_size(
                    pos2(left, baseline - sc.baseline),
                    vec2(sc.width, sc.height),
                ),
                math: obj.math.clone(),
            });
        }
    }
    out.decos.extend(decos);
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
        radius: 0.0,
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
