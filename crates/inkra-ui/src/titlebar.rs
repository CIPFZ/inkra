//! The integrated title bar (TH-008, default on Windows): menus, the window
//! title and the window buttons share one row. Dragging the empty part moves the
//! window, double-clicking it maximizes, and the window edges resize.

use egui::{Color32, CursorIcon, Pos2, Rect, Sense, Stroke, ViewportCommand, pos2, vec2};

use inkra_engine::Engine;
use inkra_i18n::I18n;

/// Whether windows draw their own title bar.
pub(crate) fn integrated(engine: &Engine) -> bool {
    integrated_pref(&engine.state.prefs.string("appearance.titleBar"))
}

/// `appearance.titleBar`: `auto` is integrated on Windows only.
pub fn integrated_pref(v: &str) -> bool {
    match v {
        "integrated" => true,
        "native" => false,
        _ => cfg!(windows),
    }
}

/// Height of the bar.
pub(crate) const HEIGHT: f32 = 30.0;

#[derive(Clone, Copy, PartialEq)]
enum Button {
    Minimize,
    Maximize,
    Close,
}

/// The right part of the bar: drag area with the title, then the buttons.
/// Call inside the bar after the menus. Returns true when the file name in the
/// title was clicked (FS-008: rename).
pub(crate) fn controls(ui: &mut egui::Ui, title: &str, fg: Color32, i18n: &I18n) -> bool {
    let mut rename = false;
    let ctx = ui.ctx().clone();
    let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
    let full = ui.max_rect();
    let avail = ui.available_rect_before_wrap();
    let btn_w = 46.0;
    let h = full.height().max(HEIGHT);
    let buttons_left = full.right() - 3.0 * btn_w;
    // Drag area: everything between the menus and the buttons.
    let drag = Rect::from_min_max(
        pos2(avail.left(), full.top()),
        pos2(buttons_left, full.top() + h),
    );
    if drag.width() > 0.0 {
        let r = ui.interact(drag, ui.id().with("titlebar-drag"), Sense::click_and_drag());
        // The title is centered in the window when it fits, else in the area.
        let font = egui::FontId::proportional(13.0);
        let galley = ui.painter().layout_no_wrap(title.to_owned(), font, fg);
        let mut x = full.center().x - galley.size().x / 2.0;
        if x < drag.left() + 8.0 || x + galley.size().x > drag.right() - 8.0 {
            x = drag.left() + 8.0;
        }
        let y = full.top() + (h - galley.size().y) / 2.0;
        // The file name part of the title (before " - ").
        let name_w = title.split(" - ").next().map_or(0.0, |n| {
            ui.painter()
                .layout_no_wrap(n.to_owned(), egui::FontId::proportional(13.0), fg)
                .size()
                .x
        });
        let name_rect = Rect::from_min_size(pos2(x, y), vec2(name_w, galley.size().y)).expand(2.0);
        let on_name = |p: Option<Pos2>| p.is_some_and(|p| name_rect.contains(p));
        if r.clicked() && on_name(r.interact_pointer_pos()) {
            rename = true;
        } else if r.double_clicked() {
            ctx.send_viewport_cmd(ViewportCommand::Maximized(!maximized));
        } else if r.drag_started() {
            ctx.send_viewport_cmd(ViewportCommand::StartDrag);
        }
        if on_name(r.hover_pos()) {
            ctx.set_cursor_icon(CursorIcon::PointingHand);
        }
        ui.painter()
            .with_clip_rect(drag.shrink2(vec2(4.0, 0.0)))
            .galley(pos2(x, y), galley, fg);
    }
    for (k, b) in [Button::Minimize, Button::Maximize, Button::Close]
        .into_iter()
        .enumerate()
    {
        let rect = Rect::from_min_size(
            pos2(buttons_left + k as f32 * btn_w, full.top()),
            vec2(btn_w, h),
        );
        let r = ui.interact(rect, ui.id().with(("titlebar-btn", k)), Sense::click());
        let label = i18n.tr(match b {
            Button::Minimize => "titlebar-minimize",
            Button::Maximize if maximized => "titlebar-restore",
            Button::Maximize => "titlebar-maximize",
            Button::Close => "titlebar-close",
        });
        r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &label));
        let r = r.on_hover_text(&label);
        let mut color = fg;
        if r.hovered() {
            let bg = if b == Button::Close {
                color = Color32::WHITE;
                Color32::from_rgb(0xe8, 0x11, 0x23)
            } else {
                fg.gamma_multiply(0.12)
            };
            ui.painter().rect_filled(rect, 0.0, bg);
        }
        paint_glyph(
            ui.painter(),
            rect.center(),
            b,
            maximized,
            Stroke::new(1.0, color),
        );
        if r.clicked() {
            ctx.send_viewport_cmd(match b {
                Button::Minimize => ViewportCommand::Minimized(true),
                Button::Maximize => ViewportCommand::Maximized(!maximized),
                Button::Close => ViewportCommand::Close,
            });
        }
    }
    ui.allocate_rect(
        Rect::from_min_max(avail.min, pos2(full.right(), full.top() + h)),
        Sense::hover(),
    );
    rename
}

fn paint_glyph(p: &egui::Painter, c: Pos2, b: Button, maximized: bool, s: Stroke) {
    let r = 5.0;
    match b {
        Button::Minimize => {
            p.line_segment([pos2(c.x - r, c.y), pos2(c.x + r, c.y)], s);
        }
        Button::Maximize if maximized => {
            let back =
                Rect::from_min_max(pos2(c.x - r + 2.0, c.y - r), pos2(c.x + r, c.y + r - 2.0));
            let front = back.translate(vec2(-2.0, 2.0));
            p.line_segment([back.left_top(), back.right_top()], s);
            p.line_segment([back.right_top(), back.right_bottom()], s);
            p.rect_stroke(front, 0.0, s, egui::StrokeKind::Inside);
        }
        Button::Maximize => {
            let rect = Rect::from_center_size(c, vec2(2.0 * r, 2.0 * r));
            p.rect_stroke(rect, 0.0, s, egui::StrokeKind::Inside);
        }
        Button::Close => {
            p.line_segment([pos2(c.x - r, c.y - r), pos2(c.x + r, c.y + r)], s);
            p.line_segment([pos2(c.x - r, c.y + r), pos2(c.x + r, c.y - r)], s);
        }
    }
}

/// The frame of an undecorated window: a hairline border, and resizing from
/// the window edges.
pub(crate) fn resize_edges(ctx: &egui::Context) {
    use egui::ResizeDirection as D;
    let (maximized, fullscreen) = ctx.input(|i| {
        (
            i.viewport().maximized.unwrap_or(false),
            i.viewport().fullscreen.unwrap_or(false),
        )
    });
    if maximized || fullscreen {
        return;
    }
    let screen = ctx.content_rect();
    // A hairline frame where the system would draw the window border.
    ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("inkra-window-frame"),
    ))
    .rect_stroke(
        screen,
        0.0,
        Stroke::new(1.0, Color32::from_black_alpha(60)),
        egui::StrokeKind::Inside,
    );
    let Some(p) = ctx.input(|i| i.pointer.hover_pos()) else {
        return;
    };
    let m = 5.0;
    let (l, r, t, b) = (
        p.x - screen.left() < m,
        screen.right() - p.x < m,
        p.y - screen.top() < m,
        screen.bottom() - p.y < m,
    );
    let dir = match (l, r, t, b) {
        (true, _, true, _) => D::NorthWest,
        (_, true, true, _) => D::NorthEast,
        (true, _, _, true) => D::SouthWest,
        (_, true, _, true) => D::SouthEast,
        (true, ..) => D::West,
        (_, true, ..) => D::East,
        (_, _, true, _) => D::North,
        (.., true) => D::South,
        _ => return,
    };
    ctx.set_cursor_icon(match dir {
        D::North | D::South => CursorIcon::ResizeVertical,
        D::East | D::West => CursorIcon::ResizeHorizontal,
        D::NorthWest | D::SouthEast => CursorIcon::ResizeNwSe,
        D::NorthEast | D::SouthWest => CursorIcon::ResizeNeSw,
    });
    if ctx.input(|i| i.pointer.primary_pressed()) {
        ctx.send_viewport_cmd(ViewportCommand::BeginResize(dir));
    }
}
