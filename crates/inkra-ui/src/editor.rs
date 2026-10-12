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
use inkra_layout::{Mode, Style, ViewLayout};
use serde_json::{Value, json};

use crate::clipboard::Clipboard;

/// Caret blink period (ED-020): 530 ms on, 530 ms off.
const BLINK: f64 = 0.53;

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
        }
    }

    /// The widget id (for focus).
    pub fn id(&self) -> egui::Id {
        self.id
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
            ViewMode::Source | ViewMode::Live => Mode::Source { wrap: true },
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
    ) -> EditorOutput {
        let t0 = std::time::Instant::now();
        let mut out = EditorOutput::default();
        let ctx = ui.ctx().clone();
        let rect = ui.available_rect_before_wrap();
        let resp = ui.interact(rect, self.id, Sense::click_and_drag());
        ui.painter().rect_filled(rect, 0.0, self.style.bg);

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

        // Geometry.
        // Until live rendering lands (M3) both modes use the source layout.
        let source = true;
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
        let mode = Self::mode(engine);
        self.view
            .sync(&engine.state.editor.doc, mode, col_w, &self.style);

        // Input.
        if focused {
            self.handle_keys(&ctx, engine, clipboard, &mut out, rect.height());
        }
        self.handle_mouse(&ctx, &resp, engine, rect, text_rect);

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

        let ed = &engine.state.editor;
        let style = &self.style;
        let caret = ed.sel.head;
        let mut caret_rect = None;
        let doc = &ed.doc;
        let view = &mut self.view;
        ctx.fonts_mut(|fonts| {
            caret_rect = view.caret_rect(caret, doc, style, fonts);
        });
        if self.scroll_to_caret {
            if let Some(r) = caret_rect {
                let margin = 2.0 * crate::editor::row_h(&self.style);
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
        // Leave room to scroll the last line up to the middle of the viewport.
        let max_scroll = (total - viewport_h * 0.5).max(0.0);
        self.scroll = self.scroll.clamp(0.0, max_scroll);

        // Lay out and paint visible items.
        let offset = vec2(text_rect.left(), rect.top() + pad - self.scroll);
        let y0 = self.scroll - pad - viewport_h * 0.5;
        let y1 = self.scroll - pad + viewport_h * 1.5;
        let visible = self.view.visible(y0..y1);
        let mut shapes = Vec::new();
        let ed = &engine.state.editor;
        let doc = &ed.doc;
        let view = &mut self.view;
        let style = &self.style;
        ctx.fonts_mut(|fonts| {
            for i in visible.clone() {
                view.ensure(i, doc, style, fonts);
            }
            caret_rect = view.caret_rect(caret, doc, style, fonts);
        });
        let painter = ui.painter_at(rect);
        // find matches and selection below text
        let ed = &engine.state.editor;
        if ed.find.open && !ed.find.matches.is_empty() {
            for (k, m) in ed.find.matches.iter().enumerate() {
                if visible.is_empty() {
                    break;
                }
                let vis_src = self.view.items()[visible.start].src.start
                    ..self.view.items()[visible.end - 1].src.end;
                if m.end < vis_src.start || m.start > vis_src.end {
                    continue;
                }
                let rects = self.view.selection_rects(m.clone(), visible.clone());
                let color = if ed.find.current == Some(k) {
                    self.style.search_current_bg
                } else {
                    self.style.search_match_bg
                };
                inkra_render::paint_selection(&mut shapes, &rects, offset, color);
            }
        }
        if !ed.sel.is_empty() {
            let rects = self.view.selection_rects(ed.sel.range(), visible.clone());
            let color = if focused {
                self.style.selection
            } else {
                self.style.selection.gamma_multiply(0.6)
            };
            inkra_render::paint_selection(&mut shapes, &rects, offset, color);
        }
        for i in visible.clone() {
            let top = self.view.top(i);
            if let Some(l) = self.view.layout(i) {
                inkra_render::paint_decos(&mut shapes, &l, top, offset, 1.0);
            }
            let runs = self.view.runs(i);
            inkra_render::paint_runs(&mut shapes, &runs, offset, self.style.fg, 1.0);
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

        // Caret (blinking, ED-020) and IME pre-edit.
        let now = ctx.input(|i| i.time);
        let window_focused = ctx.input(|i| i.viewport().focused.unwrap_or(true));
        self.caret_screen = caret_rect.map(|r| r.translate(offset));
        if focused
            && window_focused
            && let Some(r) = self.caret_screen
        {
            let phase = ((now - self.last_activity) / BLINK) as u64;
            if let Some(pre) = &engine.state.editor.preedit {
                let font = egui::FontId::new(
                    self.style.mono_size,
                    egui::FontFamily::Name(inkra_layout::fonts::MONO.into()),
                );
                let galley = painter.layout_no_wrap(pre.clone(), font, self.style.fg);
                let bg = Rect::from_min_size(r.min, galley.size());
                painter.rect_filled(bg, 0.0, self.style.bg);
                painter.line_segment(
                    [
                        pos2(bg.left(), bg.bottom() - 1.0),
                        pos2(bg.right(), bg.bottom() - 1.0),
                    ],
                    egui::Stroke::new(1.0, self.style.fg),
                );
                painter.galley(r.min, galley, self.style.fg);
            } else if phase.is_multiple_of(2) {
                let mut caret_shapes = Vec::new();
                inkra_render::paint_caret(&mut caret_shapes, r, Vec2::ZERO, self.style.cursor, 2.0);
                painter.extend(caret_shapes);
            }
            let next = BLINK - ((now - self.last_activity) % BLINK);
            ctx.request_repaint_after(Duration::from_secs_f64(next.max(0.01)));
            ctx.output_mut(|o| {
                o.ime = Some(egui::output::IMEOutput {
                    rect: text_rect,
                    cursor_rect: r,
                    purpose: Default::default(),
                    should_interrupt_composition: false,
                })
            });
        }

        // Scrollbar.
        self.scrollbar(ui, rect, total, viewport_h, max_scroll);
        self.last_ms = t0.elapsed().as_secs_f32() * 1000.0;
        out
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
        pos2(
            p.x - text_rect.left(),
            p.y - rect.top() - self.style.padding_y + self.scroll,
        )
    }

    fn hit(&mut self, ctx: &egui::Context, engine: &Engine, p: Pos2) -> usize {
        let doc = &engine.state.editor.doc;
        let view = &mut self.view;
        let style = &self.style;
        ctx.fonts_mut(|f| view.hit(p, doc, style, f))
    }

    fn handle_mouse(
        &mut self,
        ctx: &egui::Context,
        resp: &egui::Response,
        engine: &mut Engine,
        rect: Rect,
        text_rect: Rect,
    ) {
        let (pressed, pos, shift, released, primary_down) = ctx.input(|i| {
            (
                i.pointer.primary_pressed(),
                i.pointer.interact_pos(),
                i.modifiers.shift,
                i.pointer.primary_released(),
                i.pointer.primary_down(),
            )
        });
        let Some(screen) = pos else { return };
        let in_text_area = rect.contains(screen) && screen.x < rect.right() - 10.0;
        if pressed && in_text_area && resp.contains_pointer() {
            let p = self.doc_pos(screen, rect, text_rect);
            let at = self.hit(ctx, engine, p);
            let e = &mut engine.state.editor;
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
            // auto-scroll when dragging outside
            if screen.y < rect.top() {
                self.scroll -= (rect.top() - screen.y).min(40.0);
            } else if screen.y > rect.bottom() {
                self.scroll += (screen.y - rect.bottom()).min(40.0);
            }
            let p = self.doc_pos(screen, rect, text_rect);
            let at = self.hit(ctx, engine, p);
            let e = &mut engine.state.editor;
            let ext = match self.drag_unit {
                2 => ops::word_at(&e.doc, at),
                3 => ops::block_at(&e.doc, at),
                _ => Selection::caret(at),
            };
            e.sel = if at < anchor.start() {
                Selection::new(anchor.end(), ext.start())
            } else {
                Selection::new(anchor.start(), ext.end().max(at))
            };
            if self.drag_unit <= 1 {
                e.sel = Selection::new(anchor.anchor, at);
            }
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
                    self.key(ctx, engine, key, modifiers, viewport_h);
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
    ) {
        let word = m.command || (cfg!(target_os = "macos") && m.alt);
        let shift = m.shift;
        let e = &mut engine.state.editor;
        let mut moved = None;
        let mut vertical = false;
        match key {
            Key::ArrowLeft => {
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
                let page = matches!(key, Key::PageUp | Key::PageDown);
                let doc = &e.doc;
                let head = e.sel.head;
                let view = &mut self.view;
                let style = &self.style;
                let desired = &mut self.desired_x;
                let new = ctx.fonts_mut(|f| {
                    let r = view.caret_rect(head, doc, style, f);
                    let x = desired.get_or_insert(r.map_or(0.0, |r| r.min.x)).to_owned();
                    if page {
                        let y = r.map_or(0.0, |r| r.center().y)
                            + if down { viewport_h } else { -viewport_h };
                        view.hit(pos2(x, y), doc, style, f)
                    } else {
                        view.vertical(head, x, down, doc, style, f)
                    }
                });
                if page {
                    self.scroll += if down { viewport_h } else { -viewport_h };
                }
                moved = Some(new);
            }
            Key::Home | Key::End if !m.command => {
                let doc = &e.doc;
                let head = e.sel.head;
                let view = &mut self.view;
                let style = &self.style;
                let (a, b) = ctx.fonts_mut(|f| view.row_bounds(head, doc, style, f));
                moved = Some(if key == Key::Home { a } else { b });
            }
            Key::Enter => {
                e.newline(if shift { Break::Soft } else { Break::Enter });
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
                e.delete_backward(unit);
            }
            Key::Delete => {
                let unit = if m.command && shift {
                    Unit::Line
                } else if word {
                    Unit::Word
                } else {
                    Unit::Char
                };
                e.delete_forward(unit);
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
            let e = &mut engine.state.editor;
            let p = e.doc.text().snap(p);
            e.sel = if shift {
                Selection::new(e.sel.anchor, p)
            } else {
                Selection::caret(p)
            };
            e.doc.break_undo_group();
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
