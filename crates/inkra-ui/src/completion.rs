//! Inline completion popups (ED-103 emoji, ED-105 heading links and footnotes):
//! detected from the text before the caret, chosen with Up/Down and accepted
//! with Enter or Tab; typing continues normally.

use inkra_engine::Engine;
use serde_json::{Value, json};

/// What is being completed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// `:smi`
    Emoji,
    /// `[intr` → `[Introduction](#introduction)`
    Heading,
    /// `[^no` → `[^note]`
    Footnote,
}

/// An open completion.
#[derive(Debug, Clone, PartialEq)]
pub struct Completion {
    /// Kind.
    pub kind: Kind,
    /// Source range being replaced.
    pub start: usize,
    /// End (the caret).
    pub end: usize,
    /// `(label, replacement)`.
    pub items: Vec<(String, String)>,
    /// Selected item.
    pub selected: usize,
}

fn word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '+' | '-')
}

/// Finds a completion at the caret, if any.
pub fn detect(engine: &Engine) -> Option<Completion> {
    let e = &engine.state.editor;
    if !e.sel.is_empty() || e.mode != inkra_engine::editor::ViewMode::Live || e.preedit.is_some() {
        return None;
    }
    let caret = e.sel.head;
    let t = e.doc.text();
    let ls = t.line_to_byte(t.byte_to_line(caret));
    let before = t.slice(ls..caret).into_owned();
    if inkra_engine::editor::context::in_code_content(&e.doc, caret)
        || inkra_engine::editor::context::in_inline_code(&e.doc, caret)
    {
        return None;
    }
    let p = &engine.state.prefs;
    // Emoji: `:` + at least two word characters, not after a letter or digit.
    if p.bool("editor.emojiCompletion")
        && let Some(colon) = before.rfind(':')
    {
        let word = &before[colon + 1..];
        let prev = before[..colon].chars().next_back();
        if word.len() >= 2
            && word.chars().all(word_char)
            && prev.is_none_or(|c| !c.is_alphanumeric())
        {
            let shortcode = p.bool("editor.emojiAsShortcode");
            let items: Vec<(String, String)> = inkra_markdown::emoji::complete(word, 10)
                .into_iter()
                .map(|(k, v)| {
                    (
                        format!("{v}  :{k}:"),
                        if shortcode {
                            format!(":{k}:")
                        } else {
                            v.to_owned()
                        },
                    )
                })
                .collect();
            if !items.is_empty() {
                // A `:` typed right after keeps working as the end of a shortcode.
                return Some(Completion {
                    kind: Kind::Emoji,
                    start: ls + colon,
                    end: caret,
                    items,
                    selected: 0,
                });
            }
        }
    }
    if !p.bool("editor.linkCompletion") {
        return None;
    }
    // `[^label` and `[text`, with no `]` yet.
    let open = before.rfind('[')?;
    // An auto-paired `]` right after the caret is replaced too.
    let after_close = usize::from(t.slice(caret..(caret + 1).min(t.len())) == "]");
    let rest = &before[open + 1..];
    if rest.contains(']') || rest.contains('(') || rest.len() > 40 {
        return None;
    }
    if let Some(filter) = rest.strip_prefix('^') {
        let mut labels = Vec::new();
        let ast = e.doc.ast();
        for i in 0..ast.len() {
            if let Some(r) = ast.get(i) {
                r.block.walk(&mut |b| {
                    if let inkra_markdown::BlockKind::FootnoteDef { label, .. } = &b.kind
                        && label.to_lowercase().contains(&filter.to_lowercase())
                        && !labels.contains(label)
                    {
                        labels.push(label.clone());
                    }
                });
            }
        }
        let items: Vec<(String, String)> = labels
            .into_iter()
            .take(10)
            .map(|l| (format!("[^{l}]"), format!("[^{l}]")))
            .collect();
        return (!items.is_empty()).then(|| Completion {
            kind: Kind::Footnote,
            start: ls + open,
            end: caret + after_close,
            items,
            selected: 0,
        });
    }
    // Headings only right after `[` was typed (a filter may follow).
    let blocks = e.doc.ast().to_blocks();
    let heads = inkra_markdown::walk::headings(e.doc.text(), &blocks);
    if heads.is_empty() {
        return None;
    }
    let texts: Vec<&str> = heads.iter().map(|h| h.text.as_str()).collect();
    let hits = inkra_workspace::fuzzy::Fuzzy::text().rank(rest, &texts, 10);
    let items: Vec<(String, String)> = hits
        .into_iter()
        .map(|h| {
            let hd = &heads[h.index];
            (
                format!(
                    "{}{}",
                    "  ".repeat(usize::from(hd.level.saturating_sub(1))),
                    hd.text
                ),
                format!("[{}](#{})", hd.text, hd.anchor),
            )
        })
        .collect();
    (!items.is_empty()).then(|| Completion {
        kind: Kind::Heading,
        start: ls + open,
        end: caret + after_close,
        items,
        selected: 0,
    })
}

/// Keys for an open completion: returns the command to run when an item is
/// accepted, `Some(None)` when the key was consumed otherwise, `None` if not
/// handled.
pub fn handle_key(c: &mut Completion, key: egui::Key) -> Option<Option<(String, Value)>> {
    match key {
        egui::Key::ArrowDown => {
            c.selected = (c.selected + 1) % c.items.len().max(1);
            Some(None)
        }
        egui::Key::ArrowUp => {
            c.selected = (c.selected + c.items.len().max(1) - 1) % c.items.len().max(1);
            Some(None)
        }
        egui::Key::Enter | egui::Key::Tab => {
            let (_, text) = c.items.get(c.selected)?;
            Some(Some((
                "edit.replaceRange".into(),
                json!({ "start": c.start, "end": c.end, "text": text }),
            )))
        }
        _ => None,
    }
}

/// Draws the popup below `caret` (screen rect).
pub fn show(
    ctx: &egui::Context,
    c: &mut Completion,
    caret: egui::Rect,
    id: egui::Id,
) -> Option<(String, Value)> {
    let mut chosen = None;
    egui::Area::new(id.with("completion"))
        .order(egui::Order::Foreground)
        .fixed_pos(egui::pos2(caret.left(), caret.bottom() + 4.0))
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_min_width(180.0);
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                for (k, (label, text)) in c.items.iter().enumerate() {
                    let r = ui.selectable_label(k == c.selected, label);
                    if r.clicked() {
                        chosen = Some((
                            "edit.replaceRange".into(),
                            json!({ "start": c.start, "end": c.end, "text": text }),
                        ));
                    }
                }
            });
        });
    chosen
}
