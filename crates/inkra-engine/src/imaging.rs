//! Image operations (ED-093): the image at the caret (Markdown `![alt](src
//! "title")` or an HTML `<img>`), and commands to copy its address, reveal or
//! open it, resize it, edit its alt text and title, make a remote image local,
//! reload and delete it.

use std::path::{Path, PathBuf};

use inkra_doc::{Selection, Transaction};
use serde_json::Value;

use crate::command::{
    Category, CommandError, CommandRegistry, CommandResult, CommandSpec, FnCommand, Outcome, spec,
};
use crate::editor::context;
use crate::state::{AppState, CommandContext, Effect};

/// An image in the document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageRef {
    /// Absolute source range of the whole image syntax.
    pub range: std::ops::Range<usize>,
    /// Source as written.
    pub src: String,
    /// Alt text.
    pub alt: String,
    /// Title.
    pub title: Option<String>,
    /// Width in pixels (`<img width>`).
    pub width: Option<u32>,
    /// Written as an HTML tag.
    pub html: bool,
}

/// The image at (or right before) `pos`.
pub fn image_at(app: &AppState, pos: usize) -> Option<ImageRef> {
    let doc = &app.editor.doc;
    let path = context::path(doc, pos);
    let leaf = path.last()?;
    let text = doc.text();
    // An `<img>` on a line of its own is an HTML block.
    if matches!(leaf.block.kind, inkra_markdown::BlockKind::HtmlBlock { .. }) {
        let span = leaf.abs_range(&leaf.block.span);
        let src = text.slice(span.clone()).into_owned();
        let at = src.find("<img")?;
        if src[at + 4..].contains("<img") {
            return None;
        }
        let end = at + src[at..].find('>')? + 1;
        let tag = &src[at..end];
        let attr = |k: &str| inkra_html::render::tag_attr(tag, k);
        return Some(ImageRef {
            range: span.start + at..span.start + end,
            src: attr("src").unwrap_or_default(),
            alt: attr("alt").unwrap_or_default(),
            title: attr("title"),
            width: attr("width").and_then(|w| w.trim_end_matches("px").parse().ok()),
            html: true,
        });
    }
    let mut found = None;
    for i in &leaf.block.inlines {
        i.walk(&mut |n| {
            let span = leaf.abs_range(&n.span);
            if span.start > pos || pos > span.end || found.is_some() {
                return;
            }
            match &n.kind {
                inkra_markdown::InlineKind::Image {
                    dest,
                    title,
                    text: alt,
                    ..
                } => {
                    let alt = text.slice(leaf.abs_range(alt)).into_owned();
                    found = Some(ImageRef {
                        range: span,
                        src: dest.clone(),
                        alt,
                        title: title.clone(),
                        width: None,
                        html: false,
                    });
                }
                inkra_markdown::InlineKind::Html => {
                    let src = text.slice(span.clone()).into_owned();
                    if inkra_html::render::inline_tag(&src)
                        .is_some_and(|(n, closing)| n == "img" && !closing)
                    {
                        let attr = |k: &str| inkra_html::render::tag_attr(&src, k);
                        found = Some(ImageRef {
                            range: span,
                            src: attr("src").unwrap_or_default(),
                            alt: attr("alt").unwrap_or_default(),
                            title: attr("title"),
                            width: attr("width")
                                .and_then(|w| w.trim_end_matches("px").parse().ok()),
                            html: true,
                        });
                    }
                }
                _ => {}
            }
        });
    }
    found
}

/// The image at the caret.
pub fn current(app: &AppState) -> Option<ImageRef> {
    let e = &app.editor;
    image_at(app, e.sel.head).or_else(|| image_at(app, e.sel.head.saturating_sub(1)))
}

fn attr_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
}

/// The syntax for an image: Markdown unless a width needs an `<img>` tag.
pub fn render(src: &str, alt: &str, title: Option<&str>, width: Option<u32>) -> String {
    match width {
        Some(w) => {
            let mut t = format!(
                "<img src=\"{}\" alt=\"{}\"",
                attr_escape(src),
                attr_escape(alt)
            );
            if let Some(ti) = title.filter(|t| !t.is_empty()) {
                t.push_str(&format!(" title=\"{}\"", attr_escape(ti)));
            }
            t.push_str(&format!(" width=\"{w}\">"));
            t
        }
        None => {
            let dest = if src.contains([' ', '(', ')']) {
                format!("<{src}>")
            } else {
                src.to_owned()
            };
            let alt = alt.replace('[', "\\[").replace(']', "\\]");
            match title.filter(|t| !t.is_empty()) {
                Some(t) => format!("![{alt}]({dest} \"{}\")", t.replace('"', "\\\"")),
                None => format!("![{alt}]({dest})"),
            }
        }
    }
}

/// An `<img>` scaled with `style="zoom:…%"` (ED-090, ED-093).
pub fn render_zoom(src: &str, alt: &str, title: Option<&str>, pct: u32) -> String {
    let mut t = format!(
        "<img src=\"{}\" alt=\"{}\"",
        attr_escape(src),
        attr_escape(alt)
    );
    if let Some(ti) = title.filter(|t| !t.is_empty()) {
        t.push_str(&format!(" title=\"{}\"", attr_escape(ti)));
    }
    t.push_str(&format!(" style=\"zoom:{pct}%;\">"));
    t
}

/// The local file of an image source, if any.
pub fn local_path(app: &AppState, src: &str) -> Option<PathBuf> {
    if src.contains("://") && !src.starts_with("file://") || src.starts_with("data:") {
        return None;
    }
    inkra_export::html::resolve_image(app.editor.path.as_deref(), src)
}

fn fail(id: &str, msg: impl std::fmt::Display) -> CommandError {
    CommandError::Failed {
        id: id.to_owned(),
        msg: msg.to_string(),
    }
}

fn params(mut s: CommandSpec, p: &'static str) -> CommandSpec {
    s.params = Some(p);
    s
}

fn replace(cx: &mut CommandContext<'_>, img: &ImageRef, text: &str) -> CommandResult {
    let e = &mut cx.app.editor;
    let caret = img.range.start + text.len();
    let tx = Transaction::single(img.range.clone(), text)
        .with_selection_before(e.sel)
        .with_selection_after(Selection::caret(caret));
    e.apply(tx, inkra_doc::UndoPolicy::NewGroup);
    Ok(Outcome::Done)
}

fn has_image(cx: &CommandContext<'_>) -> bool {
    current(cx.app).is_some()
}

/// Registers image commands.
pub fn register(r: &mut CommandRegistry) {
    use Category::Format;
    r.register(
        FnCommand::new(
            spec(
                "image.copyAddress",
                Format,
                "ED-093",
                "Copy the address of the image at the caret.",
            ),
            |cx, _| {
                let img = current(cx.app)
                    .ok_or_else(|| fail("image.copyAddress", "no image at the caret"))?;
                cx.effects.push(Effect::SetClipboard {
                    text: img.src,
                    html: None,
                });
                Ok(Outcome::Done)
            },
        )
        .enabled(has_image),
    );
    r.register(
        FnCommand::new(
            spec(
                "image.copy",
                Format,
                "ED-093",
                "Copy the image at the caret as a bitmap.",
            ),
            |cx, _| {
                let img =
                    current(cx.app).ok_or_else(|| fail("image.copy", "no image at the caret"))?;
                let source = local_path(cx.app, &img.src)
                    .map_or(img.src.clone(), |p| p.to_string_lossy().into_owned());
                cx.effects.push(Effect::CopyImage(source));
                Ok(Outcome::Done)
            },
        )
        .enabled(has_image),
    );
    r.register(
        FnCommand::new(
            spec(
                "image.reveal",
                Format,
                "ED-093",
                "Show the image file in the file manager.",
            ),
            |cx, _| {
                let img =
                    current(cx.app).ok_or_else(|| fail("image.reveal", "no image at the caret"))?;
                let p = local_path(cx.app, &img.src)
                    .ok_or_else(|| fail("image.reveal", "not a local image"))?;
                cx.effects.push(Effect::Reveal(p));
                Ok(Outcome::Done)
            },
        )
        .enabled(|cx| current(cx.app).is_some_and(|i| local_path(cx.app, &i.src).is_some())),
    );
    r.register(
        FnCommand::new(
            spec(
                "image.open",
                Format,
                "ED-093",
                "Open the image with the system's default viewer.",
            ),
            |cx, _| {
                let img =
                    current(cx.app).ok_or_else(|| fail("image.open", "no image at the caret"))?;
                let target = local_path(cx.app, &img.src)
                    .map_or(img.src.clone(), |p| p.to_string_lossy().into_owned());
                cx.effects.push(Effect::OpenExternal(target));
                Ok(Outcome::Done)
            },
        )
        .enabled(has_image),
    );
    r.register(FnCommand::new(
        params(
            spec("image.relink", Format, "ED-090", "Point every image with source `from` at another file (`path`; asks for it without)."),
            r#"{"type":"object","required":["from"],"properties":{"from":{"type":"string"},"path":{"type":"string"}}}"#,
        ),
        |cx, p| {
            let from = p.get("from").and_then(Value::as_str).unwrap_or_default().to_owned();
            let Some(path) = p.get("path").and_then(Value::as_str).map(PathBuf::from) else {
                cx.effects.push(Effect::RelinkImage(from));
                return Ok(Outcome::Done);
            };
            let rel = cx.app.prefs.bool("image.preferRelative");
            let new = inkra_workspace::assets::markdown_src(cx.app.editor.path.as_deref(), &path, rel);
            let e = &mut cx.app.editor;
            let text = e.doc.text();
            let mut edits = Vec::new();
            let ast = e.doc.ast();
            for i in 0..ast.len() {
                let Some(r) = ast.get(i) else { continue };
                r.block.walk(&mut |b| {
                    for il in &b.inlines {
                        il.walk(&mut |n| match &n.kind {
                            inkra_markdown::InlineKind::Image { dest, syntax: inkra_markdown::ast::LinkSyntax::Inline { dest: ds }, .. } if *dest == from && !ds.is_empty() => {
                                edits.push(inkra_doc::Edit { range: r.abs_span(ds), text: new.clone() });
                            }
                            inkra_markdown::InlineKind::Html => {
                                let span = r.abs_span(&n.span);
                                let src = text.slice(span.clone()).into_owned();
                                if src.starts_with("<img") && inkra_html::render::tag_attr(&src, "src").as_deref() == Some(from.as_str()) {
                                    edits.push(inkra_doc::Edit { range: span, text: src.replacen(&from, &new, 1) });
                                }
                            }
                            _ => {}
                        });
                    }
                });
            }
            if edits.is_empty() {
                return Ok(Outcome::Done);
            }
            edits.sort_by_key(|x| x.range.start);
            let tx = Transaction::many(edits).with_selection_before(e.sel);
            let after = tx.map_selection(e.sel);
            e.apply(tx.with_selection_after(after), inkra_doc::UndoPolicy::NewGroup);
            Ok(Outcome::Done)
        },
    ));
    r.register(
        FnCommand::new(
            spec(
                "image.openWindow",
                Format,
                "ED-093",
                "Show the image at the caret in its own window (zoomable).",
            ),
            |cx, _| {
                let img = current(cx.app)
                    .ok_or_else(|| fail("image.openWindow", "no image at the caret"))?;
                cx.effects.push(Effect::ShowImage(img.src));
                Ok(Outcome::Done)
            },
        )
        .enabled(has_image),
    );
    r.register(
        FnCommand::new(
            spec(
                "image.reload",
                Format,
                "ED-093",
                "Load the image at the caret again.",
            ),
            |cx, _| {
                let img =
                    current(cx.app).ok_or_else(|| fail("image.reload", "no image at the caret"))?;
                cx.effects.push(Effect::ReloadImage(img.src));
                Ok(Outcome::Done)
            },
        )
        .enabled(has_image),
    );
    r.register(
        FnCommand::new(
            params(
                spec("image.resize", Format, "ED-093", "Set the display width of the image at the caret (`width` in px; none restores the natural size)."),
                r#"{"type":"object","properties":{"width":{"type":"integer","minimum":1},"natural":{"type":"number"}}}"#,
            ),
            |cx, p| {
                let img = current(cx.app).ok_or_else(|| fail("image.resize", "no image at the caret"))?;
                let width = p.get("width").and_then(Value::as_u64).map(|w| w as u32);
                // ED-093: write `zoom` instead of a width when so set and the
                // natural width is known.
                let natural = p.get("natural").and_then(Value::as_f64).filter(|n| *n > 0.0);
                let t = match (width, natural) {
                    (Some(w), Some(n)) if cx.app.prefs.string("image.resizeWrites") == "zoom" => {
                        let pct = (f64::from(w) / n * 100.0).round().max(1.0);
                        render_zoom(&img.src, &img.alt, img.title.as_deref(), pct as u32)
                    }
                    _ => render(&img.src, &img.alt, img.title.as_deref(), width),
                };
                replace(cx, &img, &t)
            },
        )
        .enabled(has_image),
    );
    r.register(
        FnCommand::new(
            params(
                spec("format.imageOps", Format, "ED-093", "Edit the alt text, title and width of the image at the caret (opens a dialog without parameters)."),
                r#"{"type":"object","properties":{"alt":{"type":"string"},"title":{"type":"string"},"width":{"type":["integer","null"]}}}"#,
            ),
            |cx, p| {
                let img = current(cx.app).ok_or_else(|| fail("format.imageOps", "no image at the caret"))?;
                if p.as_object().is_none_or(|o| o.is_empty()) {
                    cx.effects.push(Effect::ImageDialog(img));
                    return Ok(Outcome::Done);
                }
                let alt = p.get("alt").and_then(Value::as_str).unwrap_or(&img.alt).to_owned();
                let title = p.get("title").and_then(Value::as_str).map(str::to_owned).or(img.title.clone());
                let width = match p.get("width") {
                    Some(Value::Null) => None,
                    Some(v) => v.as_u64().map(|w| w as u32),
                    None => img.width,
                };
                let t = render(&img.src, &alt, title.as_deref(), width);
                replace(cx, &img, &t)
            },
        )
        .enabled(has_image),
    );
    r.register(
        FnCommand::new(
            spec(
                "image.delete",
                Format,
                "ED-093",
                "Remove the image at the caret from the document.",
            ),
            |cx, _| {
                let img =
                    current(cx.app).ok_or_else(|| fail("image.delete", "no image at the caret"))?;
                replace(cx, &img, "")
            },
        )
        .enabled(has_image),
    );
    r.register(
        FnCommand::new(
            params(
                spec("image.toLocal", Format, "ED-093", "Download the remote image at the caret into the assets folder and point to the copy."),
                r#"{"type":"object","properties":{"file":{"type":"string"}}}"#,
            ),
            |cx, p| {
                let img = current(cx.app).ok_or_else(|| fail("image.toLocal", "no image at the caret"))?;
                match p.get("file").and_then(Value::as_str) {
                    // The download finished: copy it in like an inserted image.
                    Some(file) => {
                        let placed = crate::formatting::place_image(cx, Path::new(file), true).map_err(|e| fail("image.toLocal", e))?;
                        let rel = cx.app.prefs.bool("image.preferRelative");
                        let src = inkra_workspace::assets::markdown_src(cx.app.editor.path.as_deref(), &placed, rel);
                        let t = render(&src, &img.alt, img.title.as_deref(), img.width);
                        replace(cx, &img, &t)
                    }
                    None => {
                        cx.effects.push(Effect::DownloadImage(img.src));
                        Ok(Outcome::Done)
                    }
                }
            },
        )
        .enabled(|cx| current(cx.app).is_some_and(|i| i.src.starts_with("http://") || i.src.starts_with("https://"))),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn engine(text: &str, caret: usize) -> crate::Engine {
        let mut e = crate::Engine::new(crate::Preferences::default());
        e.state.editor = crate::editor::Editor::from_text(text);
        e.state.editor.sel = Selection::caret(caret);
        e
    }

    #[test]
    fn finds_resizes_edits_and_deletes() {
        let mut e = engine("a ![cat](img/cat.png \"Cat\") b", 5);
        let img = current(&e.state).unwrap();
        assert_eq!(
            (img.src.as_str(), img.alt.as_str(), img.title.as_deref()),
            ("img/cat.png", "cat", Some("Cat"))
        );
        e.execute("image.resize", &json!({ "width": 120 })).unwrap();
        assert_eq!(
            e.state.editor.text(),
            "a <img src=\"img/cat.png\" alt=\"cat\" title=\"Cat\" width=\"120\"> b"
        );
        e.state.editor.sel = Selection::caret(6);
        let img = current(&e.state).unwrap();
        assert!(img.html && img.width == Some(120));
        e.execute("format.imageOps", &json!({ "alt": "a cat", "width": null }))
            .unwrap();
        assert_eq!(e.state.editor.text(), "a ![a cat](img/cat.png \"Cat\") b");
        e.state.editor.sel = Selection::caret(4);
        e.execute("image.copyAddress", &json!({})).unwrap();
        assert!(
            matches!(e.take_effects().as_slice(), [Effect::SetClipboard { text, .. }] if text == "img/cat.png")
        );
        e.execute("image.delete", &json!({})).unwrap();
        assert_eq!(e.state.editor.text(), "a  b");
        // An image alone on its line (an HTML block once resized).
        let mut e = engine("![x](y.png)\n", 3);
        e.execute("image.resize", &json!({ "width": 50 })).unwrap();
        assert_eq!(
            e.state.editor.text(),
            "<img src=\"y.png\" alt=\"x\" width=\"50\">\n"
        );
        e.state.editor.sel = Selection::caret(2);
        assert_eq!(current(&e.state).map(|i| i.width), Some(Some(50)));
        let mut e = engine("![z](z.png)", 2);
        e.execute(
            "app.setPreference",
            &json!({ "key": "image.resizeWrites", "value": "zoom" }),
        )
        .unwrap();
        e.execute("image.resize", &json!({ "width": 50, "natural": 200.0 }))
            .unwrap();
        assert_eq!(
            e.state.editor.text(),
            "<img src=\"z.png\" alt=\"z\" style=\"zoom:25%;\">"
        );
        let mut e = engine("![a](old.png) and ![b](old.png) <img src=\"old.png\">", 0);
        e.execute(
            "image.relink",
            &json!({ "from": "old.png", "path": "/abs/new.png" }),
        )
        .unwrap();
        assert_eq!(e.state.editor.text().matches("/abs/new.png").count(), 3);
        let mut e = engine("plain", 2);
        assert!(!e.is_enabled("image.resize"));
    }
}
