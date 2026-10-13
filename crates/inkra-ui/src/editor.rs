//! The editing canvas: a custom text editor widget over the document rope.
//!
//! egui's `TextEdit` cannot map a caret through rendered Markdown, so Inkra
//! implements its own caret, selection, input and IME handling and only uses egui
//! for events and painting (SPEC appendix D-3).

use std::time::Duration;

use egui::{Event, Key, Modifiers, Pos2, Rect, Sense, Vec2, pos2, vec2};
use inkra_doc::Selection;
use inkra_engine::Engine;
use inkra_engine::editor::{Break, Unit, ViewMode, ops};
use inkra_layout::{Ctx, Deco, HitAction, Mode, Overlay, Style, ViewLayout};
use serde_json::{Value, json};

use crate::clipboard::Clipboard;
use crate::images::Images;

/// A table wider than the text column in the current frame.
#[derive(Debug, Clone, Copy)]
struct Wide {
    item: usize,
    key: usize,
    /// Grid in document coordinates.
    rect: Rect,
    /// Maximum horizontal scroll.
    max: f32,
    /// Current horizontal scroll.
    shift: f32,
}

/// Caret blink period (ED-020): 530 ms on, 530 ms off.
const BLINK: f64 = 0.53;
/// Blinking stops this long after the last input.
const BLINK_FOR: f64 = 30.0;

/// Commands requested by the editor that the shell must run.
#[derive(Debug, Default)]
pub struct EditorOutput {
    /// `(command id, params)` to execute.
    pub commands: Vec<(String, Value)>,
}

/// Editor widget state (one per window).
pub struct EditorView {
    id: egui::Id,
    view: ViewLayout,
    /// Visual style of the document area.
    pub style: Style,
    scroll: f32,
    desired_x: Option<f32>,
    last_activity: f64,
    drag_anchor: Option<Selection>,
    drag_unit: u8,
    scroll_to_caret: bool,
    /// Show line numbers in source mode (ED-121).
    pub line_numbers: bool,
    /// Last caret rectangle in screen coordinates (IME, tests).
    pub caret_screen: Option<Rect>,
    last_text_rect: Rect,
    scrollbar_drag: Option<f32>,
    /// Duration of the last frame's editor work in milliseconds (debug overlay).
    pub last_ms: f32,
    /// Loaded images.
    pub images: Images,
    /// Caret blinks (PR-042).
    pub blink: bool,
    col_handles: Vec<Rect>,
    col_drag: Option<(usize, usize)>,
    /// A reorder grip being dragged: table start, column?, index.
    grip_drag: Option<(usize, bool, usize)>,
    pending_top: Option<usize>,
    viewport_top: usize,
    /// Horizontal scroll of wide tables, by table source start (ED-080).
    hscroll: std::collections::HashMap<usize, f32>,
    wide: Vec<Wide>,
    i18n: inkra_i18n::I18n,
    base_style: Style,
    zoom: f32,
    /// syntect theme for code blocks.
    pub syntax_theme: String,
    last_mode: Option<Mode>,
}

impl Default for EditorView {
    fn default() -> Self {
        Self::new()
    }
}

impl EditorView {
    /// A new editor view.
    pub fn new() -> Self {
        Self {
            id: egui::Id::new("inkra-editor"),
            view: ViewLayout::new(),
            style: Style::default(),
            scroll: 0.0,
            desired_x: None,
            last_activity: 0.0,
            drag_anchor: None,
            drag_unit: 0,
            scroll_to_caret: true,
            line_numbers: true,
            caret_screen: None,
            last_text_rect: Rect::NOTHING,
            scrollbar_drag: None,
            last_ms: 0.0,
            images: Images::default(),
            blink: true,
            col_handles: Vec::new(),
            col_drag: None,
            grip_drag: None,
            pending_top: None,
            viewport_top: 0,
            hscroll: Default::default(),
            wide: Vec::new(),
            i18n: inkra_i18n::I18n::default(),
            base_style: Style::default(),
            zoom: 1.0,
            syntax_theme: "InspiredGitHub".to_owned(),
            last_mode: None,
        }
    }

    /// Sets the theme style (the zoomed copy is derived from it).
    pub fn set_style(&mut self, s: Style) {
        self.style = s.scaled(self.zoom);
        self.syntax_theme = s.syntax_theme.clone();
        self.base_style = s;
    }

    /// Text of every item laid out in the last frame, one line per item (tests).
    pub fn rendered_text(&self) -> Vec<String> {
        (0..self.view.items().len())
            .filter_map(|i| self.view.layout(i))
            .map(|l| {
                l.runs
                    .iter()
                    .map(|r| r.job.text.clone())
                    .collect::<Vec<_>>()
                    .join("|")
            })
            .collect()
    }

    /// Distinct text colors used by the item containing `pos` (tests: syntax
    /// highlighting).
    pub fn colors_at(&self, pos: usize) -> usize {
        let i = self.view.item_of(pos);
        let Some(l) = self.view.layout(i) else {
            return 0;
        };
        let mut set = std::collections::HashSet::new();
        for r in &l.runs {
            for s in &r.job.sections {
                set.insert(s.format.color);
            }
        }
        set.len()
    }

    /// Version of the theme style this view uses.
    pub fn base_version(&self) -> u64 {
        self.base_style.version
    }

    fn apply_zoom(&mut self, zoom: f32) {
        if (zoom - self.zoom).abs() > 1e-3 {
            self.zoom = zoom;
            self.style = self.base_style.scaled(zoom);
            self.scroll_to_caret = true;
        }
    }

    /// The widget id (for focus).
    pub fn id(&self) -> egui::Id {
        self.id
    }

    /// Document position shown at the top of the viewport (last frame).
    pub fn viewport_top(&self) -> usize {
        self.viewport_top
    }

    /// Scrolls so that the item containing `pos` is at the top (outline jumps).
    pub fn scroll_to_top(&mut self, pos: usize) {
        self.pending_top = Some(pos);
    }

    /// Requests that the caret be scrolled into view on the next frame.
    pub fn reveal_caret(&mut self) {
        self.scroll_to_caret = true;
    }

    /// Current scroll offset.
    pub fn scroll(&self) -> f32 {
        self.scroll
    }

    fn mode(engine: &Engine) -> Mode {
        match engine.state.editor.mode {
            ViewMode::Source => Mode::Source { wrap: true },
            ViewMode::Live => Mode::Live,
        }
    }

    fn touch(&mut self, ctx: &egui::Context) {
        self.last_activity = ctx.input(|i| i.time);
        self.scroll_to_caret = true;
    }

    /// Draws the editor in `ui`'s remaining space and handles its input.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        engine: &mut Engine,
        clipboard: &mut Clipboard,
        i18n: &inkra_i18n::I18n,
    ) -> EditorOutput {
        if self.i18n.language() != i18n.language() {
            self.i18n = i18n.clone();
        }
        let t0 = std::time::Instant::now();
        let mut out = EditorOutput::default();
        let ctx = ui.ctx().clone();
        let rect = ui.available_rect_before_wrap();
        let resp = ui.interact(rect, self.id, Sense::click_and_drag());
        ui.painter().rect_filled(rect, 0.0, self.style.bg);
        if self.images.poll(&ctx) {
            ctx.request_repaint();
        }
        self.images.set_base(
            engine
                .state
                .editor
                .path
                .as_ref()
                .and_then(|p| p.parent().map(|d| d.to_owned())),
        );

        // Focus: the editor owns keyboard focus unless another widget takes it.
        if resp.clicked() || resp.drag_started() || ctx.memory(|m| m.focused().is_none()) {
            resp.request_focus();
        }
        let focused = resp.has_focus();
        ctx.memory_mut(|m| {
            m.set_focus_lock_filter(
                self.id,
                egui::EventFilter {
                    tab: true,
                    horizontal_arrows: true,
                    vertical_arrows: true,
                    escape: false,
                },
            )
        });

        // Ctrl+wheel zoom (PR-039).
        if resp.contains_pointer() {
            let z = ctx.input(|i| i.zoom_delta());
            if (z - 1.0).abs() > 1e-3 {
                out.commands.push((
                    if z > 1.0 {
                        "view.zoomIn"
                    } else {
                        "view.zoomOut"
                    }
                    .into(),
                    Value::Null,
                ));
            }
        }
        self.apply_zoom(engine.state.view.zoom);

        // Geometry.
        let mode = Self::mode(engine);
        if self.last_mode.is_some_and(|m| m != mode) {
            self.scroll_to_caret = true;
        }
        self.last_mode = Some(mode);
        let source = matches!(mode, Mode::Source { .. });
        let lines = engine.state.editor.doc.text().len_lines().max(1);
        let gutter = if self.line_numbers && source {
            let digits = (lines as f32).log10().floor() as usize + 1;
            digits.max(2) as f32 * self.style.mono_size * 0.62 + 24.0
        } else {
            0.0
        };
        let scrollbar_w = 10.0;
        let avail = (rect.width() - gutter - scrollbar_w - 2.0 * self.style.padding_x).max(80.0);
        let col_w = if source {
            avail
        } else {
            avail.min(self.style.max_width)
        };
        let col_left = rect.left() + gutter + self.style.padding_x + (avail - col_w) / 2.0;
        let text_rect = Rect::from_min_max(
            pos2(col_left, rect.top()),
            pos2(col_left + col_w, rect.bottom()),
        );
        self.last_text_rect = text_rect;
        self.view.sync(
            &engine.state.editor.doc,
            mode,
            col_w,
            &self.style,
            engine.state.editor.sel.head,
        );

        // Input.
        if focused {
            self.handle_keys(&ctx, engine, clipboard, &mut out, rect.height());
        }
        self.handle_mouse(&ctx, &resp, engine, rect, text_rect, &mut out);
        // Live mode: the caret never rests on hidden syntax (snap against the view
        // of the edited document).
        self.view.sync(
            &engine.state.editor.doc,
            mode,
            col_w,
            &self.style,
            engine.state.editor.sel.head,
        );
        if !source {
            self.snap_caret(&ctx, engine, true);
            self.view.sync(
                &engine.state.editor.doc,
                mode,
                col_w,
                &self.style,
                engine.state.editor.sel.head,
            );
        }

        // Scrolling.
        let wheel = if resp.hovered() || resp.contains_pointer() {
            ctx.input(|i| i.smooth_scroll_delta.y)
        } else {
            0.0
        };
        self.scroll -= wheel;
        let pad = self.style.padding_y;
        let viewport_h = rect.height();
        self.view.begin_frame(self.scroll - pad);
        let shift = self.view.take_shift_above();
        self.scroll += shift;

        let caret = engine.state.editor.sel.head;
        let mut caret_rect = None;
        {
            let lcx = Ctx {
                doc: &engine.state.editor.doc,
                style: &self.style,
                caret,
                images: &self.images,
                syntax_theme: &self.syntax_theme,
            };
            let view = &mut self.view;
            ctx.fonts_mut(|fonts| caret_rect = view.caret_rect(caret, &lcx, fonts));
        }
        if let Some(pos) = self.pending_top.take() {
            let i = self.view.item_of(pos);
            self.scroll = self.view.top(i) + pad - 8.0;
            self.scroll_to_caret = false;
        }
        let typewriter = engine.state.view.typewriter;
        if self.scroll_to_caret && typewriter {
            // WM-002: the caret line stays in the vertical middle.
            if let Some(r) = caret_rect {
                self.scroll = r.center().y + pad - viewport_h / 2.0;
            }
            self.scroll_to_caret = false;
        }
        if self.scroll_to_caret {
            if let Some(r) = caret_rect {
                let margin = 2.0 * row_h(&self.style);
                let top = r.min.y + pad - margin;
                let bottom = r.max.y + pad + margin;
                if top < self.scroll {
                    self.scroll = top;
                } else if bottom > self.scroll + viewport_h {
                    self.scroll = bottom - viewport_h;
                }
            }
            self.scroll_to_caret = false;
        }
        let total = self.view.total_height() + 2.0 * pad;
        // Leave room to scroll the last line up to the middle of the viewport (and,
        // in typewriter mode, the first line down to it).
        let max_scroll = (total - viewport_h * 0.5).max(0.0);
        let min_scroll = if typewriter {
            -viewport_h * 0.5 + pad
        } else {
            0.0
        };
        self.scroll = self.scroll.clamp(min_scroll, max_scroll.max(min_scroll));

        // A little below the edge so a heading scrolled to the top counts.
        let top_item = self.view.item_at_y((self.scroll - pad + 12.0).max(0.0));
        self.viewport_top = self.view.items().get(top_item).map_or(0, |it| it.src.start);

        // Lay out and paint visible items.
        let offset = vec2(text_rect.left(), rect.top() + pad - self.scroll);
        let y0 = self.scroll - pad - viewport_h * 0.5;
        let y1 = self.scroll - pad + viewport_h * 1.5;
        let visible = self.view.visible(y0..y1);
        {
            let lcx = Ctx {
                doc: &engine.state.editor.doc,
                style: &self.style,
                caret,
                images: &self.images,
                syntax_theme: &self.syntax_theme,
            };
            let view = &mut self.view;
            ctx.fonts_mut(|fonts| {
                for i in visible.clone() {
                    view.ensure(i, &lcx, fonts);
                }
                caret_rect = view.caret_rect(caret, &lcx, fonts);
            });
        }
        // Wide tables scroll horizontally (shift+wheel or a horizontal wheel over
        // the table) and follow the caret.
        self.collect_wide_tables(visible.clone(), col_w);
        if !self.wide.is_empty() {
            let (hover, dx) = ctx.input(|i| (i.pointer.hover_pos(), i.smooth_scroll_delta.x));
            let caret_doc = caret_rect;
            for w in &mut self.wide {
                let mut shift = w.shift;
                if let Some(h) = hover {
                    let y = h.y - rect.top() - pad + self.scroll;
                    if dx != 0.0 && w.rect.min.y <= y && y <= w.rect.max.y {
                        shift -= dx;
                    }
                }
                if let Some(c) = caret_doc
                    .filter(|c| w.rect.min.y <= c.center().y && c.center().y <= w.rect.max.y)
                {
                    if c.min.x - shift < 0.0 {
                        shift = c.min.x - 8.0;
                    } else if c.max.x - shift > col_w {
                        shift = c.max.x - col_w + 8.0;
                    }
                }
                w.shift = shift.clamp(0.0, w.max);
                self.hscroll.insert(w.key, w.shift);
            }
            if let Some(c) = caret_rect {
                let s = self.hshift_at_y(c.center().y);
                caret_rect = Some(c.translate(vec2(-s, 0.0)));
            }
        }
        // Request textures for visible images and formulas.
        self.images.begin_frame();
        for i in visible.clone() {
            if let Some(l) = self.view.layout(i) {
                for d in &l.decos {
                    match d {
                        Deco::Image { src, .. } => {
                            self.images.get(&ctx, src);
                        }
                        Deco::Math { math, .. } => self.images.ensure_math(&ctx, math),
                        _ => {}
                    }
                }
            }
        }
        let painter = ui.painter_at(rect);
        let mut shapes = Vec::new();
        let mut wide_sel = Vec::new();
        let ed = &engine.state.editor;
        if ed.find.open && !ed.find.matches.is_empty() && !visible.is_empty() {
            let vis_src = self.view.items()[visible.start].src.start
                ..self.view.items()[visible.end - 1].src.end;
            for (k, m) in ed.find.matches.iter().enumerate() {
                if m.end < vis_src.start || m.start > vis_src.end {
                    continue;
                }
                let rects = self.view.selection_rects(m.clone(), visible.clone());
                let color = if ed.find.current == Some(k) {
                    self.style.search_current_bg
                } else {
                    self.style.search_match_bg
                };
                let (rects, wide) = self.split_rects(rects);
                inkra_render::paint_selection(&mut shapes, &rects, offset, color);
                inkra_render::paint_selection(&mut wide_sel, &wide, offset, color);
            }
        }
        // Focus mode (WM-001): items outside the current top-level block fade.
        let focus = engine.state.view.focus;
        let active_block = {
            let ast = engine.state.editor.doc.ast();
            ast.index_at(caret)
                .map(|i| ast.start(i)..ast.end(i))
                .unwrap_or(caret..caret)
        };
        let alphas: Vec<f32> = visible
            .clone()
            .map(|i| {
                let it = &self.view.items()[i];
                let active = it.src.start <= active_block.end && it.src.end >= active_block.start;
                let target = if !focus || active {
                    1.0
                } else {
                    self.style.focus_dim_opacity
                };
                ctx.animate_value_with_time(self.id.with(("focus", it.src.start)), target, 0.15)
            })
            .collect();
        let mut deco_shapes = Vec::new();
        let lookup = &self.images;
        let clip = Rect::from_min_max(
            pos2(text_rect.left() - 1.0, rect.top()),
            pos2(text_rect.right() + 1.0, rect.bottom()),
        );
        let wide_painter = ui.painter_at(clip);
        let mut wide_shapes = Vec::new();
        for (k, i) in visible.clone().enumerate() {
            let top = self.view.top(i);
            if let Some(l) = self.view.layout(i) {
                match self.wide.iter().find(|w| w.item == i) {
                    Some(w) => inkra_render::paint_decos(
                        &mut wide_shapes,
                        &l,
                        top,
                        offset - vec2(w.shift, 0.0),
                        alphas[k],
                        lookup,
                    ),
                    None => inkra_render::paint_decos(
                        &mut deco_shapes,
                        &l,
                        top,
                        offset,
                        alphas[k],
                        lookup,
                    ),
                }
            }
        }
        painter.extend(deco_shapes);
        wide_painter.extend(wide_shapes);
        if !ed.sel.is_empty() {
            let rects = self.view.selection_rects(ed.sel.range(), visible.clone());
            let (rects, wide) = self.split_rects(rects);
            let color = if focused {
                self.style.selection
            } else {
                self.style.selection.gamma_multiply(0.6)
            };
            inkra_render::paint_selection(&mut shapes, &rects, offset, color);
            inkra_render::paint_selection(&mut wide_sel, &wide, offset, color);
        }
        let mut wide_runs = wide_sel;
        for (k, i) in visible.clone().enumerate() {
            let runs = self.view.runs(i);
            match self.wide.iter().find(|w| w.item == i) {
                Some(w) => inkra_render::paint_runs(
                    &mut wide_runs,
                    &runs,
                    offset - vec2(w.shift, 0.0),
                    self.style.fg,
                    alphas[k],
                ),
                None => {
                    inkra_render::paint_runs(&mut shapes, &runs, offset, self.style.fg, alphas[k])
                }
            }
        }
        // Horizontal scroll indicators under wide tables.
        for w in &self.wide {
            let r = w.rect.translate(offset);
            let track = Rect::from_min_max(
                pos2(text_rect.left(), r.bottom() + 2.0),
                pos2(text_rect.right(), r.bottom() + 5.0),
            );
            let frac = col_w / (col_w + w.max);
            let x0 = track.left() + track.width() * (w.shift / (col_w + w.max));
            let thumb = Rect::from_min_max(
                pos2(x0, track.top()),
                pos2(x0 + track.width() * frac, track.bottom()),
            );
            wide_runs.push(egui::Shape::rect_filled(
                thumb,
                2.0,
                self.style.fg_muted.gamma_multiply(0.35),
            ));
        }
        // Line numbers.
        if gutter > 0.0 {
            let font = egui::FontId::new(
                self.style.mono_size * 0.9,
                egui::FontFamily::Name(inkra_layout::fonts::MONO.into()),
            );
            let caret_item = self.view.item_of(caret);
            for i in visible.clone() {
                let y = self.view.top(i) + offset.y;
                let color = if i == caret_item {
                    self.style.fg_muted
                } else {
                    self.style.line_numbers_fg
                };
                painter.text(
                    pos2(rect.left() + gutter - 12.0, y + 2.0),
                    egui::Align2::RIGHT_TOP,
                    (i + 1).to_string(),
                    font.clone(),
                    color,
                );
            }
        }
        painter.extend(shapes);
        wide_painter.extend(wide_runs);

        // Column borders of tables can be dragged (ED-084, display only).
        if !source {
            self.table_handles(ui, engine, &visible, offset, &mut out);
        }

        // Code block tools (ED-002): language and copy button when the caret is inside.
        if !source {
            self.code_tools(ui, engine, &visible, offset, &mut out);
        }

        // Caret (blinking, ED-020) and IME pre-edit.
        let now = ctx.input(|i| i.time);
        let window_focused = ctx.input(|i| i.viewport().focused.unwrap_or(true));
        self.caret_screen = caret_rect.map(|r| r.translate(offset));
        if focused
            && window_focused
            && let Some(r) = self.caret_screen
        {
            let idle = now - self.last_activity;
            // Blinking stops (caret stays visible) after a while without input so
            // an idle editor costs no CPU (SPEC 2.5).
            let blinking = self.blink && idle < BLINK_FOR;
            let phase = if blinking { (idle / BLINK) as u64 } else { 0 };
            if let Some(pre) = &engine.state.editor.preedit {
                let font = egui::FontId::new(
                    r.height().clamp(10.0, 40.0) / 1.5,
                    egui::FontFamily::Proportional,
                );
                let galley = painter.layout_no_wrap(pre.clone(), font, self.style.fg);
                let bg = Rect::from_min_size(r.min, vec2(galley.size().x, r.height()));
                painter.rect_filled(bg, 0.0, self.style.bg);
                painter.line_segment(
                    [
                        pos2(bg.left(), bg.bottom() - 1.0),
                        pos2(bg.right(), bg.bottom() - 1.0),
                    ],
                    egui::Stroke::new(1.0, self.style.fg),
                );
                painter.galley(
                    pos2(r.min.x, r.center().y - galley.size().y / 2.0),
                    galley,
                    self.style.fg,
                );
            } else if phase.is_multiple_of(2) {
                let mut caret_shapes = Vec::new();
                inkra_render::paint_caret(&mut caret_shapes, r, Vec2::ZERO, self.style.cursor, 2.0);
                painter.extend(caret_shapes);
            }
            if blinking {
                let next = BLINK - (idle % BLINK);
                ctx.request_repaint_after(Duration::from_secs_f64(next.max(0.01)));
            }
            ctx.output_mut(|o| {
                o.ime = Some(egui::output::IMEOutput {
                    rect: r, // egui-winit uses `rect` as the IME cursor area (ED-010)
                    cursor_rect: r,
                    purpose: Default::default(),
                    should_interrupt_composition: false,
                })
            });
        }

        // Right-click menu (ED-130).
        resp.context_menu(|ui| {
            let cmds = crate::context_menu::show(ui, engine, i18n);
            out.commands.extend(cmds);
        });

        // Scrollbar.
        self.scrollbar(ui, rect, total, viewport_h, max_scroll);
        self.last_ms = t0.elapsed().as_secs_f32() * 1000.0;
        out
    }

    /// Draggable column borders of visible tables.
    fn table_handles(
        &mut self,
        ui: &mut egui::Ui,
        engine: &mut Engine,
        visible: &std::ops::Range<usize>,
        offset: Vec2,
        out: &mut EditorOutput,
    ) {
        self.col_handles.clear();
        let mut tables = Vec::new();
        for i in visible.clone() {
            let Some(l) = self.view.layout(i) else {
                continue;
            };
            let top = self.view.top(i);
            for o in &l.overlays {
                if let Overlay::Table {
                    rect,
                    span,
                    cols,
                    rows,
                } = o
                {
                    tables.push((
                        i,
                        rect.translate(vec2(0.0, top)),
                        span.clone(),
                        cols.clone(),
                        rows.clone(),
                    ));
                }
            }
        }
        for (item, rect, span, cols, rows) in tables {
            let key = span.start;
            let shift = self
                .wide
                .iter()
                .find(|w| w.item == item)
                .map_or(0.0, |w| w.shift);
            let r = rect.translate(offset - vec2(shift, 0.0));
            let mut x = r.left();
            for (c, w) in cols.iter().enumerate() {
                x += w;
                let handle = Rect::from_min_max(pos2(x - 3.0, r.top()), pos2(x + 3.0, r.bottom()));
                if handle.right() > self.last_text_rect.right() + 4.0 {
                    continue;
                }
                let id = self.id.with(("col-handle", key, c));
                let resp = ui.interact(handle, id, Sense::drag());
                self.col_handles.push(handle);
                if resp.hovered() || resp.dragged() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeColumn);
                }
                if resp.drag_started() {
                    self.col_drag = Some((key, c));
                }
                if resp.dragged() {
                    let dx = resp.drag_delta().x;
                    let ws = self
                        .images
                        .column_widths
                        .entry(key)
                        .or_insert_with(|| cols.clone());
                    if let Some(v) = ws.get_mut(c) {
                        *v = (*v + dx).max(40.0);
                    }
                }
                if resp.drag_stopped() {
                    self.col_drag = None;
                }
                if resp.double_clicked() {
                    // Back to automatic widths.
                    self.images.column_widths.remove(&key);
                }
            }
            self.reorder_grips(ui, engine, r, &span, &cols, &rows, out);
        }
    }

    /// Grips to reorder columns (top edge of the header cells) and body rows (left
    /// margin) by dragging (ED-084). The source is rewritten on release.
    #[allow(clippy::too_many_arguments)]
    fn reorder_grips(
        &mut self,
        ui: &mut egui::Ui,
        engine: &mut Engine,
        r: Rect,
        span: &std::ops::Range<usize>,
        cols: &[f32],
        rows: &[f32],
        out: &mut EditorOutput,
    ) {
        let hovered = ui
            .input(|i| i.pointer.hover_pos())
            .is_some_and(|p| r.expand2(vec2(16.0, 8.0)).contains(p));
        let caret = engine.state.editor.sel.head;
        let active = caret >= span.start && caret <= span.end;
        let dragging_here = self.grip_drag.is_some_and(|g| g.0 == span.start);
        if !(hovered || active || dragging_here) {
            return;
        }
        let painter = ui.painter_at(self.last_text_rect.expand2(vec2(24.0, 0.0)));
        let muted = self.style.fg_muted.gamma_multiply(0.5);
        let pointer = ui.input(|i| i.pointer.interact_pos());
        // Column grips.
        let mut x = r.left();
        let mut col_x = vec![x];
        for w in cols {
            x += w;
            col_x.push(x);
        }
        let mut row_y = vec![r.top()];
        let mut y = r.top();
        for h in rows {
            y += h;
            row_y.push(y);
        }
        let mut release = None;
        for c in 0..cols.len() {
            let g = Rect::from_min_max(
                pos2(col_x[c] + 4.0, r.top()),
                pos2(col_x[c + 1] - 4.0, r.top() + 6.0),
            );
            let id = self.id.with(("col-grip", span.start, c));
            let resp = ui.interact(g, id, Sense::drag());
            self.col_handles.push(g);
            if resp.hovered() || resp.dragged() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
                painter.rect_filled(g.shrink2(vec2(g.width() * 0.35, 1.5)), 2.0, muted);
            }
            if resp.drag_started() {
                self.grip_drag = Some((span.start, true, c));
            }
            if resp.drag_stopped() {
                release = Some((true, c));
            }
        }
        for rr in 1..rows.len() {
            let g = Rect::from_min_max(
                pos2(r.left() - 14.0, row_y[rr] + 2.0),
                pos2(r.left() - 3.0, row_y[rr + 1] - 2.0),
            );
            let id = self.id.with(("row-grip", span.start, rr));
            let resp = ui.interact(g, id, Sense::drag());
            self.col_handles.push(g);
            if resp.hovered() || resp.dragged() || active {
                for k in 0..3 {
                    let cy = g.center().y - 4.0 + 4.0 * k as f32;
                    painter.circle_filled(pos2(g.center().x, cy), 1.2, muted);
                }
            }
            if resp.hovered() || resp.dragged() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
            }
            if resp.drag_started() {
                self.grip_drag = Some((span.start, false, rr));
            }
            if resp.drag_stopped() {
                release = Some((false, rr));
            }
        }
        // Drop target under the pointer, shown as a line while dragging.
        let target = |p: Pos2, col: bool| -> usize {
            if col {
                (0..cols.len())
                    .min_by(|&a, &b| {
                        let ca = (col_x[a] + col_x[a + 1]) / 2.0;
                        let cb = (col_x[b] + col_x[b + 1]) / 2.0;
                        (p.x - ca).abs().total_cmp(&(p.x - cb).abs())
                    })
                    .unwrap_or(0)
            } else {
                (1..rows.len())
                    .min_by(|&a, &b| {
                        let ca = (row_y[a] + row_y[a + 1]) / 2.0;
                        let cb = (row_y[b] + row_y[b + 1]) / 2.0;
                        (p.y - ca).abs().total_cmp(&(p.y - cb).abs())
                    })
                    .unwrap_or(1)
            }
        };
        if let (Some((key, col, from)), Some(p)) = (self.grip_drag, pointer)
            && key == span.start
        {
            let to = target(p, col);
            let stroke = egui::Stroke::new(2.0, self.style.accent);
            if col {
                let x = if to > from { col_x[to + 1] } else { col_x[to] };
                painter.line_segment([pos2(x, r.top()), pos2(x, r.bottom())], stroke);
            } else {
                let y = if to > from { row_y[to + 1] } else { row_y[to] };
                painter.line_segment([pos2(r.left(), y), pos2(r.right(), y)], stroke);
            }
            if let Some((col, from)) = release {
                self.grip_drag = None;
                if to != from {
                    // Commands act on the table at the caret.
                    let inside = span.start + 1;
                    engine
                        .state
                        .editor
                        .set_selection(inkra_doc::Selection::caret(inside));
                    let id = if col {
                        "table.moveColumnTo"
                    } else {
                        "table.moveRowTo"
                    };
                    out.commands
                        .push((id.into(), serde_json::json!({ "from": from, "to": to })));
                }
            }
        }
    }

    /// Splits highlight rectangles into those outside wide tables and those inside
    /// (shifted by the table's horizontal scroll; painted clipped).
    fn split_rects(&self, rects: Vec<Rect>) -> (Vec<Rect>, Vec<Rect>) {
        if self.wide.is_empty() {
            return (rects, Vec::new());
        }
        let mut plain = Vec::new();
        let mut wide = Vec::new();
        for r in rects {
            match self
                .wide
                .iter()
                .find(|w| w.rect.min.y <= r.center().y && r.center().y <= w.rect.max.y)
            {
                Some(w) => wide.push(r.translate(vec2(-w.shift, 0.0))),
                None => plain.push(r),
            }
        }
        (plain, wide)
    }

    fn code_tools(
        &mut self,
        ui: &mut egui::Ui,
        engine: &mut Engine,
        visible: &std::ops::Range<usize>,
        offset: Vec2,
        out: &mut EditorOutput,
    ) {
        let caret = engine.state.editor.sel.head;
        for i in visible.clone() {
            let Some(l) = self.view.layout(i) else {
                continue;
            };
            let top = self.view.top(i);
            for o in &l.overlays {
                let (rect, lang, body) = match o {
                    Overlay::Code {
                        rect, lang, body, ..
                    } => (rect, lang, body),
                    Overlay::Table { rect, span, .. } => {
                        let active = caret >= span.start && caret <= span.end;
                        if active {
                            let r = rect.translate(vec2(0.0, top) + offset);
                            self.table_toolbar(ui, engine, r, span.start, out);
                        }
                        continue;
                    }
                };
                let r = rect.translate(vec2(0.0, top) + offset);
                let active = caret >= body.start && caret <= body.end;
                let label = if lang.is_empty() {
                    "text".to_owned()
                } else {
                    lang.clone()
                };
                let pos = pos2(r.right() - 8.0, r.top() + 4.0);
                if active {
                    let area = egui::Area::new(self.id.with(("code-tools", body.start)))
                        .fixed_pos(pos - vec2(150.0, 0.0))
                        .order(egui::Order::Foreground);
                    area.show(ui.ctx(), |ui| {
                        ui.horizontal(|ui| {
                            ui.add_space(150.0 - 110.0);
                            ui.label(
                                egui::RichText::new(label)
                                    .small()
                                    .color(self.style.fg_muted),
                            );
                            if ui.small_button("⧉").on_hover_text("Copy").clicked() {
                                let code = engine
                                    .state
                                    .editor
                                    .doc
                                    .text()
                                    .slice(body.clone())
                                    .into_owned();
                                out.commands
                                    .push(("app.setClipboard".into(), json!({ "text": code })));
                            }
                        });
                    });
                } else if !lang.is_empty() {
                    ui.painter().text(
                        pos,
                        egui::Align2::RIGHT_TOP,
                        lang,
                        egui::FontId::proportional(11.0),
                        self.style.fg_muted.gamma_multiply(0.7),
                    );
                }
            }
        }
    }

    /// The floating table toolbar (ED-082): alignment, delete, resize and more.
    fn table_toolbar(
        &mut self,
        ui: &mut egui::Ui,
        engine: &mut Engine,
        r: Rect,
        key: usize,
        out: &mut EditorOutput,
    ) {
        let i18n = self.i18n.clone();
        let area = egui::Area::new(self.id.with(("table-tools", key)))
            .fixed_pos(pos2(r.left(), r.top() - 28.0))
            .order(egui::Order::Foreground);
        area.show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style())
                .inner_margin(egui::Margin::symmetric(4, 2))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let mut b = |ui: &mut egui::Ui, label: &str, id: &str| {
                            let tip = i18n.tr(&inkra_engine::command::title_key(id));
                            if ui.small_button(label).on_hover_text(tip).clicked() {
                                out.commands.push((id.to_owned(), Value::Null));
                            }
                        };
                        b(ui, "⇤", "table.alignLeft");
                        b(ui, "↔", "table.alignCenter");
                        b(ui, "⇥", "table.alignRight");
                        ui.separator();
                        b(ui, "⊞", "table.resize");
                        b(ui, "🗑", "table.deleteTable");
                        ui.menu_button("⋯", |ui| {
                            let cmds = crate::context_menu::table_items(ui, engine, &i18n);
                            out.commands.extend(cmds);
                        });
                    });
                });
        });
    }

    fn snap_caret(&mut self, ctx: &egui::Context, engine: &mut Engine, forward: bool) {
        let e = &mut engine.state.editor;
        let lcx = Ctx {
            doc: &e.doc,
            style: &self.style,
            caret: e.sel.head,
            images: &self.images,
            syntax_theme: &self.syntax_theme,
        };
        let view = &mut self.view;
        let head = e.sel.head;
        let snapped = ctx.fonts_mut(|f| view.snap(head, forward, &lcx, f));
        if snapped != head {
            let keep_anchor = !e.sel.is_empty();
            e.sel = if keep_anchor {
                Selection::new(e.sel.anchor, snapped)
            } else {
                Selection::caret(snapped)
            };
        }
    }

    fn scrollbar(
        &mut self,
        ui: &mut egui::Ui,
        rect: Rect,
        total: f32,
        viewport_h: f32,
        max_scroll: f32,
    ) {
        if total <= viewport_h {
            return;
        }
        let track = Rect::from_min_max(pos2(rect.right() - 10.0, rect.top()), rect.right_bottom());
        let frac = (viewport_h / (total + viewport_h * 0.5)).clamp(0.05, 1.0);
        let h = (track.height() * frac).max(24.0);
        let t = if max_scroll > 0.0 {
            self.scroll / max_scroll
        } else {
            0.0
        };
        let y = track.top() + (track.height() - h) * t;
        let thumb = Rect::from_min_size(pos2(track.left() + 2.0, y), vec2(6.0, h));
        let resp = ui.interact(track, self.id.with("scrollbar"), Sense::click_and_drag());
        if resp.drag_started()
            && let Some(p) = resp.interact_pointer_pos()
        {
            self.scrollbar_drag = Some(if thumb.contains(p) {
                p.y - thumb.top()
            } else {
                h / 2.0
            });
        }
        if resp.dragged()
            && let (Some(grab), Some(p)) = (self.scrollbar_drag, resp.interact_pointer_pos())
        {
            let top = (p.y - grab - track.top()).clamp(0.0, track.height() - h);
            self.scroll = top / (track.height() - h).max(1.0) * max_scroll;
        }
        if resp.drag_stopped() {
            self.scrollbar_drag = None;
        }
        let color = if resp.hovered() || resp.dragged() {
            self.style.fg_muted
        } else {
            self.style.mark_fg.gamma_multiply(0.6)
        };
        ui.painter().rect_filled(thumb, 3.0, color);
    }

    fn doc_pos(&self, p: Pos2, rect: Rect, text_rect: Rect) -> Pos2 {
        let y = p.y - rect.top() - self.style.padding_y + self.scroll;
        pos2(p.x - text_rect.left() + self.hshift_at_y(y), y)
    }

    /// Horizontal scroll of the wide table at document height `y` (ED-080).
    fn hshift_at_y(&self, y: f32) -> f32 {
        self.wide
            .iter()
            .find(|w| w.rect.min.y <= y && y <= w.rect.max.y)
            .map_or(0.0, |w| w.shift)
    }

    /// Finds tables wider than the column among `visible` items and clamps their
    /// horizontal scroll.
    fn collect_wide_tables(&mut self, visible: std::ops::Range<usize>, col_w: f32) {
        self.wide.clear();
        for i in visible {
            let Some(l) = self.view.layout(i) else {
                continue;
            };
            let top = self.view.top(i);
            for o in &l.overlays {
                if let Overlay::Table { rect, span, .. } = o {
                    let over = rect.max.x - col_w;
                    if over > 0.5 {
                        let shift = self
                            .hscroll
                            .get(&span.start)
                            .copied()
                            .unwrap_or(0.0)
                            .clamp(0.0, over);
                        self.wide.push(Wide {
                            item: i,
                            key: span.start,
                            rect: rect.translate(vec2(0.0, top)),
                            max: over,
                            shift,
                        });
                    }
                }
            }
        }
    }

    fn hit(&mut self, ctx: &egui::Context, engine: &Engine, p: Pos2) -> usize {
        let e = &engine.state.editor;
        let lcx = Ctx {
            doc: &e.doc,
            style: &self.style,
            caret: e.sel.head,
            images: &self.images,
            syntax_theme: &self.syntax_theme,
        };
        let view = &mut self.view;
        ctx.fonts_mut(|f| view.hit(p, &lcx, f))
    }

    /// Clickable region (task checkbox) at document point `p`.
    fn hit_action(&mut self, p: Pos2) -> Option<HitAction> {
        let i = self.view.item_at_y(p.y);
        let top = self.view.top(i);
        let l = self.view.layout(i)?;
        l.hits
            .iter()
            .find(|h| h.rect.translate(vec2(0.0, top)).contains(p))
            .map(|h| h.action.clone())
    }

    fn handle_mouse(
        &mut self,
        ctx: &egui::Context,
        resp: &egui::Response,
        engine: &mut Engine,
        rect: Rect,
        text_rect: Rect,
        out: &mut EditorOutput,
    ) {
        let (pressed, pos, shift, released, primary_down, command) = ctx.input(|i| {
            (
                i.pointer.primary_pressed(),
                i.pointer.interact_pos(),
                i.modifiers.shift,
                i.pointer.primary_released(),
                i.pointer.primary_down(),
                i.modifiers.command,
            )
        });
        let Some(screen) = pos else { return };
        if self.col_handles.iter().any(|r| r.contains(screen))
            || self.col_drag.is_some()
            || self.grip_drag.is_some()
        {
            return;
        }
        // Right click: move the caret there unless it is inside the selection.
        if ctx.input(|i| i.pointer.secondary_pressed()) && resp.contains_pointer() {
            let p = self.doc_pos(screen, rect, text_rect);
            let at = self.hit(ctx, engine, p);
            let e = &mut engine.state.editor;
            if e.sel.is_empty() || at < e.sel.start() || at > e.sel.end() {
                e.set_selection(Selection::caret(at));
            }
        }
        let in_text_area = rect.contains(screen) && screen.x < rect.right() - 10.0;
        if pressed && in_text_area && resp.contains_pointer() {
            let p = self.doc_pos(screen, rect, text_rect);
            if let Some(action) = self.hit_action(p) {
                match action {
                    HitAction::ToggleTask(at) => {
                        out.commands
                            .push(("paragraph.taskStatus".into(), json!({ "at": at })));
                    }
                    HitAction::Link(url) => out
                        .commands
                        .push(("app.openUrl".into(), json!({ "url": url }))),
                }
                self.drag_anchor = None;
                return;
            }
            // ED-006: clicking below the last block starts a new paragraph.
            let below_end = p.y > self.view.total_height();
            let at = self.hit(ctx, engine, p);
            let e = &mut engine.state.editor;
            if below_end && e.mode == ViewMode::Live && !shift {
                out.commands
                    .push(("paragraph.ensureTrailing".into(), Value::Null));
            }
            // Ctrl+click on a link opens it (PR-040).
            if command
                && !shift
                && let Some(url) = ops::link_at(&e.doc, at)
            {
                out.commands
                    .push(("app.openUrl".into(), json!({ "url": url })));
            }
            if shift {
                e.sel = Selection::new(e.sel.anchor, at);
                self.drag_anchor = Some(Selection::caret(e.sel.anchor));
            } else {
                e.set_selection(Selection::caret(at));
                self.drag_anchor = Some(Selection::caret(at));
            }
            self.drag_unit = 1;
            self.desired_x = None;
            e.doc.break_undo_group();
            self.touch(ctx);
        }
        // double / triple click select word / block (ED-011)
        if resp.double_clicked() || resp.triple_clicked() {
            let p = self.doc_pos(screen, rect, text_rect);
            let at = self.hit(ctx, engine, p);
            let e = &mut engine.state.editor;
            let s = if resp.triple_clicked() {
                ops::block_at(&e.doc, at)
            } else {
                ops::word_at(&e.doc, at)
            };
            e.sel = s;
            self.drag_anchor = Some(s);
            self.drag_unit = if resp.triple_clicked() { 3 } else { 2 };
            self.touch(ctx);
        }
        if primary_down
            && resp.dragged()
            && let Some(anchor) = self.drag_anchor
        {
            if screen.y < rect.top() {
                self.scroll -= (rect.top() - screen.y).min(40.0);
            } else if screen.y > rect.bottom() {
                self.scroll += (screen.y - rect.bottom()).min(40.0);
            }
            let p = self.doc_pos(screen, rect, text_rect);
            let at = self.hit(ctx, engine, p);
            let e = &mut engine.state.editor;
            e.sel = match self.drag_unit {
                2 | 3 => {
                    let ext = if self.drag_unit == 2 {
                        ops::word_at(&e.doc, at)
                    } else {
                        ops::block_at(&e.doc, at)
                    };
                    if at < anchor.start() {
                        Selection::new(anchor.end(), ext.start())
                    } else {
                        Selection::new(anchor.start(), ext.end().max(at))
                    }
                }
                _ => Selection::new(anchor.anchor, at),
            };
            self.last_activity = ctx.input(|i| i.time);
            ctx.request_repaint();
        }
        if released {
            self.drag_anchor = None;
        }
    }

    fn handle_keys(
        &mut self,
        ctx: &egui::Context,
        engine: &mut Engine,
        clipboard: &mut Clipboard,
        out: &mut EditorOutput,
        viewport_h: f32,
    ) {
        let events = ctx.input(|i| i.events.clone());
        for ev in events {
            match ev {
                Event::Text(t) => {
                    if t.chars().all(|c| !c.is_control()) {
                        engine.state.editor.type_text(&t);
                        self.desired_x = None;
                        self.touch(ctx);
                    }
                }
                Event::Ime(ime) => {
                    match ime {
                        egui::ImeEvent::Preedit { text, .. } => {
                            engine.state.editor.preedit = (!text.is_empty()).then_some(text);
                        }
                        egui::ImeEvent::Commit(text) => {
                            engine.state.editor.preedit = None;
                            if !text.is_empty() {
                                engine.state.editor.type_text(&text);
                            }
                        }
                        _ => {}
                    }
                    self.touch(ctx);
                }
                Event::Copy => out.commands.push(("edit.copy".into(), Value::Null)),
                Event::Cut => out.commands.push(("edit.cut".into(), Value::Null)),
                Event::Paste(text) => {
                    let plain = ctx.input(|i| i.modifiers.shift);
                    let c = clipboard.read(Some(text));
                    let params = json!({
                        "text": c.text,
                        "html": c.html,
                        "files": c.files.iter().map(|p| p.to_string_lossy().into_owned()).collect::<Vec<_>>(),
                        "fromInkra": c.from_inkra,
                    });
                    out.commands.push((
                        if plain {
                            "edit.pastePlain"
                        } else {
                            "edit.paste"
                        }
                        .into(),
                        params,
                    ));
                }
                Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } => {
                    self.key(ctx, engine, key, modifiers, viewport_h, out);
                }
                _ => {}
            }
        }
    }

    fn key(
        &mut self,
        ctx: &egui::Context,
        engine: &mut Engine,
        key: Key,
        m: Modifiers,
        viewport_h: f32,
        out: &mut EditorOutput,
    ) {
        let word = m.command || (cfg!(target_os = "macos") && m.alt);
        let shift = m.shift;
        let live = engine.state.editor.mode == ViewMode::Live;
        let mut moved = None;
        let mut vertical = false;
        let mut forward = true;
        {
            let e = &mut engine.state.editor;
            match key {
                Key::ArrowLeft => {
                    forward = false;
                    moved = Some(if !shift && !e.sel.is_empty() {
                        e.sel.start()
                    } else {
                        ops::prev_pos(&e.doc, e.sel.head, word)
                    });
                }
                Key::ArrowRight => {
                    moved = Some(if !shift && !e.sel.is_empty() {
                        e.sel.end()
                    } else {
                        ops::next_pos(&e.doc, e.sel.head, word)
                    });
                }
                Key::ArrowUp | Key::ArrowDown | Key::PageUp | Key::PageDown => {
                    vertical = true;
                    let down = matches!(key, Key::ArrowDown | Key::PageDown);
                    forward = down;
                    let page = matches!(key, Key::PageUp | Key::PageDown);
                    let lcx = Ctx {
                        doc: &e.doc,
                        style: &self.style,
                        caret: e.sel.head,
                        images: &self.images,
                        syntax_theme: &self.syntax_theme,
                    };
                    let head = e.sel.head;
                    let view = &mut self.view;
                    let desired = &mut self.desired_x;
                    let new = ctx.fonts_mut(|f| {
                        let r = view.caret_rect(head, &lcx, f);
                        let x = *desired.get_or_insert(r.map_or(0.0, |r| r.min.x));
                        if page {
                            let y = r.map_or(0.0, |r| r.center().y)
                                + if down { viewport_h } else { -viewport_h };
                            view.hit(pos2(x, y), &lcx, f)
                        } else {
                            view.vertical(head, x, down, &lcx, f)
                        }
                    });
                    if page {
                        self.scroll += if down { viewport_h } else { -viewport_h };
                    }
                    // ED-005/ED-006: Down on the last line of a document that ends in
                    // a block the caret cannot leave (code, math, table) adds a
                    // paragraph below it.
                    let at_end_block = live
                        && down
                        && !page
                        && !shift
                        && (new == head
                            || inkra_engine::editor::context::in_code_content(&e.doc, new))
                        && e.doc
                            .ast()
                            .index_at(head)
                            .is_some_and(|i| i + 1 == e.doc.ast().len())
                        && {
                            let ast = e.doc.ast();
                            ast.get(ast.len() - 1).is_some_and(|b| {
                                matches!(
                                    b.block.kind,
                                    inkra_markdown::BlockKind::CodeBlock { .. }
                                        | inkra_markdown::BlockKind::MathBlock { .. }
                                        | inkra_markdown::BlockKind::Table { .. }
                                        | inkra_markdown::BlockKind::HtmlBlock { .. }
                                )
                            })
                        }
                        && {
                            let t = e.doc.text();
                            t.byte_to_line(head) + 2 >= t.len_lines()
                        };
                    if at_end_block {
                        out.commands
                            .push(("paragraph.ensureTrailing".into(), Value::Null));
                        moved = None;
                    } else {
                        moved = Some(new);
                    }
                }
                Key::Home | Key::End if !m.command => {
                    forward = key == Key::End;
                    let lcx = Ctx {
                        doc: &e.doc,
                        style: &self.style,
                        caret: e.sel.head,
                        images: &self.images,
                        syntax_theme: &self.syntax_theme,
                    };
                    let head = e.sel.head;
                    let view = &mut self.view;
                    let (a, b) = ctx.fonts_mut(|f| view.row_bounds(head, &lcx, f));
                    moved = Some(if key == Key::Home { a } else { b });
                }
                Key::Enter => {
                    if live && m.command {
                        out.commands.push(("edit.exitBlock".into(), Value::Null));
                    } else if live {
                        e.newline_live(if shift { Break::Soft } else { Break::Enter });
                    } else {
                        e.newline(if shift { Break::Soft } else { Break::Enter });
                    }
                }
                Key::Tab => e.tab(shift),
                Key::Backspace => {
                    let unit = if m.command && shift {
                        Unit::Line
                    } else if word {
                        Unit::Word
                    } else {
                        Unit::Char
                    };
                    let unit = if cfg!(target_os = "macos") && m.mac_cmd && !shift {
                        Unit::Line
                    } else {
                        unit
                    };
                    if live {
                        e.delete_backward_live(unit);
                    } else {
                        e.delete_backward(unit);
                    }
                    forward = false;
                }
                Key::Delete => {
                    let unit = if m.command && shift {
                        Unit::Line
                    } else if word {
                        Unit::Word
                    } else {
                        Unit::Char
                    };
                    if live {
                        e.delete_forward_live(unit);
                    } else {
                        e.delete_forward(unit);
                    }
                }
                Key::Escape => {
                    if e.find.open {
                        e.find.open = false;
                    } else if !e.sel.is_empty() {
                        e.sel = Selection::caret(e.sel.head);
                    }
                }
                _ => return,
            }
            if let Some(p) = moved {
                let p = e.doc.text().snap(p);
                e.sel = if shift {
                    Selection::new(e.sel.anchor, p)
                } else {
                    Selection::caret(p)
                };
                e.doc.break_undo_group();
            }
        }
        if live && moved.is_some() {
            // Re-sync so hidden/revealed markers match the new caret, then snap.
            let mode = Self::mode(engine);
            let w = self.view.width();
            self.view.sync(
                &engine.state.editor.doc,
                mode,
                w,
                &self.style,
                engine.state.editor.sel.head,
            );
            self.snap_caret(ctx, engine, forward);
        }
        if !vertical {
            self.desired_x = None;
        }
        self.touch(ctx);
    }
}

/// Source-mode row height.
pub fn row_h(style: &Style) -> f32 {
    inkra_layout::source::row_height(style)
}
