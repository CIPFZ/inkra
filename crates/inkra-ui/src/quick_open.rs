//! Quick open (FS-030): a centered palette that fuzzy-matches the root folder's
//! files and the recent files.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use egui::text::LayoutJob;
use egui::{FontId, Key, Sense, TextFormat, pos2, vec2};
use inkra_i18n::I18n;
use inkra_layout::Style;
use inkra_workspace::fuzzy::{Fuzzy, Hit};
use inkra_workspace::index::FileIndex;

struct Candidate {
    path: PathBuf,
    /// What is matched and shown (root-relative path, or the full path).
    key: String,
}

/// Quick-open state.
#[derive(Default)]
pub struct QuickOpen {
    open: bool,
    query: String,
    selected: usize,
    focus: bool,
    candidates: Vec<Candidate>,
    built_for: Option<(u64, usize, Option<PathBuf>)>,
    hits: Vec<Hit>,
    hits_for: Option<String>,
    fuzzy: Fuzzy,
    /// Time of the last ranking (perf overlay, tests).
    pub last_rank_ms: f32,
}

/// What the user chose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    /// File.
    pub path: PathBuf,
    /// Open in a new window (`Ctrl+Enter`).
    pub new_window: bool,
}

impl QuickOpen {
    /// Shows the palette.
    pub fn open(&mut self) {
        self.open = true;
        self.focus = true;
        self.query.clear();
        self.selected = 0;
        self.hits_for = None;
    }

    /// Whether the palette is shown.
    pub fn is_open(&self) -> bool {
        self.open
    }

    fn rebuild(&mut self, index: Option<&FileIndex>, recent: &[PathBuf]) {
        let key = (
            index.map_or(0, FileIndex::version),
            recent.len(),
            index.map(|i| i.root().to_owned()),
        );
        if self.built_for.as_ref() == Some(&key) {
            return;
        }
        self.built_for = Some(key);
        self.hits_for = None;
        let mut seen: HashSet<PathBuf> = HashSet::new();
        let mut c = Vec::new();
        let root = index.map(|i| i.root().to_owned());
        for p in recent {
            if seen.insert(p.clone()) {
                let key = match &root {
                    Some(r) if p.starts_with(r) => p
                        .strip_prefix(r)
                        .unwrap_or(p)
                        .to_string_lossy()
                        .replace('\\', "/"),
                    _ => p.to_string_lossy().replace('\\', "/"),
                };
                c.push(Candidate {
                    path: p.clone(),
                    key,
                });
            }
        }
        if let Some(ix) = index {
            ix.with_files(|files| {
                for f in files {
                    if seen.insert(f.path.clone()) {
                        c.push(Candidate {
                            path: f.path.clone(),
                            key: f.rel.clone(),
                        });
                    }
                }
            });
        }
        self.candidates = c;
    }

    fn rank(&mut self) {
        if self.hits_for.as_deref() == Some(self.query.as_str()) {
            return;
        }
        let t = std::time::Instant::now();
        let keys: Vec<&str> = self.candidates.iter().map(|c| c.key.as_str()).collect();
        self.hits = self.fuzzy.rank(&self.query, &keys, 50);
        self.last_rank_ms = t.elapsed().as_secs_f32() * 1000.0;
        self.hits_for = Some(self.query.clone());
        self.selected = 0;
    }

    /// Draws the palette; returns the chosen file.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        index: Option<&FileIndex>,
        recent: &[PathBuf],
        i18n: &I18n,
        style: &Style,
    ) -> Option<Choice> {
        if !self.open {
            return None;
        }
        self.rebuild(index, recent);
        self.rank();
        let mut choice = None;
        let (up, down, enter, esc, cmd) = ctx.input(|i| {
            (
                i.key_pressed(Key::ArrowUp),
                i.key_pressed(Key::ArrowDown),
                i.key_pressed(Key::Enter),
                i.key_pressed(Key::Escape),
                i.modifiers.command,
            )
        });
        if esc {
            self.open = false;
            return None;
        }
        let n = self.hits.len();
        if down && n > 0 {
            self.selected = (self.selected + 1) % n;
        }
        if up && n > 0 {
            self.selected = (self.selected + n - 1) % n;
        }
        if enter && let Some(h) = self.hits.get(self.selected) {
            choice = Some(Choice {
                path: self.candidates[h.index].path.clone(),
                new_window: cmd,
            });
        }
        let screen = ctx.content_rect();
        let width = (screen.width() * 0.6).clamp(360.0, 680.0);
        let area = egui::Area::new(egui::Id::new("inkra-quick-open"))
            .order(egui::Order::Foreground)
            .fixed_pos(pos2(screen.center().x - width / 2.0, screen.top() + 60.0));
        let resp = area.show(ctx, |ui| {
            egui::Frame::popup(ui.style())
                .inner_margin(egui::Margin::same(8))
                .show(ui, |ui| {
                    ui.set_width(width);
                    let r = ui.add(
                        egui::TextEdit::singleline(&mut self.query)
                            .hint_text(i18n.tr("quick-open-placeholder"))
                            .desired_width(f32::INFINITY)
                            .font(FontId::proportional(15.0)),
                    );
                    if self.focus {
                        r.request_focus();
                        self.focus = false;
                    }
                    if r.changed() {
                        self.selected = 0;
                    }
                    ui.add_space(4.0);
                    match (index, self.hits.is_empty()) {
                        (None, _) => {
                            ui.weak(i18n.tr("quick-open-no-root"));
                        }
                        (Some(ix), true) if !ix.ready() => {
                            ui.weak(i18n.tr("quick-open-indexing"));
                        }
                        (_, true) => {
                            ui.weak(i18n.tr("quick-open-none"));
                        }
                        _ => {}
                    }
                    egui::ScrollArea::vertical()
                        .max_height(screen.height() * 0.5)
                        .show(ui, |ui| {
                            for (k, h) in self.hits.iter().enumerate() {
                                let c = &self.candidates[h.index];
                                let (rect, row) = ui.allocate_exact_size(
                                    vec2(ui.available_width(), 38.0),
                                    Sense::click(),
                                );
                                crate::sidebar::accessible(&row, egui::WidgetType::Button, &c.key);
                                if k == self.selected {
                                    ui.painter().rect_filled(rect, 4.0, style.sidebar_active_bg);
                                    if up || down {
                                        row.scroll_to_me(None);
                                    }
                                } else if row.hovered() {
                                    ui.painter().rect_filled(
                                        rect,
                                        4.0,
                                        style.sidebar_active_bg.gamma_multiply(0.5),
                                    );
                                }
                                let name = c
                                    .path
                                    .file_name()
                                    .map(|n| n.to_string_lossy().into_owned())
                                    .unwrap_or_default();
                                let name_start =
                                    c.key.chars().count().saturating_sub(name.chars().count())
                                        as u32;
                                let name_idx: Vec<u32> = h
                                    .indices
                                    .iter()
                                    .filter(|&&i| i >= name_start)
                                    .map(|i| i - name_start)
                                    .collect();
                                let base = TextFormat {
                                    font_id: FontId::proportional(14.0),
                                    color: style.fg,
                                    ..Default::default()
                                };
                                let job = highlight(&name, &name_idx, base, style.accent);
                                let g = ui.painter().layout_job(job);
                                ui.painter().galley(
                                    pos2(rect.left() + 8.0, rect.top() + 3.0),
                                    g,
                                    style.fg,
                                );
                                let dir = Path::new(&c.key)
                                    .parent()
                                    .map(|d| d.to_string_lossy().into_owned())
                                    .unwrap_or_default();
                                let meta = std::fs::metadata(&c.path)
                                    .ok()
                                    .and_then(|m| m.modified().ok());
                                let detail = format!(
                                    "{dir}   {}",
                                    crate::sidebar::relative_time(i18n, meta)
                                );
                                let dg = ui.painter().layout_no_wrap(
                                    detail,
                                    FontId::proportional(11.5),
                                    style.fg_muted,
                                );
                                ui.painter().with_clip_rect(rect).galley(
                                    pos2(rect.left() + 8.0, rect.top() + 21.0),
                                    dg,
                                    style.fg_muted,
                                );
                                if row.clicked() {
                                    choice = Some(Choice {
                                        path: c.path.clone(),
                                        new_window: ui.input(|i| i.modifiers.command),
                                    });
                                }
                            }
                        });
                });
        });
        // Clicking elsewhere closes it.
        if ctx.input(|i| i.pointer.any_pressed())
            && !resp.response.contains_pointer()
            && !resp.response.hovered()
        {
            let inside = ctx
                .input(|i| i.pointer.interact_pos())
                .is_some_and(|p| resp.response.rect.contains(p));
            if !inside {
                self.open = false;
            }
        }
        if choice.is_some() {
            self.open = false;
        }
        // Keys used by the palette must not reach the editor.
        ctx.input_mut(|i| {
            i.events.retain(|e| {
                !matches!(
                    e,
                    egui::Event::Key {
                        key: Key::ArrowUp | Key::ArrowDown | Key::Enter | Key::Escape,
                        ..
                    }
                )
            })
        });
        choice
    }
}

fn highlight(text: &str, idx: &[u32], base: TextFormat, hl: egui::Color32) -> LayoutJob {
    let mut job = LayoutJob::default();
    for (i, c) in text.chars().enumerate() {
        let mut f = base.clone();
        if idx.contains(&(i as u32)) {
            f.color = hl;
        }
        let mut b = [0u8; 4];
        job.append(c.encode_utf8(&mut b), 0.0, f);
    }
    job
}
