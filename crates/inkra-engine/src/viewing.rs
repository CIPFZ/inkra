//! View commands (FS-040, WM-001 … WM-003) and copy-as commands (ED-016).

use serde_json::Value;

use crate::command::{Category, CommandRegistry, FnCommand, Outcome, spec};
use crate::state::Effect;

/// Zoom range and step (FS-040).
pub const ZOOM_MIN: f32 = 0.5;
/// Maximum zoom.
pub const ZOOM_MAX: f32 = 3.0;
const ZOOM_STEP: f32 = 0.1;

fn zoom(z: f32) -> f32 {
    ((z * 10.0).round() / 10.0).clamp(ZOOM_MIN, ZOOM_MAX)
}

/// Registers view and copy-as commands.
pub fn register(r: &mut CommandRegistry) {
    use Category::{Edit, View};
    macro_rules! toggle {
        ($id:literal, $req:literal, $doc:literal, $field:ident) => {
            r.register(
                FnCommand::new(spec($id, View, $req, $doc), |cx, _| {
                    cx.app.view.$field = !cx.app.view.$field;
                    cx.effects.push(Effect::WindowFlags);
                    Ok(Outcome::Done)
                })
                .checked(|cx| Some(cx.app.view.$field)),
            );
        };
    }
    toggle!(
        "view.toggleStatusBar",
        "PR-043",
        "Show or hide the status bar.",
        status_bar
    );
    toggle!(
        "view.toggleMenuBar",
        "TH-008",
        "Show or hide the menu bar (Alt shows it temporarily).",
        menu_bar
    );
    toggle!(
        "view.focusMode",
        "WM-001",
        "Focus mode: dim everything except the current block.",
        focus
    );
    toggle!(
        "view.typewriterMode",
        "WM-002",
        "Typewriter mode: keep the caret line vertically centered.",
        typewriter
    );
    toggle!(
        "view.fullscreen",
        "WM-003",
        "Toggle full screen.",
        fullscreen
    );
    toggle!(
        "view.alwaysOnTop",
        "FS-040",
        "Keep the window above others.",
        always_on_top
    );
    r.register(FnCommand::new(
        spec(
            "view.actualSize",
            View,
            "FS-040",
            "Reset the editor zoom to 100%.",
        ),
        |cx, _| {
            cx.app.view.zoom = 1.0;
            Ok(Outcome::Done)
        },
    ));
    r.register(FnCommand::new(
        spec(
            "view.zoomIn",
            View,
            "FS-040",
            "Zoom the editor in by 10% (max 300%).",
        ),
        |cx, _| {
            cx.app.view.zoom = zoom(cx.app.view.zoom + ZOOM_STEP);
            Ok(Outcome::Done)
        },
    ));
    r.register(FnCommand::new(
        spec(
            "view.zoomOut",
            View,
            "FS-040",
            "Zoom the editor out by 10% (min 50%).",
        ),
        |cx, _| {
            cx.app.view.zoom = zoom(cx.app.view.zoom - ZOOM_STEP);
            Ok(Outcome::Done)
        },
    ));

    // Copy as … (ED-016, ED-130).
    let copy_as: &[(&str, &str, u8)] = &[
        (
            "edit.copyAsMarkdown",
            "Copy the selection as Markdown source.",
            0,
        ),
        (
            "edit.copyAsHtml",
            "Copy the selection as rich HTML (plain text alongside).",
            1,
        ),
        (
            "edit.copyAsPlain",
            "Copy the selection as plain text without Markdown syntax.",
            2,
        ),
        (
            "edit.copyAsHtmlCode",
            "Copy the HTML source of the selection as text.",
            3,
        ),
    ];
    for (id, doc, kind) in copy_as {
        let run: fn(
            &mut crate::state::CommandContext<'_>,
            &Value,
        ) -> crate::command::CommandResult = match kind {
            0 => |cx, _| copy_with(cx, 0),
            1 => |cx, _| copy_with(cx, 1),
            2 => |cx, _| copy_with(cx, 2),
            _ => |cx, _| copy_with(cx, 3),
        };
        r.register(
            FnCommand::new(spec(id, Edit, "ED-016", doc), run)
                .enabled(|cx| !cx.app.editor.sel.is_empty()),
        );
    }
}

/// Plain text of the selection: Markdown parsed and rendered without syntax.
pub fn plain(d: &inkra_doc::Doc, s: inkra_doc::Selection) -> String {
    let src = d.text().slice(s.range()).into_owned();
    let p = inkra_markdown::parse(src.as_str(), d.options());
    let mut out = String::new();
    for b in &p.blocks {
        b.walk(&mut |b| {
            if b.kind.has_inlines() {
                if !out.is_empty() {
                    out.push('\n');
                }
                inkra_markdown::walk::plain_text(src.as_str(), &b.inlines, &mut out);
            } else if let inkra_markdown::BlockKind::CodeBlock { .. } = b.kind {
                for l in &b.lines {
                    if !out.is_empty() {
                        out.push('\n');
                    }
                    out.push_str(&src[l.span.clone()]);
                }
            }
        });
    }
    out
}

fn copy_with(cx: &mut crate::state::CommandContext<'_>, kind: u8) -> crate::command::CommandResult {
    let e = &cx.app.editor;
    let (d, s) = (&e.doc, e.sel);
    let (text, html) = match kind {
        0 => (d.text().slice(s.range()).into_owned(), None),
        1 => {
            let src = d.text().slice(s.range()).into_owned();
            (
                plain(d, s),
                Some(inkra_markdown::html::to_html(&src, d.options())),
            )
        }
        2 => (plain(d, s), None),
        _ => {
            let src = d.text().slice(s.range()).into_owned();
            (inkra_markdown::html::to_html(&src, d.options()), None)
        }
    };
    cx.effects.push(Effect::SetClipboard { text, html });
    Ok(Outcome::Done)
}
