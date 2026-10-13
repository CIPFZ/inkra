//! The sidebar (FS-020 … FS-024, FS-031): outline, article list, file tree and
//! global search. One sidebar model serves every window (the root folder is
//! shared); the outline follows the active document.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use egui::text::LayoutJob;
use egui::{Color32, FontId, Rect, Sense, TextFormat, pos2, vec2};
use inkra_engine::Engine;
use inkra_engine::state::SidebarView;
use inkra_i18n::I18n;
use inkra_layout::Style;
use inkra_markdown::walk::HeadingInfo;
use inkra_text::sort::natural_cmp;
use inkra_workspace::index::{Articles, FileIndex, RootWatcher};
use inkra_workspace::search::{self, Event as SearchEvent, FileHits, Query, Search};
use inkra_workspace::tree::{self, Entry, Filter};
use serde_json::{Value, json};

const ROW_H: f32 = 22.0;

/// What the sidebar asks the shell to do.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// Run a command.
    Invoke(String, Value),
    /// Scroll the active editor so that `pos` is at the top (outline click).
    JumpTo(usize),
    /// Open `path` and show the search match on `line`.
    OpenAt {
        /// File.
        path: PathBuf,
        /// 1-based line.
        line: u64,
        /// The search, for the in-document find state.
        query: Query,
    },
    /// Ask for a folder to move `path` into.
    PickMoveTarget(PathBuf),
    /// Ask for a new root folder.
    PickRoot,
}

#[derive(Default)]
struct SearchUi {
    query: Query,
    running: Option<Search>,
    results: Vec<FileHits>,
    collapsed: HashSet<PathBuf>,
    status: Option<SearchEvent>,
    focus: bool,
    last_edit: f64,
    dirty: bool,
}

/// Sidebar state.
pub struct Sidebar {
    root: Option<PathBuf>,
    filter: Filter,
    filter_key: String,
    watcher: Option<RootWatcher>,
    /// Quick-open index of the root folder.
    pub index: Option<FileIndex>,
    articles: Articles,
    listings: HashMap<PathBuf, Vec<Entry>>,
    expanded: HashSet<PathBuf>,
    selected: Option<PathBuf>,
    renaming: Option<(PathBuf, String, bool)>,
    reveal: Option<PathBuf>,
    tree_id: egui::Id,
    last_active: Option<PathBuf>,
    outline_filter: String,
    outline_cache: (u64, Option<PathBuf>, Vec<HeadingInfo>),
    outline_collapsed: HashMap<Option<PathBuf>, HashSet<String>>,
    search: SearchUi,
}

impl Default for Sidebar {
    fn default() -> Self {
        Self {
            root: None,
            filter: Filter::default(),
            filter_key: String::new(),
            watcher: None,
            index: None,
            articles: Articles::default(),
            listings: HashMap::new(),
            expanded: HashSet::new(),
            selected: None,
            renaming: None,
            reveal: None,
            tree_id: egui::Id::new("inkra-file-tree"),
            last_active: None,
            outline_filter: String::new(),
            outline_cache: (u64::MAX, None, Vec::new()),
            outline_collapsed: HashMap::new(),
            search: SearchUi::default(),
        }
    }
}

/// "5 min ago" style time (falls back to the UTC date).
pub fn relative_time(i18n: &I18n, t: Option<SystemTime>) -> String {
    let Some(t) = t else { return String::new() };
    let secs = SystemTime::now()
        .duration_since(t)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let n = |v: u64| -> inkra_i18n::Value<'static> { (v as i64).into() };
    match secs {
        0..60 => i18n.tr("time-just-now"),
        60..3600 => i18n.tr_args("time-minutes", &[("n", n(secs / 60))]),
        3600..86_400 => i18n.tr_args("time-hours", &[("n", n(secs / 3600))]),
        86_400..604_800 => i18n.tr_args("time-days", &[("n", n(secs / 86_400))]),
        _ => inkra_workspace::time::Utc::from_system(t).date(),
    }
}

/// A disclosure triangle (the bundled fonts have no ▸/▾ glyphs).
fn triangle(painter: &egui::Painter, center: egui::Pos2, open: bool, color: Color32) {
    let r = 3.5;
    let pts = if open {
        vec![
            pos2(center.x - r, center.y - r * 0.6),
            pos2(center.x + r, center.y - r * 0.6),
            pos2(center.x, center.y + r * 0.7),
        ]
    } else {
        vec![
            pos2(center.x - r * 0.6, center.y - r),
            pos2(center.x + r * 0.7, center.y),
            pos2(center.x - r * 0.6, center.y + r),
        ]
    };
    painter.add(egui::Shape::convex_polygon(pts, color, egui::Stroke::NONE));
}

/// Gives a custom-painted row an accessible name (AccessKit, egui_kittest).
pub(crate) fn accessible(resp: &egui::Response, kind: egui::WidgetType, label: &str) {
    let label = label.to_owned();
    resp.widget_info(move || egui::WidgetInfo::labeled(kind, true, label.clone()));
}

fn highlighted(text: &str, indices: &[u32], base: TextFormat, hl: Color32) -> LayoutJob {
    let mut job = LayoutJob::default();
    let set: HashSet<u32> = indices.iter().copied().collect();
    for (i, c) in text.chars().enumerate() {
        let mut f = base.clone();
        if set.contains(&(i as u32)) {
            f.color = hl;
            f.underline = egui::Stroke::new(1.0, hl);
        }
        let mut buf = [0u8; 4];
        job.append(c.encode_utf8(&mut buf), 0.0, f);
    }
    job
}

fn ranges_job(
    text: &str,
    ranges: &[std::ops::Range<usize>],
    base: TextFormat,
    bg: Color32,
) -> LayoutJob {
    let mut job = LayoutJob::default();
    let mut at = 0;
    for r in ranges {
        if r.start < at
            || r.end > text.len()
            || !text.is_char_boundary(r.start)
            || !text.is_char_boundary(r.end)
        {
            continue;
        }
        job.append(&text[at..r.start], 0.0, base.clone());
        let mut f = base.clone();
        f.background = bg;
        job.append(&text[r.clone()], 0.0, f);
        at = r.end;
    }
    job.append(&text[at..], 0.0, base);
    job
}

impl Sidebar {
    /// The current root folder.
    pub fn root(&self) -> Option<&Path> {
        self.root.as_deref()
    }

    /// Rebuilds watcher, index and caches when the root or the filter changed.
    pub fn sync_root(&mut self, ctx: &egui::Context, engine: &Engine, force: bool) {
        let p = &engine.state.prefs;
        let root = engine.state.session.root.clone();
        let key = format!(
            "{}|{}|{}|{}",
            p.bool("sidebar.showHidden"),
            p.bool("sidebar.showAllFiles"),
            p.string("sidebar.fileFilter"),
            p.bool("search.gitignore")
        );
        if !force && root == self.root && key == self.filter_key {
            return;
        }
        self.filter_key = key;
        self.root = root.clone();
        self.listings.clear();
        self.watcher = None;
        self.index = None;
        self.articles = Articles::default();
        let Some(root) = root else { return };
        self.filter = Filter::new(
            &root,
            p.bool("sidebar.showHidden"),
            p.bool("sidebar.showAllFiles"),
            &p.string("sidebar.fileFilter"),
        );
        self.expanded.insert(root.clone());
        let c = ctx.clone();
        self.watcher = RootWatcher::new(&root, move || c.request_repaint());
        let c = ctx.clone();
        self.index = Some(FileIndex::spawn(
            &root,
            self.filter.clone(),
            p.bool("search.gitignore"),
            move || c.request_repaint(),
        ));
    }

    /// Applies watcher events.
    pub fn poll(&mut self, ctx: &egui::Context, engine: &Engine) {
        self.sync_root(ctx, engine, false);
        let changed = self
            .watcher
            .as_ref()
            .map(RootWatcher::changed)
            .unwrap_or_default();
        if !changed.is_empty() {
            for p in &changed {
                if let Some(parent) = p.parent() {
                    self.listings.remove(parent);
                }
                self.listings.remove(p);
            }
            if let Some(ix) = &self.index {
                ix.update(&changed);
            }
            self.articles.invalidate();
        }
        // FS-023: follow the active document.
        let active = engine.state.editor.path.clone();
        if active != self.last_active {
            self.last_active = active.clone();
            if let Some(p) = active
                && engine.state.prefs.bool("sidebar.revealCurrent")
            {
                self.reveal_path(&p);
            }
        }
    }

    /// Expands the tree down to `p` and selects it.
    pub fn reveal_path(&mut self, p: &Path) {
        let Some(root) = &self.root else { return };
        if !p.starts_with(root) {
            return;
        }
        let mut cur = p.parent();
        while let Some(d) = cur {
            self.expanded.insert(d.to_owned());
            if d == root {
                break;
            }
            cur = d.parent();
        }
        self.selected = Some(p.to_owned());
        self.reveal = Some(p.to_owned());
    }

    /// Starts inline renaming of `p`.
    pub fn start_rename(&mut self, p: &Path) {
        let name = p
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.renaming = Some((p.to_owned(), name, true));
        if let Some(parent) = p.parent() {
            self.listings.remove(parent);
        }
    }

    /// Re-reads everything (e.g. after `sidebar.refresh`).
    pub fn invalidate(&mut self) {
        self.listings.clear();
        self.articles.invalidate();
    }

    /// Focuses the global search field.
    pub fn focus_search(&mut self) {
        self.search.focus = true;
    }

    fn listing(&mut self, dir: &Path) -> Vec<Entry> {
        if !self.listings.contains_key(dir) {
            let l = tree::list_dir(dir, &self.filter);
            self.listings.insert(dir.to_owned(), l);
        }
        self.listings.get(dir).cloned().unwrap_or_default()
    }

    /// Draws the sidebar.
    /// Draws the sidebar. `viewport_top` is the document position at the top of
    /// the active editor's viewport (the outline highlights that section).
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        engine: &Engine,
        i18n: &I18n,
        style: &Style,
        viewport_top: usize,
    ) -> Vec<Action> {
        let mut out = Vec::new();
        let view = engine.state.view.sidebar_view;
        // View switcher at the bottom (FS-020 … FS-024).
        egui::Panel::bottom("inkra-sidebar-tabs")
            .frame(egui::Frame::new().inner_margin(egui::Margin::symmetric(6, 4)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    for (v, key, cmd) in [
                        (SidebarView::Outline, "sidebar-outline", "view.outline"),
                        (SidebarView::Articles, "sidebar-articles", "view.articles"),
                        (SidebarView::Files, "sidebar-files", "view.fileTree"),
                        (SidebarView::Search, "sidebar-search", "search.global"),
                    ] {
                        if ui.selectable_label(view == v, i18n.tr(key)).clicked() && view != v {
                            out.push(Action::Invoke(cmd.into(), Value::Null));
                        }
                    }
                });
            });
        egui::CentralPanel::default()
            .frame(egui::Frame::new().inner_margin(egui::Margin::symmetric(6, 6)))
            .show(ui, |ui| match view {
                SidebarView::Outline => {
                    self.outline(ui, engine, i18n, style, viewport_top, &mut out)
                }
                SidebarView::Articles => self.article_list(ui, engine, i18n, style, &mut out),
                SidebarView::Files => self.file_tree(ui, engine, i18n, style, &mut out),
                SidebarView::Search => self.search_panel(ui, engine, i18n, style, &mut out),
            });
        out
    }

    // ---- Outline (FS-020) ----

    fn outline(
        &mut self,
        ui: &mut egui::Ui,
        engine: &Engine,
        i18n: &I18n,
        style: &Style,
        viewport_top: usize,
        out: &mut Vec<Action>,
    ) {
        let e = &engine.state.editor;
        let key = (e.doc.version(), e.path.clone());
        if self.outline_cache.0 != key.0 || self.outline_cache.1 != key.1 {
            let blocks = e.doc.ast().to_blocks();
            self.outline_cache = (
                key.0,
                key.1.clone(),
                inkra_markdown::walk::headings(e.doc.text(), &blocks),
            );
        }
        let depth = engine
            .state
            .prefs
            .f64("sidebar.outlineDepth")
            .clamp(1.0, 6.0) as u8;
        let heads: Vec<HeadingInfo> = self
            .outline_cache
            .2
            .iter()
            .filter(|h| h.level <= depth)
            .cloned()
            .collect();
        ui.add(
            egui::TextEdit::singleline(&mut self.outline_filter)
                .hint_text(i18n.tr("outline-filter"))
                .desired_width(f32::INFINITY),
        );
        ui.add_space(4.0);
        if heads.is_empty() {
            ui.weak(i18n.tr("outline-empty"));
            return;
        }
        // The section at the top of the viewport is highlighted.
        let top = viewport_top;
        let current = heads.iter().rposition(|h| h.span.start <= top);
        let collapsed = self.outline_collapsed.entry(key.1.clone()).or_default();
        let min_level = heads.iter().map(|h| h.level).min().unwrap_or(1);
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if !self.outline_filter.trim().is_empty() {
                    let texts: Vec<&str> = heads.iter().map(|h| h.text.as_str()).collect();
                    let hits = inkra_workspace::fuzzy::Fuzzy::text().rank(
                        &self.outline_filter,
                        &texts,
                        500,
                    );
                    for hit in hits {
                        let h = &heads[hit.index];
                        let base = TextFormat {
                            font_id: FontId::proportional(13.0),
                            color: style.sidebar_fg,
                            ..Default::default()
                        };
                        let job = highlighted(&h.text, &hit.indices, base, style.accent);
                        if ui
                            .add(egui::Button::new(job).frame(false).truncate())
                            .clicked()
                        {
                            out.push(Action::JumpTo(h.span.start));
                        }
                    }
                    return;
                }
                let mut hidden_below: Option<u8> = None;
                for (k, h) in heads.iter().enumerate() {
                    if let Some(l) = hidden_below {
                        if h.level > l {
                            continue;
                        }
                        hidden_below = None;
                    }
                    let has_children = heads.get(k + 1).is_some_and(|n| n.level > h.level);
                    let is_collapsed = collapsed.contains(&h.anchor);
                    if is_collapsed {
                        hidden_below = Some(h.level);
                    }
                    let indent = (h.level - min_level) as f32 * 14.0;
                    let (rect, resp) =
                        ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click());
                    accessible(&resp, egui::WidgetType::Button, &h.text);
                    if Some(k) == current {
                        ui.painter().rect_filled(rect, 3.0, style.sidebar_active_bg);
                    } else if resp.hovered() {
                        ui.painter().rect_filled(
                            rect,
                            3.0,
                            style.sidebar_active_bg.gamma_multiply(0.5),
                        );
                    }
                    let tri = Rect::from_min_size(
                        pos2(rect.left() + indent, rect.top()),
                        vec2(14.0, ROW_H),
                    );
                    if has_children {
                        triangle(ui.painter(), tri.center(), !is_collapsed, style.fg_muted);
                    }
                    let weight = if h.level <= min_level { 14.0 } else { 13.0 };
                    let text_pos = pos2(tri.right() + 2.0, rect.center().y);
                    let galley = ui.painter().layout(
                        h.text.clone(),
                        FontId::proportional(weight),
                        style.sidebar_fg,
                        rect.right() - text_pos.x,
                    );
                    ui.painter().galley(
                        pos2(text_pos.x, rect.center().y - galley.size().y / 2.0),
                        galley,
                        style.sidebar_fg,
                    );
                    if resp.clicked() {
                        let on_tri = resp.interact_pointer_pos().is_some_and(|p| tri.contains(p));
                        if has_children && on_tri {
                            if !collapsed.remove(&h.anchor) {
                                collapsed.insert(h.anchor.clone());
                            }
                        } else {
                            out.push(Action::JumpTo(h.span.start));
                        }
                    }
                    resp.context_menu(|ui| {
                        if ui.button(i18n.tr("outline-copy-anchor")).clicked() {
                            out.push(Action::Invoke(
                                "app.setClipboard".into(),
                                json!({ "text": format!("#{}", h.anchor) }),
                            ));
                            ui.close();
                        }
                    });
                }
            });
    }

    // ---- Article list (FS-021) ----

    fn article_list(
        &mut self,
        ui: &mut egui::Ui,
        engine: &Engine,
        i18n: &I18n,
        style: &Style,
        out: &mut Vec<Action>,
    ) {
        let Some(root) = self.root.clone() else {
            self.no_root(ui, i18n, out);
            return;
        };
        let p = &engine.state.prefs;
        let c = ui.ctx().clone();
        self.articles.refresh(
            &root,
            &self.filter,
            p.bool("sidebar.articlesRecursive"),
            move || c.request_repaint(),
        );
        let sort = p.string("sidebar.articlesSort");
        let desc = p.bool("sidebar.articlesSortDesc");
        ui.horizontal(|ui| {
            for (v, key) in [
                ("name", "articles-sort-name"),
                ("modified", "articles-sort-modified"),
                ("created", "articles-sort-created"),
            ] {
                if ui.selectable_label(sort == v, i18n.tr(key)).clicked() {
                    out.push(Action::Invoke(
                        "app.setPreference".into(),
                        json!({ "key": "sidebar.articlesSort", "value": v }),
                    ));
                }
            }
            let arrow = if desc { "↓" } else { "↑" };
            if ui.small_button(arrow).clicked() {
                out.push(Action::Invoke(
                    "app.setPreference".into(),
                    json!({ "key": "sidebar.articlesSortDesc", "value": !desc }),
                ));
            }
        });
        ui.separator();
        let mut list = self.articles.with_list(<[_]>::to_vec);
        list.sort_by(|a, b| {
            let o = match sort.as_str() {
                "modified" => a.modified.cmp(&b.modified),
                "created" => a.created.cmp(&b.created),
                _ => natural_cmp(&a.name, &b.name),
            };
            if desc { o.reverse() } else { o }
        });
        if list.is_empty() {
            ui.weak(i18n.tr("articles-empty"));
            return;
        }
        let current = engine.state.editor.path.clone();
        let row_h = 58.0;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show_rows(ui, row_h, list.len(), |ui, range| {
                for a in &list[range] {
                    let (rect, resp) =
                        ui.allocate_exact_size(vec2(ui.available_width(), row_h), Sense::click());
                    accessible(&resp, egui::WidgetType::Button, &a.name);
                    let active = current.as_deref() == Some(a.path.as_path());
                    if active {
                        ui.painter().rect_filled(rect, 4.0, style.sidebar_active_bg);
                    } else if resp.hovered() {
                        ui.painter().rect_filled(
                            rect,
                            4.0,
                            style.sidebar_active_bg.gamma_multiply(0.5),
                        );
                    }
                    let pad = 6.0;
                    let w = rect.width() - 2.0 * pad;
                    let name = ui.painter().layout_no_wrap(
                        a.name.clone(),
                        FontId::proportional(13.5),
                        style.sidebar_fg,
                    );
                    let time = relative_time(i18n, a.modified);
                    let tg = ui.painter().layout_no_wrap(
                        time,
                        FontId::proportional(11.0),
                        style.fg_muted,
                    );
                    ui.painter().galley(
                        pos2(rect.right() - pad - tg.size().x, rect.top() + 5.0),
                        tg.clone(),
                        style.fg_muted,
                    );
                    let clip = Rect::from_min_max(
                        rect.min,
                        pos2(rect.right() - pad - tg.size().x - 6.0, rect.max.y),
                    );
                    ui.painter().with_clip_rect(clip).galley(
                        pos2(rect.left() + pad, rect.top() + 4.0),
                        name,
                        style.sidebar_fg,
                    );
                    let mut job = LayoutJob::simple(
                        a.summary.clone(),
                        FontId::proportional(11.5),
                        style.fg_muted,
                        w,
                    );
                    job.wrap.max_rows = 2;
                    job.wrap.break_anywhere = false;
                    let sg = ui.painter().layout_job(job);
                    ui.painter().galley(
                        pos2(rect.left() + pad, rect.top() + 24.0),
                        sg,
                        style.fg_muted,
                    );
                    if resp.clicked() {
                        out.push(Action::Invoke(
                            "file.open".into(),
                            json!({ "path": a.path }),
                        ));
                    }
                    let path = a.path.clone();
                    resp.context_menu(|ui| self.entry_menu(ui, &path, false, i18n, out));
                }
            });
    }

    fn no_root(&mut self, ui: &mut egui::Ui, i18n: &I18n, out: &mut Vec<Action>) {
        ui.add_space(12.0);
        ui.vertical_centered(|ui| {
            ui.weak(i18n.tr("files-no-root"));
            ui.add_space(6.0);
            if ui.button(i18n.tr("files-open-folder")).clicked() {
                out.push(Action::PickRoot);
            }
        });
    }

    // ---- File tree (FS-022, FS-023) ----

    fn file_tree(
        &mut self,
        ui: &mut egui::Ui,
        engine: &Engine,
        i18n: &I18n,
        style: &Style,
        out: &mut Vec<Action>,
    ) {
        let Some(root) = self.root.clone() else {
            self.no_root(ui, i18n, out);
            return;
        };
        // Root header: name, recent folders, open folder.
        let root_name = root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| root.display().to_string());
        let header = ui.menu_button(
            egui::RichText::new(format!("{root_name}    ")).strong(),
            |ui| {
                ui.weak(i18n.tr("files-recent-folders"));
                for f in engine.state.session.recent_folders.iter().take(10) {
                    let label = f
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    if ui
                        .button(label)
                        .on_hover_text(f.display().to_string())
                        .clicked()
                    {
                        out.push(Action::Invoke(
                            "sidebar.setRoot".into(),
                            json!({ "path": f }),
                        ));
                        ui.close();
                    }
                }
                ui.separator();
                if ui.button(i18n.tr("files-open-folder")).clicked() {
                    out.push(Action::PickRoot);
                    ui.close();
                }
            },
        );
        let hr = header.response.rect;
        triangle(
            ui.painter(),
            pos2(hr.right() - 9.0, hr.center().y),
            true,
            style.fg_muted,
        );
        // Dropping on the header moves to the root folder.
        if let Some(src) = header.response.dnd_release_payload::<PathBuf>() {
            out.push(Action::Invoke(
                "sidebar.move".into(),
                json!({ "path": *src, "dest": root }),
            ));
        }
        ui.separator();
        let current = engine.state.editor.path.clone();
        let tree_rect = ui.available_rect_before_wrap();
        let tree_resp = ui.interact(tree_rect, self.tree_id, Sense::focusable_noninteractive());
        let focused = tree_resp.has_focus() || ui.ctx().memory(|m| m.has_focus(self.tree_id));
        let mut rows: Vec<(PathBuf, bool)> = Vec::new();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                self.dir_rows(
                    ui,
                    &root,
                    0,
                    current.as_deref(),
                    style,
                    i18n,
                    out,
                    &mut rows,
                );
            });
        // Context menu on empty space: new file/folder in the root.
        tree_resp.context_menu(|ui| self.entry_menu(ui, &root, true, i18n, out));
        if focused {
            // Arrow keys navigate the tree instead of moving egui's focus.
            let id = self.tree_id;
            let filter = egui::EventFilter {
                tab: false,
                horizontal_arrows: true,
                vertical_arrows: true,
                escape: false,
            };
            ui.ctx().memory_mut(|m| m.set_focus_lock_filter(id, filter));
        }
        if focused && self.renaming.is_none() {
            self.tree_keys(ui.ctx(), &rows, out);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn dir_rows(
        &mut self,
        ui: &mut egui::Ui,
        dir: &Path,
        depth: usize,
        current: Option<&Path>,
        style: &Style,
        i18n: &I18n,
        out: &mut Vec<Action>,
        rows: &mut Vec<(PathBuf, bool)>,
    ) {
        for e in self.listing(dir) {
            rows.push((e.path.clone(), e.is_dir));
            let (rect, resp) =
                ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click_and_drag());
            accessible(&resp, egui::WidgetType::Button, &e.name);
            let selected = self.selected.as_deref() == Some(e.path.as_path());
            let is_current = current == Some(e.path.as_path());
            let drop_hover = e.is_dir && resp.dnd_hover_payload::<PathBuf>().is_some();
            if selected || is_current || drop_hover {
                ui.painter().rect_filled(rect, 3.0, style.sidebar_active_bg);
            } else if resp.hovered() {
                ui.painter()
                    .rect_filled(rect, 3.0, style.sidebar_active_bg.gamma_multiply(0.5));
            }
            let indent = 6.0 + depth as f32 * 14.0;
            let open = self.expanded.contains(&e.path);
            if e.is_dir {
                triangle(
                    ui.painter(),
                    pos2(rect.left() + indent + 5.0, rect.center().y),
                    open,
                    style.fg_muted,
                );
            }
            let text_x = rect.left() + indent + 14.0;
            let renaming = self.renaming.as_ref().is_some_and(|r| r.0 == e.path);
            if renaming {
                let edit_rect = Rect::from_min_max(
                    pos2(text_x, rect.top() + 1.0),
                    pos2(rect.right() - 2.0, rect.bottom() - 1.0),
                );
                let (_, name, first) = self.renaming.as_mut().unwrap_or_else(|| unreachable!());
                let r = ui.put(
                    edit_rect,
                    egui::TextEdit::singleline(name).margin(egui::Margin::symmetric(2, 0)),
                );
                if *first {
                    r.request_focus();
                    // Select the stem so typing replaces the name, not the extension.
                    let stem = Path::new(name.as_str())
                        .file_stem()
                        .map(|s| s.to_string_lossy().chars().count())
                        .unwrap_or(0);
                    if let Some(mut st) = egui::TextEdit::load_state(ui.ctx(), r.id) {
                        let cr = egui::text::CCursorRange::two(
                            egui::text::CCursor::new(0),
                            egui::text::CCursor::new(stem),
                        );
                        st.cursor.set_char_range(Some(cr));
                        st.store(ui.ctx(), r.id);
                    }
                    *first = false;
                }
                let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                let esc = ui.input(|i| i.key_pressed(egui::Key::Escape));
                if esc {
                    self.renaming = None;
                } else if enter || r.lost_focus() {
                    let new = name.trim().to_owned();
                    if !new.is_empty()
                        && Some(new.as_str()) != e.path.file_name().and_then(|n| n.to_str())
                    {
                        out.push(Action::Invoke(
                            "sidebar.rename".into(),
                            json!({ "path": e.path, "name": new }),
                        ));
                    }
                    self.renaming = None;
                }
            } else {
                let color = if is_current {
                    style.accent
                } else {
                    style.sidebar_fg
                };
                let g =
                    ui.painter()
                        .layout_no_wrap(e.name.clone(), FontId::proportional(13.0), color);
                ui.painter().with_clip_rect(rect).galley(
                    pos2(text_x, rect.center().y - g.size().y / 2.0),
                    g,
                    color,
                );
            }
            if self.reveal.as_deref() == Some(e.path.as_path()) {
                resp.scroll_to_me(Some(egui::Align::Center));
                self.reveal = None;
            }
            if resp.drag_started() {
                resp.dnd_set_drag_payload(e.path.clone());
            }
            if e.is_dir
                && let Some(src) = resp.dnd_release_payload::<PathBuf>()
                && *src != e.path
            {
                out.push(Action::Invoke(
                    "sidebar.move".into(),
                    json!({ "path": *src, "dest": e.path }),
                ));
            }
            if resp.clicked() {
                ui.ctx().memory_mut(|m| m.request_focus(self.tree_id));
                self.selected = Some(e.path.clone());
                if e.is_dir {
                    if !self.expanded.remove(&e.path) {
                        self.expanded.insert(e.path.clone());
                    }
                } else {
                    out.push(Action::Invoke(
                        "file.open".into(),
                        json!({ "path": e.path }),
                    ));
                }
            }
            if resp.secondary_clicked() {
                self.selected = Some(e.path.clone());
            }
            let path = e.path.clone();
            let is_dir = e.is_dir;
            resp.context_menu(|ui| self.entry_menu(ui, &path, is_dir, i18n, out));
            if e.is_dir && self.expanded.contains(&e.path) {
                self.dir_rows(ui, &e.path, depth + 1, current, style, i18n, out, rows);
            }
        }
    }

    /// Keyboard navigation in the focused tree: arrows, Enter, F2, Delete,
    /// first-letter jump.
    fn tree_keys(&mut self, ctx: &egui::Context, rows: &[(PathBuf, bool)], out: &mut Vec<Action>) {
        let cur = self
            .selected
            .as_ref()
            .and_then(|s| rows.iter().position(|r| &r.0 == s));
        let events = ctx.input(|i| i.events.clone());
        for ev in events {
            match ev {
                egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } if modifiers.is_none() => {
                    let sel = |s: &mut Self, k: usize| {
                        if let Some(r) = rows.get(k) {
                            s.selected = Some(r.0.clone());
                            s.reveal = Some(r.0.clone());
                        }
                    };
                    match key {
                        egui::Key::ArrowDown => sel(
                            self,
                            cur.map_or(0, |c| (c + 1).min(rows.len().saturating_sub(1))),
                        ),
                        egui::Key::ArrowUp => sel(self, cur.map_or(0, |c| c.saturating_sub(1))),
                        egui::Key::ArrowRight => {
                            if let Some(c) = cur
                                && rows[c].1
                            {
                                self.expanded.insert(rows[c].0.clone());
                            }
                        }
                        egui::Key::ArrowLeft => {
                            if let Some(c) = cur {
                                let p = rows[c].0.clone();
                                if rows[c].1 && self.expanded.remove(&p) {
                                } else if let Some(parent) =
                                    p.parent().filter(|d| Some(*d) != self.root.as_deref())
                                {
                                    self.selected = Some(parent.to_owned());
                                }
                            }
                        }
                        egui::Key::Enter => {
                            if let Some(c) = cur {
                                if rows[c].1 {
                                    let p = rows[c].0.clone();
                                    if !self.expanded.remove(&p) {
                                        self.expanded.insert(p);
                                    }
                                } else {
                                    out.push(Action::Invoke(
                                        "file.open".into(),
                                        json!({ "path": rows[c].0 }),
                                    ));
                                }
                            }
                        }
                        egui::Key::F2 => {
                            if let Some(c) = cur {
                                let p = rows[c].0.clone();
                                self.start_rename(&p);
                            }
                        }
                        egui::Key::Delete => {
                            if let Some(c) = cur {
                                out.push(Action::Invoke(
                                    "sidebar.delete".into(),
                                    json!({ "path": rows[c].0 }),
                                ));
                            }
                        }
                        _ => {}
                    }
                }
                egui::Event::Text(t) => {
                    // First-letter jump (after the current row, wrapping).
                    let Some(ch) = t.chars().next().map(|c| c.to_lowercase().to_string()) else {
                        continue;
                    };
                    let start = cur.map_or(0, |c| c + 1);
                    let n = rows.len();
                    if let Some(k) = (0..n).map(|d| (start + d) % n).find(|&k| {
                        rows[k]
                            .0
                            .file_name()
                            .map(|f| f.to_string_lossy().to_lowercase().starts_with(&ch))
                            .unwrap_or(false)
                    }) {
                        self.selected = Some(rows[k].0.clone());
                        self.reveal = Some(rows[k].0.clone());
                    }
                }
                _ => {}
            }
        }
    }

    fn entry_menu(
        &mut self,
        ui: &mut egui::Ui,
        path: &Path,
        is_dir: bool,
        i18n: &I18n,
        out: &mut Vec<Action>,
    ) {
        let dir = if is_dir {
            path.to_owned()
        } else {
            path.parent().map(Path::to_owned).unwrap_or_default()
        };
        let mut item = |ui: &mut egui::Ui, key: &str, a: Action| {
            if ui.button(i18n.tr(key)).clicked() {
                out.push(a);
                ui.close();
            }
        };
        item(
            ui,
            "cmd-sidebar-newFile",
            Action::Invoke("sidebar.newFile".into(), json!({ "dir": dir })),
        );
        item(
            ui,
            "cmd-sidebar-newFolder",
            Action::Invoke("sidebar.newFolder".into(), json!({ "dir": dir })),
        );
        ui.separator();
        let is_root = Some(path) == self.root.as_deref();
        if !is_root {
            if !is_dir {
                item(
                    ui,
                    "cmd-sidebar-openInNewWindow",
                    Action::Invoke("sidebar.openInNewWindow".into(), json!({ "path": path })),
                );
            }
            if ui.button(i18n.tr("cmd-sidebar-rename")).clicked() {
                self.start_rename(path);
                ui.close();
            }
            item(
                ui,
                "cmd-sidebar-move",
                Action::PickMoveTarget(path.to_owned()),
            );
            item(
                ui,
                "cmd-sidebar-delete",
                Action::Invoke("sidebar.delete".into(), json!({ "path": path })),
            );
            ui.separator();
        }
        item(
            ui,
            "cmd-sidebar-reveal",
            Action::Invoke("sidebar.reveal".into(), json!({ "path": path })),
        );
        item(
            ui,
            "cmd-sidebar-copyPath",
            Action::Invoke("sidebar.copyPath".into(), json!({ "path": path })),
        );
        item(
            ui,
            "files-copy-relative",
            Action::Invoke(
                "sidebar.copyPath".into(),
                json!({ "path": path, "relative": true }),
            ),
        );
        ui.separator();
        if is_dir && !is_root {
            item(
                ui,
                "cmd-sidebar-setRoot",
                Action::Invoke("sidebar.setRoot".into(), json!({ "path": path })),
            );
        }
        item(
            ui,
            "cmd-sidebar-refresh",
            Action::Invoke("sidebar.refresh".into(), Value::Null),
        );
        if ui.button(i18n.tr("files-collapse-all")).clicked() {
            self.expanded
                .retain(|p| Some(p.as_path()) == self.root.as_deref());
            ui.close();
        }
    }

    // ---- Global search (FS-031) ----

    fn search_panel(
        &mut self,
        ui: &mut egui::Ui,
        engine: &Engine,
        i18n: &I18n,
        style: &Style,
        out: &mut Vec<Action>,
    ) {
        let Some(root) = self.root.clone() else {
            self.no_root(ui, i18n, out);
            return;
        };
        let now = ui.input(|i| i.time);
        let s = &mut self.search;
        let r = ui.add(
            egui::TextEdit::singleline(&mut s.query.pattern)
                .hint_text(i18n.tr("search-placeholder"))
                .desired_width(f32::INFINITY),
        );
        if s.focus {
            r.request_focus();
            s.focus = false;
        }
        if r.changed() {
            s.dirty = true;
            s.last_edit = now;
        }
        let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        ui.horizontal(|ui| {
            let toggle = |ui: &mut egui::Ui, v: &mut bool, label: &str, tip: &str| {
                if ui
                    .selectable_label(*v, label)
                    .on_hover_text(i18n.tr(tip))
                    .clicked()
                {
                    *v = !*v;
                    true
                } else {
                    false
                }
            };
            let mut c = false;
            c |= toggle(ui, &mut s.query.case_sensitive, "Aa", "search-case");
            c |= toggle(ui, &mut s.query.whole_word, "W", "search-word");
            c |= toggle(ui, &mut s.query.regex, ".*", "search-regex");
            if c {
                s.dirty = true;
                s.last_edit = 0.0;
            }
        });
        let g = ui.add(
            egui::TextEdit::singleline(&mut s.query.globs)
                .hint_text(i18n.tr("search-globs"))
                .desired_width(f32::INFINITY),
        );
        if g.changed() {
            s.dirty = true;
            s.last_edit = now;
        }
        // Debounced: search 300 ms after the last change, or on Enter.
        if (s.dirty && now - s.last_edit > 0.3) || enter {
            s.dirty = false;
            s.results.clear();
            s.status = None;
            s.running = None;
            if !s.query.pattern.is_empty() {
                s.query.gitignore = engine.state.prefs.bool("search.gitignore");
                let c = ui.ctx().clone();
                s.running = Some(search::start(
                    &root,
                    s.query.clone(),
                    self.filter.clone(),
                    move || c.request_repaint(),
                ));
            }
        } else if s.dirty {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(320));
        }
        if let Some(run) = &s.running {
            for ev in run.poll() {
                match ev {
                    SearchEvent::File(f) => s.results.push(f),
                    other => s.status = Some(other),
                }
            }
            if s.status.is_some() {
                s.running = None;
            }
        }
        // Status line.
        let hits: usize = s.results.iter().map(|f| f.lines.len()).sum();
        match &s.status {
            None if s.running.is_some() => {
                ui.weak(i18n.tr("search-searching"));
            }
            Some(SearchEvent::Error(e)) => {
                ui.colored_label(style.spell_error, e);
            }
            Some(SearchEvent::Done { truncated, .. }) => {
                if hits == 0 {
                    ui.weak(i18n.tr("search-none"));
                } else {
                    ui.weak(i18n.tr_args(
                        "search-results",
                        &[
                            ("hits", (hits as i64).into()),
                            ("files", (s.results.len() as i64).into()),
                        ],
                    ));
                }
                if *truncated {
                    ui.weak(i18n.tr_args(
                        "search-truncated",
                        &[("max", (search::MAX_HITS as i64).into())],
                    ));
                }
            }
            _ => {}
        }
        ui.separator();
        // Flat row list: file headers and lines.
        let mut rows: Vec<(usize, Option<usize>)> = Vec::new();
        for (fi, f) in s.results.iter().enumerate() {
            rows.push((fi, None));
            if !s.collapsed.contains(&f.path) {
                rows.extend((0..f.lines.len()).map(|li| (fi, Some(li))));
            }
        }
        let query = s.query.clone();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show_rows(ui, ROW_H, rows.len(), |ui, range| {
                for &(fi, li) in &rows[range] {
                    let f = &s.results[fi];
                    let (rect, resp) =
                        ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click());
                    if resp.hovered() {
                        ui.painter().rect_filled(
                            rect,
                            3.0,
                            style.sidebar_active_bg.gamma_multiply(0.5),
                        );
                    }
                    match li {
                        None => {
                            let rel = f
                                .path
                                .strip_prefix(&root)
                                .unwrap_or(&f.path)
                                .to_string_lossy()
                                .replace('\\', "/");
                            let open = !s.collapsed.contains(&f.path);
                            triangle(
                                ui.painter(),
                                pos2(rect.left() + 6.0, rect.center().y),
                                open,
                                style.fg_muted,
                            );
                            let text = format!("{rel}  ({})", f.lines.len());
                            let g = ui.painter().layout_no_wrap(
                                text,
                                FontId::proportional(13.0),
                                style.sidebar_fg,
                            );
                            ui.painter().with_clip_rect(rect).galley(
                                pos2(rect.left() + 14.0, rect.center().y - g.size().y / 2.0),
                                g,
                                style.sidebar_fg,
                            );
                            if resp.clicked() && !s.collapsed.remove(&f.path) {
                                s.collapsed.insert(f.path.clone());
                            }
                        }
                        Some(li) => {
                            let l = &f.lines[li];
                            // Trim leading whitespace and keep the first match visible.
                            let lead = l.text.len() - l.text.trim_start().len();
                            let first = l.ranges.first().map_or(0, |r| r.start);
                            // A sidebar is narrow: show only a few characters before
                            // the first match so the match itself stays visible.
                            let context = l.text[lead..first.max(lead)].chars().count();
                            let (cut, ellipsis) = if context > 12 {
                                let skip = context - 10;
                                let c = l.text[lead..]
                                    .char_indices()
                                    .nth(skip)
                                    .map_or(lead, |(i, _)| lead + i);
                                (c, "…")
                            } else {
                                (lead, "")
                            };
                            let shown = format!("{ellipsis}{}", &l.text[cut..]);
                            let shift = ellipsis.len();
                            let ranges: Vec<std::ops::Range<usize>> = l
                                .ranges
                                .iter()
                                .filter(|r| r.start >= cut)
                                .map(|r| r.start - cut + shift..r.end - cut + shift)
                                .collect();
                            let base = TextFormat {
                                font_id: FontId::proportional(12.5),
                                color: style.sidebar_fg,
                                ..Default::default()
                            };
                            let mut job = ranges_job(&shown, &ranges, base, style.search_match_bg);
                            job.wrap.max_rows = 1;
                            job.wrap.max_width = rect.width() - 40.0;
                            let g = ui.painter().layout_job(job);
                            let n = ui.painter().layout_no_wrap(
                                l.line.to_string(),
                                FontId::monospace(11.0),
                                style.fg_muted,
                            );
                            ui.painter().galley(
                                pos2(rect.left() + 14.0, rect.center().y - n.size().y / 2.0),
                                n,
                                style.fg_muted,
                            );
                            ui.painter().with_clip_rect(rect).galley(
                                pos2(rect.left() + 44.0, rect.center().y - g.size().y / 2.0),
                                g,
                                style.sidebar_fg,
                            );
                            if resp.clicked() {
                                out.push(Action::OpenAt {
                                    path: f.path.clone(),
                                    line: l.line,
                                    query: query.clone(),
                                });
                            }
                        }
                    }
                }
            });
    }
}
