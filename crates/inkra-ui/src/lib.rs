//! # inkra-ui
//!
//! **Layer L6** (see SPEC 2.2).
//!
//! egui front end: windows, sidebar, editing canvas, menus, dialogs, toolbar and
//! status bar. The UI renders engine state and turns user input into command
//! invocations; it never edits documents directly.

pub mod clipboard;
mod completion;
mod context_menu;
pub mod dialogs;
pub mod editor;
pub mod find_bar;
pub mod images;
mod jobs;
pub mod keys;
pub mod menubar;
mod modals;
#[cfg(target_os = "macos")]
pub mod native_menu;
pub mod perf;
pub mod quick_open;
pub mod sidebar;
pub mod status_bar;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use inkra_engine::keymap::Os;
use inkra_engine::prefs::LoadProblem;
use inkra_engine::{Effect, Engine, Notice};
use inkra_i18n::I18n;
use serde_json::{Value, json};

pub use menubar::Invocation;

/// The 256 px application icon (UI-050, UI-052).
pub const ICON_PNG: &[u8] = include_bytes!("../../../assets/brand/inkra-256.png");

/// Raw key signals from the platform event loop, for shortcuts that egui's
/// winit integration consumes without an event (e.g. `Ctrl+V` when the clipboard
/// holds only an image or only HTML).
#[derive(Debug, Default)]
pub struct RawKeys {
    /// `Ctrl+V` (or `Cmd+V`) was pressed.
    pub paste: std::sync::atomic::AtomicBool,
    /// It was `Ctrl+Shift+V`.
    pub paste_plain: std::sync::atomic::AtomicBool,
}

/// How long a toast stays visible (UI-041).
const TOAST_SECS: f64 = 3.0;
/// Recovery data is written this often (FS-004).
const RECOVERY_SECS: f64 = 5.0;
/// Autosave debounce (FS-004).
const AUTOSAVE_SECS: f64 = 3.0;

struct Toast {
    text: String,
    until: f64,
}

/// UI state of one window.
struct WinUi {
    editor: editor::EditorView,
    find_focus: bool,
    status: status_bar::StatusCache,
    title: String,
    last_version: u64,
    last_edit: f64,
    was_focused: bool,
    /// Outer rectangle `[x, y, w, h]` (secondary windows; remembered, M4).
    rect: Option<[f32; 4]>,
    maximized: bool,
    /// Where the window opens (fixed, so the viewport builder stays stable).
    initial: Option<([f32; 4], bool)>,
}

impl WinUi {
    fn new() -> Self {
        Self {
            editor: editor::EditorView::new(),
            find_focus: false,
            status: Default::default(),
            title: String::new(),
            last_version: 0,
            last_edit: 0.0,
            was_focused: true,
            rect: None,
            maximized: false,
            initial: None,
        }
    }
}

/// A pending close / quit confirmation (FS-005).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Confirm {
    id: u64,
    quit: bool,
}

/// An update check: whether the user asked, and the newer release if any.
type UpdateResult = (bool, Result<Option<(String, String)>, String>);

/// The application shell (all windows).
pub struct Shell {
    /// The engine.
    pub engine: Engine,
    /// Localizer.
    pub i18n: I18n,
    os: Os,
    about_open: bool,
    prefs_problem: Option<String>,
    toasts: Vec<Toast>,
    icon: Option<egui::TextureHandle>,
    quit_requested: bool,
    clipboard: clipboard::Clipboard,
    setup_done: bool,
    perf: perf::Perf,
    /// Raw key signals (set by the platform event loop).
    pub raw_keys: std::sync::Arc<RawKeys>,
    dialogs: dialogs::Dialogs,
    sidebar: sidebar::Sidebar,
    quick_open: quick_open::QuickOpen,
    table_dialog: Option<(u32, u32)>,
    table_dialog_resize: bool,
    windows: BTreeMap<u64, WinUi>,
    root: u64,
    confirm: Option<Confirm>,
    quitting: bool,
    recovery_offer: Vec<inkra_workspace::recovery::RecoveryMeta>,
    watcher: Option<inkra_workspace::watch::Watcher>,
    last_recovery: f64,
    /// The root window should close (exit) at the end of this frame.
    close_root: bool,
    focus_request: Option<u64>,
    applied_style: u64,
    theme_watcher: Option<inkra_workspace::watch::Watcher>,
    last_time: f64,
    theme_names: Vec<String>,
    mmdc_conf: String,
    theme_names_at: f64,
    jobs: jobs::Jobs,
    updates: Option<std::sync::mpsc::Receiver<UpdateResult>>,
    update_available: Option<(String, String)>,
    page_setup: Option<modals::PageSetupForm>,
    import_offer: Option<PathBuf>,
    #[cfg(target_os = "macos")]
    native_menu: Option<native_menu::NativeMenu>,
}

impl Shell {
    /// Creates the shell. `prefs_problem` is shown once in a dialog (PR-003).
    pub fn new(engine: Engine, i18n: I18n, prefs_problem: Option<LoadProblem>) -> Self {
        let prefs_problem = prefs_problem.map(|p| match p {
            LoadProblem::Corrupt { backup, .. } => i18n.tr_args(
                "notice-prefs-corrupt",
                &[("path", backup.display().to_string().into())],
            ),
            LoadProblem::Unreadable { error } => error,
        });
        let mut engine = engine;
        engine.state.language = i18n.language().to_owned();
        let root = engine.state.active();
        let mut windows = BTreeMap::new();
        windows.insert(root, WinUi::new());
        Self {
            engine,
            i18n,
            os: Os::current(),
            about_open: false,
            prefs_problem,
            toasts: Vec::new(),
            icon: None,
            quit_requested: false,
            clipboard: clipboard::Clipboard::default(),
            setup_done: false,
            perf: perf::Perf::default(),
            raw_keys: Default::default(),
            dialogs: Default::default(),
            sidebar: Default::default(),
            quick_open: Default::default(),
            table_dialog: None,
            table_dialog_resize: false,
            windows,
            root,
            confirm: None,
            quitting: false,
            recovery_offer: Vec::new(),
            watcher: None,
            last_recovery: 0.0,
            close_root: false,
            focus_request: None,
            applied_style: 0,
            theme_watcher: None,
            last_time: 0.0,
            theme_names: Vec::new(),
            mmdc_conf: String::new(),
            jobs: Default::default(),
            updates: None,
            update_available: None,
            page_setup: None,
            import_offer: None,
            theme_names_at: f64::NEG_INFINITY,
            #[cfg(target_os = "macos")]
            native_menu: None,
        }
    }

    /// One-time setup of fonts, style and (on macOS) the native menu.
    pub fn setup(&mut self, ctx: &egui::Context) {
        // PR-020: only the real application (with configuration paths) checks.
        if self.engine.state.paths.is_some() && self.engine.state.prefs.bool("general.checkUpdates")
        {
            self.check_updates(ctx, false);
        }
        if self.setup_done {
            return;
        }
        self.setup_done = true;
        let t0 = std::time::Instant::now();
        ctx.set_fonts(inkra_layout::fonts::definitions());
        log::info!(
            "fonts installed in {:.1} ms",
            t0.elapsed().as_secs_f64() * 1e3
        );
        let c = ctx.clone();
        self.theme_watcher = Some(inkra_workspace::watch::Watcher::new(move || {
            c.request_repaint()
        }));
        self.engine.state.system_dark = ctx.system_theme() == Some(egui::Theme::Dark);
        let _ = self.engine.state.reload_theme();
        self.sync_style(ctx);
        // The spell checker loads in the background too (ED-104).
        let c = ctx.clone();
        // PR-035: a configured personal dictionary, else the default one.
        let configured = self
            .engine
            .state
            .prefs
            .string("editor.spellcheckDictionary");
        let personal = if configured.trim().is_empty() {
            self.engine
                .state
                .paths
                .as_ref()
                .map(inkra_workspace::Paths::dictionary_file)
        } else {
            Some(PathBuf::from(configured.trim()))
        };
        inkra_text::spell::load(personal, move || c.request_repaint());
        let c = ctx.clone();
        inkra_diagram::external::on_ready(move || c.request_repaint());
        // Syntax definitions load in the background (AD-5).
        let c = ctx.clone();
        inkra_highlight::on_ready(move || c.request_repaint());
        inkra_highlight::preload();
        let c = ctx.clone();
        self.watcher = Some(inkra_workspace::watch::Watcher::new(move || {
            c.request_repaint()
        }));
        if let Some(p) = &self.engine.state.paths {
            self.recovery_offer = inkra_workspace::recovery::Recovery::new(p.recovery_dir()).list();
        }
        self.os = keys::os_of(ctx.os());
        // Ctrl+= / Ctrl+- are heading shortcuts; editor zoom is ours (FS-040).
        ctx.options_mut(|o| o.zoom_with_keyboard = false);
        #[cfg(target_os = "macos")]
        {
            self.native_menu = Some(native_menu::NativeMenu::install(
                &mut self.engine,
                &self.i18n,
            ));
        }
    }

    /// Records the CPU time of the previous frame (from the platform integration).
    pub fn record_frame_cpu(&mut self, ms: f32) {
        self.perf.record(ms);
    }

    /// Whether `app.quit` completed.
    pub fn quit_requested(&self) -> bool {
        self.quit_requested
    }

    /// Whether the About dialog is open.
    pub fn about_open(&self) -> bool {
        self.about_open
    }

    /// Caret rectangle (screen) of the active window's editor (tests, IME).
    pub fn caret_screen(&self) -> Option<egui::Rect> {
        self.windows
            .get(&self.engine.state.active())
            .and_then(|w| w.editor.caret_screen)
    }

    /// Shows an "operation failed" notice after a caught panic.
    pub fn notify_failure(&mut self, what: &str) {
        let now = self.last_time;
        let text = self.i18n.tr_args(
            "notice-operation-failed",
            &[("command", what.to_owned().into())],
        );
        self.toasts.push(Toast {
            text,
            until: now + TOAST_SECS,
        });
    }

    /// The active window's editor view (tests).
    pub fn editor_view(&self) -> Option<&editor::EditorView> {
        self.windows
            .get(&self.engine.state.active())
            .map(|w| &w.editor)
    }

    /// Whether the quick-open file index finished building (tests).
    pub fn index_ready(&self) -> bool {
        self.sidebar
            .index
            .as_ref()
            .is_some_and(inkra_workspace::index::FileIndex::ready)
    }

    /// Duration of the last quick-open ranking in ms (tests, FS-030 budget).
    pub fn quick_open_ms(&self) -> f32 {
        self.quick_open.last_rank_ms
    }

    /// Whether a close confirmation is pending (tests).
    pub fn confirm_pending(&self) -> bool {
        self.confirm.is_some()
    }

    /// Number of windows.
    pub fn window_count(&self) -> usize {
        self.windows.len()
    }

    /// Restores windows of the last session (FS-006); the first goes into the
    /// root window. Returns how many files were opened.
    pub fn restore_session(&mut self, session: &inkra_workspace::session::Session) -> usize {
        let mut n = 0;
        for w in &session.windows {
            let Some(path) = &w.path else { continue };
            if !path.exists() {
                continue;
            }
            if self
                .engine
                .execute("file.open", &json!({ "path": path }))
                .is_err()
            {
                continue;
            }
            let effects = self.engine.take_effects();
            let mut target = self.engine.state.active();
            for e in effects {
                if let Effect::NewWindow(id) = e {
                    self.windows.insert(id, WinUi::new());
                    target = id;
                }
            }
            self.engine.state.activate(target);
            let ed = &mut self.engine.state.editor;
            let len = ed.doc.text().len();
            ed.sel = inkra_doc::Selection::caret(ed.doc.text().snap(w.caret.min(len)));
            if w.source_mode {
                ed.mode = inkra_engine::editor::ViewMode::Source;
            }
            self.engine.state.view.sidebar_visible = w.sidebar;
            if let Some(ui) = self.windows.get_mut(&target) {
                ui.editor.reveal_caret();
                if target != self.root
                    && self
                        .engine
                        .state
                        .prefs
                        .bool("appearance.rememberWindowSize")
                {
                    ui.initial = w.rect.map(|r| (r, w.maximized));
                }
            }
            n += 1;
        }
        self.engine.state.activate(self.root);
        n
    }

    /// Applies effects produced before the first frame (opening files at startup).
    pub fn apply_startup_effects(&mut self, effects: Vec<Effect>) {
        for e in effects {
            match e {
                Effect::NewWindow(id) => {
                    self.windows.insert(id, WinUi::new());
                }
                Effect::FocusWindow(id) => self.focus_request = Some(id),
                _ => {}
            }
        }
    }

    /// The current session (FS-006).
    pub fn session(&self) -> inkra_workspace::session::Session {
        let mut s = self.engine.state.session.clone();
        s.windows = self
            .engine
            .state
            .editors()
            .map(|(meta, e)| inkra_workspace::session::WindowSession {
                path: e.path.clone(),
                caret: e.sel.head,
                scroll: self
                    .windows
                    .get(&meta.id)
                    .map_or(0.0, |w| w.editor.scroll()),
                source_mode: e.mode == inkra_engine::editor::ViewMode::Source,
                sidebar: false,
                rect: self.windows.get(&meta.id).and_then(|w| w.rect),
                maximized: self.windows.get(&meta.id).is_some_and(|w| w.maximized),
            })
            .collect();
        s
    }

    /// Runs a command in the active window and applies its effects.
    pub fn invoke(&mut self, ctx: &egui::Context, id: &str, params: &Value) {
        if let Err(e) = self.engine.execute(id, params) {
            log::debug!("{e}");
            if let inkra_engine::CommandError::Failed { msg, .. } = &e {
                self.toast(ctx, format!("✕ {msg}"));
            }
        }
        self.apply_effects(ctx, None);
    }

    fn toast(&mut self, ctx: &egui::Context, text: String) {
        let now = ctx.input(|i| i.time);
        self.toasts.push(Toast {
            text,
            until: now + TOAST_SECS,
        });
    }

    fn win(&mut self) -> &mut WinUi {
        let id = self.engine.state.active();
        self.windows.entry(id).or_insert_with(WinUi::new)
    }

    fn apply_effects(&mut self, ctx: &egui::Context, list: Option<Vec<Effect>>) {
        let effects = list.unwrap_or_else(|| self.engine.take_effects());
        for effect in effects {
            match effect {
                Effect::Quit => self.begin_quit(),
                Effect::ShowAbout => self.about_open = true,
                Effect::CheckUpdates { manual } => self.check_updates(ctx, manual),
                Effect::SetClipboard { text, html } => self.clipboard.set(&text, html.as_deref()),
                Effect::RequestPaste { plain } => {
                    // ED-016 rule 1: a bitmap is inserted as an image file.
                    let e = &self.engine.state.editor;
                    let in_code =
                        inkra_engine::editor::context::in_code_content(&e.doc, e.sel.head);
                    if !plain
                        && !in_code
                        && let Some(png) = self.clipboard.image_png()
                    {
                        self.invoke(
                            ctx,
                            "format.insertImageFile",
                            &json!({ "path": png, "pasted": true }),
                        );
                        continue;
                    }
                    let c = self.clipboard.read(None);
                    let params = json!({
                        "text": c.text,
                        "html": c.html,
                        "files": c.files.iter().map(|p| p.to_string_lossy().into_owned()).collect::<Vec<_>>(),
                        "fromInkra": c.from_inkra,
                    });
                    let id = if plain {
                        "edit.pastePlain"
                    } else {
                        "edit.paste"
                    };
                    if let Err(e) = self.engine.execute(id, &params) {
                        log::debug!("{e}");
                    }
                    self.win().editor.reveal_caret();
                    let more = self.engine.take_effects();
                    self.apply_effects(ctx, Some(more));
                }
                Effect::ScrollToCaret => self.win().editor.reveal_caret(),
                Effect::FocusFind => self.win().find_focus = true,
                Effect::PickImage => {
                    let dir = self
                        .engine
                        .state
                        .editor
                        .path
                        .as_ref()
                        .and_then(|p| p.parent().map(Path::to_owned));
                    self.dialogs
                        .open(ctx, dialogs::Purpose::InsertImage, dir, None);
                }
                Effect::ShowTableDialog => {
                    self.table_dialog = Some((3, 3));
                    self.table_dialog_resize = false;
                }
                Effect::ShowTableResize => {
                    let e = &self.engine.state.editor;
                    if let Some(t) = inkra_engine::editor::table::table_at(&e.doc, e.sel.head) {
                        self.table_dialog = Some((
                            (t.rows.len().saturating_sub(1)).max(1) as u32,
                            t.cols() as u32,
                        ));
                        self.table_dialog_resize = true;
                    }
                }
                Effect::OpenUrl(url) => self.open_url(ctx, &url),
                Effect::NewWindow(id) => {
                    let mut w = WinUi::new();
                    w.editor.set_style(self.engine.state.style.clone());
                    self.windows.insert(id, w);
                    self.focus_request = Some(id);
                }
                Effect::FocusWindow(id) => self.focus_request = Some(id),
                Effect::CloseWindow(id) => self.close_window(ctx, id),
                Effect::ConfirmClose { id, quit } => self.confirm = Some(Confirm { id, quit }),
                Effect::OpenDialog => {
                    let dir = self
                        .engine
                        .state
                        .editor
                        .path
                        .as_ref()
                        .and_then(|p| p.parent().map(Path::to_owned));
                    self.dialogs.open(ctx, dialogs::Purpose::Open, dir, None);
                }
                Effect::OpenFolderDialog => {
                    self.dialogs
                        .open(ctx, dialogs::Purpose::OpenFolder, None, None)
                }
                Effect::SaveAsDialog { close_after, quit } => {
                    let e = &self.engine.state.editor;
                    let dir = e.path.as_ref().and_then(|p| p.parent().map(Path::to_owned));
                    let name = e
                        .path
                        .as_ref()
                        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
                        .unwrap_or_else(|| {
                            inkra_workspace::files::suggested_name(
                                &e.doc,
                                &self.i18n.tr("untitled"),
                                "md",
                            )
                        });
                    let id = self.engine.state.active();
                    self.dialogs.open(
                        ctx,
                        dialogs::Purpose::SaveAs {
                            close_after,
                            quit,
                            window: id,
                        },
                        dir,
                        Some(name),
                    );
                }
                Effect::Reveal(path) => reveal(&path),
                Effect::ShowQuickOpen => self.quick_open.open(),
                Effect::FocusGlobalSearch => self.sidebar.focus_search(),
                Effect::RevealInTree(p) => self.sidebar.reveal_path(&p),
                Effect::RenameInTree(p) => self.sidebar.start_rename(&p),
                Effect::RootChanged => {
                    self.sidebar.sync_root(ctx, &self.engine, true);
                    self.sidebar.invalidate();
                }
                Effect::ExportDialog { format, name, dir } => {
                    let window = self.engine.state.active();
                    self.dialogs.open(
                        ctx,
                        dialogs::Purpose::Export { format, window },
                        dir,
                        Some(name),
                    );
                }
                Effect::RunJob(job) => {
                    let name = job
                        .out
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    if !matches!(job.kind, inkra_engine::exporting::JobKind::Print(_)) {
                        self.toast(
                            ctx,
                            self.i18n
                                .tr_args("export-running", &[("name", name.into())]),
                        );
                    }
                    self.jobs.spawn(ctx, *job);
                }
                Effect::SetClipboardImage(png) => self.clipboard.set_png(&png),
                Effect::ShowPageSetup => {
                    self.page_setup =
                        Some(modals::PageSetupForm::from_prefs(&self.engine.state.prefs));
                }
                Effect::ImportDialog => {
                    let dir = self
                        .engine
                        .state
                        .editor
                        .path
                        .as_ref()
                        .and_then(|p| p.parent().map(Path::to_owned));
                    self.dialogs.open(ctx, dialogs::Purpose::Import, dir, None);
                }
                Effect::WindowFlags => {
                    let v = &self.engine.state.view;
                    let vid = if self.engine.state.active() == self.root {
                        egui::ViewportId::ROOT
                    } else {
                        egui::ViewportId::from_hash_of(("inkra-window", self.engine.state.active()))
                    };
                    ctx.send_viewport_cmd_to(vid, egui::ViewportCommand::Fullscreen(v.fullscreen));
                    let level = if v.always_on_top {
                        egui::WindowLevel::AlwaysOnTop
                    } else {
                        egui::WindowLevel::Normal
                    };
                    ctx.send_viewport_cmd_to(vid, egui::ViewportCommand::WindowLevel(level));
                }
                Effect::Error(msg) => self.toast(ctx, format!("✕ {msg}")),
                Effect::Notify(Notice::OperationFailed { command }) => {
                    let text = self
                        .i18n
                        .tr_args("notice-operation-failed", &[("command", command.into())]);
                    self.toast(ctx, text);
                }
            }
        }
    }

    /// Closes window `id` without asking (FS-005).
    fn close_window(&mut self, ctx: &egui::Context, id: u64) {
        if let Some(p) = &self.engine.state.paths {
            for (meta, _) in self.engine.state.editors() {
                if meta.id == id {
                    inkra_workspace::recovery::Recovery::new(p.recovery_dir())
                        .remove(&meta.recovery_id);
                }
            }
        }
        if self.windows.len() <= 1 {
            // Last window: the application exits (Windows/Linux, FS-005).
            self.remember_session();
            self.quit_requested = true;
            self.close_root = true;
            return;
        }
        if id == self.root {
            // The root OS window stays; it now shows another document.
            self.engine.state.remove_window(id);
            self.windows.remove(&id);
            if let Some(&next) = self.windows.keys().next() {
                self.root = next;
                self.engine.state.activate(next);
                ctx.send_viewport_cmd(egui::ViewportCommand::Title(String::new()));
            }
        } else {
            self.engine.state.remove_window(id);
            self.windows.remove(&id);
        }
        if self.quitting {
            self.continue_quit();
        }
    }

    fn remember_session(&mut self) {
        let s = self.session();
        self.engine.state.session = s;
    }

    /// Starts quitting: asks about every modified window in turn (FS-005).
    fn begin_quit(&mut self) {
        self.quitting = true;
        self.remember_session();
        self.continue_quit();
    }

    fn continue_quit(&mut self) {
        let dirty = self
            .engine
            .state
            .editors()
            .find(|(_, e)| e.doc.is_dirty())
            .map(|(m, _)| m.id);
        match dirty {
            Some(id) => self.confirm = Some(Confirm { id, quit: true }),
            None => {
                self.quit_requested = true;
                self.close_root = true;
            }
        }
    }

    /// `Ctrl+V` seen by the platform loop: read every clipboard format ourselves and
    /// drop egui's text-only paste event (ED-016 rule order).
    fn handle_raw_paste(&mut self, ctx: &egui::Context) {
        use std::sync::atomic::Ordering;
        if !self.raw_keys.paste.swap(false, Ordering::SeqCst) {
            return;
        }
        let plain = self.raw_keys.paste_plain.swap(false, Ordering::SeqCst);
        ctx.input_mut(|i| i.events.retain(|e| !matches!(e, egui::Event::Paste(_))));
        let editor_id = self.win().editor.id();
        let focus = ctx.memory(|m| m.focused());
        if focus.is_some() && focus != Some(editor_id) {
            return;
        }
        self.apply_effects(ctx, Some(vec![Effect::RequestPaste { plain }]));
    }

    /// Files dropped on the window (FS-002, ED-017): Markdown files open, images
    /// are inserted, anything else becomes a link.
    fn handle_drops(&mut self, ctx: &egui::Context) {
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        for f in dropped {
            let path = f.path().to_path_buf();
            if path.as_os_str().is_empty() {
                continue;
            }
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .map(str::to_ascii_lowercase)
                .unwrap_or_default();
            if inkra_workspace::tree::markdown_exts().contains(&ext) {
                self.invoke(ctx, "file.open", &json!({ "path": path }));
            } else if matches!(
                ext.as_str(),
                "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "svg" | "tif" | "tiff"
            ) {
                self.invoke(ctx, "format.insertImageFile", &json!({ "path": path }));
            } else if inkra_export::pandoc::IMPORT_EXTS.contains(&ext.as_str()) {
                // EX-021: dropped documents are imported after asking.
                self.import_offer = Some(path);
            } else {
                self.invoke(ctx, "edit.paste", &json!({ "files": [path] }));
            }
        }
    }

    /// Follows a link: `#anchor` jumps to the heading (MD-033), Markdown files
    /// open in Inkra, other targets open with the system handler.
    fn open_url(&mut self, ctx: &egui::Context, url: &str) {
        if let Some(anchor) = url.strip_prefix('#') {
            let e = &mut self.engine.state.editor;
            let blocks = e.doc.ast().to_blocks();
            let headings = inkra_markdown::walk::headings(e.doc.text(), &blocks);
            if let Some(h) = headings.iter().find(|h| h.anchor == anchor) {
                e.sel = inkra_doc::Selection::caret(h.span.end);
                self.win().editor.reveal_caret();
            }
            return;
        }
        let is_web = url.contains("://") || url.starts_with("mailto:");
        if !is_web {
            let (file, _anchor) = url.split_once('#').unwrap_or((url, ""));
            let base = self
                .engine
                .state
                .editor
                .path
                .as_ref()
                .and_then(|p| p.parent().map(Path::to_owned));
            if let Some(path) = images::resolve(base.as_deref(), file) {
                let is_md = path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
                    inkra_workspace::tree::markdown_exts().contains(&e.to_ascii_lowercase())
                });
                if is_md && path.exists() {
                    self.invoke(ctx, "file.open", &json!({ "path": path }));
                    return;
                }
                if path.exists() {
                    dialogs::open_external(&path.to_string_lossy());
                    return;
                }
            }
        }
        if !dialogs::open_external(url) {
            self.toast(ctx, format!("✕ {url}"));
        }
    }

    fn handle_dialogs(&mut self, ctx: &egui::Context) {
        for (purpose, path) in self.dialogs.poll() {
            match (purpose, path) {
                (dialogs::Purpose::InsertImage, Some(path)) => {
                    self.invoke(ctx, "format.insertImageFile", &json!({ "path": path }));
                }
                (dialogs::Purpose::Open, Some(path)) => {
                    self.invoke(ctx, "file.open", &json!({ "path": path }))
                }
                (dialogs::Purpose::OpenFolder, Some(path)) => {
                    self.invoke(ctx, "sidebar.setRoot", &json!({ "path": path }));
                    let v = &mut self.engine.state.view;
                    v.sidebar_visible = true;
                    v.sidebar_view = inkra_engine::state::SidebarView::Files;
                }
                (dialogs::Purpose::MoveTo(src), Some(dest)) => {
                    self.invoke(ctx, "sidebar.move", &json!({ "path": src, "dest": dest }));
                }
                (
                    dialogs::Purpose::SaveAs {
                        close_after,
                        quit,
                        window,
                    },
                    Some(path),
                ) => {
                    self.engine.state.activate(window);
                    let path = with_default_extension(path);
                    self.invoke(ctx, "file.saveAs", &json!({ "path": path }));
                    if close_after && !self.engine.state.editor.doc.is_dirty() {
                        self.close_window(ctx, window);
                    }
                    if quit && !self.engine.state.editor.doc.is_dirty() {
                        self.continue_quit();
                    }
                }
                (dialogs::Purpose::SaveAs { quit: true, .. }, None) => self.quitting = false,
                (dialogs::Purpose::Export { format, window }, Some(path)) => {
                    self.engine.state.activate(window);
                    let mut p = format.params();
                    p["path"] = json!(path);
                    self.invoke(ctx, format.command(), &p);
                }
                (dialogs::Purpose::Import, Some(path)) => {
                    self.invoke(ctx, "file.import", &json!({ "path": path }));
                }
                _ => {}
            }
        }
    }

    /// Dispatches key presses bound in the keymap and removes them from the input.
    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        let os = self.os;
        let mut hits = Vec::new();
        let in_table = {
            let e = &self.engine.state.editor;
            e.mode == inkra_engine::editor::ViewMode::Live
                && inkra_engine::editor::table::in_table(&e.doc, e.sel.head)
        };
        let keymap = &self.engine.keymap;
        let registry = &self.engine.registry;
        ctx.input_mut(|i| {
            i.events.retain(|ev| {
                let egui::Event::Key {
                    key,
                    physical_key,
                    pressed: true,
                    modifiers,
                    ..
                } = ev
                else {
                    return true;
                };
                let Some(chord) = keys::chord(*key, *physical_key, *modifiers, os) else {
                    return true;
                };
                match keymap.resolve(&chord, in_table) {
                    Some(b) if registry.contains(&b.command) => {
                        hits.push((b.command.clone(), b.params.clone()));
                        false
                    }
                    _ => true,
                }
            });
        });
        for (id, params) in hits {
            self.invoke(ctx, &id, &params);
        }
    }

    /// Re-reads the installed theme list (at most every few seconds; it is shown
    /// in the Themes menu).
    fn refresh_theme_names(&mut self, now: f64) {
        if now - self.theme_names_at < 3.0 && !self.theme_names.is_empty() {
            return;
        }
        self.theme_names_at = now;
        let dir = self.engine.state.paths.as_ref().map(|p| p.themes_dir());
        self.theme_names = inkra_engine::theme::list(dir.as_deref())
            .into_iter()
            .map(|e| e.name)
            .collect();
    }

    fn menu_data(&self) -> menubar::MenuData {
        let recent = self
            .engine
            .state
            .session
            .recent
            .iter()
            .take(20)
            .map(|p| {
                (
                    p.file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                    p.to_string_lossy().into_owned(),
                )
            })
            .collect();
        let active = self.engine.state.active();
        let windows = self
            .engine
            .state
            .editors()
            .map(|(m, e)| (self.title_of(e), m.id, m.id == active))
            .collect();
        let current = &self.engine.state.style.name;
        let themes = self
            .theme_names
            .iter()
            .map(|n| (n.clone(), n == current))
            .collect();
        let custom_exports = inkra_engine::exporting::custom_exports(&self.engine.state)
            .into_iter()
            .map(|c| c.label)
            .collect();
        menubar::MenuData {
            recent,
            windows,
            themes,
            custom_exports,
        }
    }

    /// Window title (FS-008): `● name - folder`.
    fn title_of(&self, e: &inkra_engine::editor::Editor) -> String {
        let dirty = if e.doc.is_dirty() { "● " } else { "" };
        match &e.path {
            Some(p) => {
                let name = p
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let folder = p
                    .parent()
                    .and_then(|d| d.file_name())
                    .map(|n| n.to_string_lossy().into_owned());
                match folder {
                    Some(f) => format!("{dirty}{name} - {f}"),
                    None => format!("{dirty}{name}"),
                }
            }
            None => format!("{dirty}{}", self.i18n.tr("untitled")),
        }
    }

    /// Pushes the engine's theme to egui and every editor when it changed.
    fn sync_style(&mut self, ctx: &egui::Context) {
        let style = &self.engine.state.style;
        if style.version == self.applied_style
            && self
                .windows
                .values()
                .all(|w| w.editor.base_version() == style.version)
        {
            return;
        }
        self.applied_style = style.version;
        apply_visuals(ctx, style);
        for w in self.windows.values_mut() {
            w.editor.set_style(style.clone());
        }
        // Hot reload of user themes (TH-003).
        let dir = self.engine.state.paths.as_ref().map(|p| p.themes_dir());
        let file = inkra_engine::theme::file_of(&style.name, dir.as_deref());
        log::debug!("theme {} active; watching {:?}", style.name, file);
        if let Some(w) = &mut self.theme_watcher {
            w.set_files(file.as_deref());
        }
        ctx.request_repaint();
    }

    /// Theme follow-ups: system appearance (Auto) and edited theme files.
    fn theme_tasks(&mut self, ctx: &egui::Context) {
        let dark = ctx.system_theme() == Some(egui::Theme::Dark);
        if dark != self.engine.state.system_dark {
            self.engine.state.system_dark = dark;
            if self.engine.state.prefs.bool("appearance.themeAuto") {
                let _ = self.engine.state.reload_theme();
            }
        }
        let changed = self
            .theme_watcher
            .as_mut()
            .map(|w| !w.changed().is_empty())
            .unwrap_or(false);
        if changed {
            log::debug!("theme file changed; reloading");
            if let Err(e) = self.engine.state.reload_theme() {
                self.toast(ctx, format!("✕ {e}"));
            }
        }
        self.sync_style(ctx);
    }

    /// Carries out what the sidebar asked for.
    fn sidebar_actions(&mut self, ctx: &egui::Context, actions: Vec<sidebar::Action>) {
        for a in actions {
            match a {
                sidebar::Action::Invoke(id, p) => self.invoke(ctx, &id, &p),
                sidebar::Action::JumpTo(pos) => {
                    // The heading goes to the top of the viewport; the caret to the
                    // end of its text (inside the heading, past the `#` markers).
                    let e = &mut self.engine.state.editor;
                    let t = e.doc.text();
                    let pos = t.snap(pos.min(t.len()));
                    let line = t.line_content_range(t.byte_to_line(pos));
                    e.set_selection(inkra_doc::Selection::caret(line.end));
                    self.win().editor.scroll_to_top(pos);
                }
                sidebar::Action::OpenAt { path, line, query } => {
                    self.open_search_hit(ctx, &path, line, &query)
                }
                sidebar::Action::PickMoveTarget(p) => {
                    let dir = p.parent().map(Path::to_owned);
                    self.dialogs
                        .open(ctx, dialogs::Purpose::MoveTo(p), dir, None);
                }
                sidebar::Action::PickRoot => {
                    self.dialogs
                        .open(ctx, dialogs::Purpose::OpenFolder, None, None)
                }
            }
        }
    }

    /// Opens a global search hit: the file, the find bar with the same query, and
    /// the first match on `line` selected (FS-031).
    fn open_search_hit(
        &mut self,
        ctx: &egui::Context,
        path: &Path,
        line: u64,
        q: &inkra_workspace::search::Query,
    ) {
        self.invoke(ctx, "file.open", &json!({ "path": path }));
        let canon = inkra_workspace::paths::canonical(path);
        let Some(id) = self
            .engine
            .state
            .editors()
            .find(|(_, e)| e.path.as_deref() == Some(canon.as_path()))
            .map(|(m, _)| m.id)
        else {
            return;
        };
        let back = self.engine.state.active();
        self.engine.state.activate(id);
        let e = &mut self.engine.state.editor;
        e.find.open = true;
        e.find.query = q.pattern.clone();
        e.find.options = inkra_engine::editor::find::FindOptions {
            case_sensitive: q.case_sensitive,
            whole_word: q.whole_word,
            regex: q.regex,
        };
        e.find.invalidate();
        let t = e.doc.text();
        let line_start =
            t.line_to_byte((line.saturating_sub(1) as usize).min(t.len_lines().saturating_sub(1)));
        e.find.refresh(&e.doc, line_start);
        let hit = e.find.matches.iter().position(|m| m.start >= line_start);
        if let Some(k) = hit {
            let m = e.find.matches[k].clone();
            e.find.current = Some(k);
            e.set_selection(inkra_doc::Selection::new(m.start, m.end));
        } else {
            e.set_selection(inkra_doc::Selection::caret(line_start));
        }
        if let Some(w) = self.windows.get_mut(&id) {
            w.editor.reveal_caret();
        }
        self.engine.state.activate(back);
        self.focus_request = Some(id);
    }

    /// Points the diagram engine at the configured `mmdc` (MD-100).
    fn sync_mermaid_cli(&mut self) {
        let want = self.engine.state.prefs.string("markdown.mermaidCli");
        if want == self.mmdc_conf {
            return;
        }
        self.mmdc_conf = want.clone();
        let cache = self
            .engine
            .state
            .paths
            .as_ref()
            .map_or_else(std::env::temp_dir, |p| p.data.join("diagrams"));
        inkra_diagram::external::configure(
            (!want.trim().is_empty()).then(|| PathBuf::from(want.trim())),
            cache,
        );
    }

    /// Frame-level work shared by all windows.
    /// Starts an update check in the background (PR-020).
    fn check_updates(&mut self, ctx: &egui::Context, manual: bool) {
        if self.updates.is_some() {
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let ctx = ctx.clone();
        let repo = env!("CARGO_PKG_REPOSITORY")
            .trim_start_matches("https://github.com/")
            .to_owned();
        let spawned = std::thread::Builder::new()
            .name("inkra-update".into())
            .spawn(move || {
                let r = inkra_workspace::net::newer_release(&repo, env!("CARGO_PKG_VERSION"));
                let _ = tx.send((manual, r));
                ctx.request_repaint();
            });
        if spawned.is_ok() {
            self.updates = Some(rx);
        }
    }

    fn poll_updates(&mut self, ctx: &egui::Context) {
        let Some(rx) = &self.updates else { return };
        let Ok((manual, r)) = rx.try_recv() else {
            return;
        };
        self.updates = None;
        match r {
            Ok(Some(found)) => self.update_available = Some(found),
            Ok(None) if manual => self.toast(ctx, self.i18n.tr("update-none")),
            Err(e) if manual => self.toast(
                ctx,
                self.i18n.tr_args("update-failed", &[("error", e.into())]),
            ),
            Err(e) => log::info!("update check: {e}"),
            Ok(None) => {}
        }
    }

    /// Reports finished export, print and import jobs.
    fn finish_jobs(&mut self, ctx: &egui::Context) {
        use inkra_engine::exporting::JobResult;
        for (job, result) in self.jobs.poll() {
            match result {
                Ok(JobResult::Exported(files)) => {
                    let name = files
                        .first()
                        .and_then(|f| f.file_name())
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    self.toast(
                        ctx,
                        self.i18n.tr_args("export-done", &[("name", name.into())]),
                    );
                    if job.open_after
                        && let Some(f) = files.first()
                    {
                        dialogs::open_external(&f.to_string_lossy());
                    }
                }
                Ok(JobResult::Print(pdf)) => {
                    if let Err(e) = jobs::print_pdf(&pdf) {
                        self.toast(ctx, format!("✕ {e}"));
                    }
                }
                Ok(JobResult::Imported(md)) => {
                    self.invoke(ctx, "file.open", &json!({ "path": md }))
                }
                Err(e) => self.toast(
                    ctx,
                    self.i18n.tr_args("export-failed", &[("error", e.into())]),
                ),
            }
        }
    }

    fn frame_tasks(&mut self, ctx: &egui::Context) {
        self.sync_mermaid_cli();
        self.poll_updates(ctx);
        self.finish_jobs(ctx);
        self.sidebar.poll(ctx, &self.engine);
        self.last_time = ctx.input(|i| i.time);
        self.refresh_theme_names(self.last_time);
        self.theme_tasks(ctx);
        let now = ctx.input(|i| i.time);
        // Recovery (FS-004).
        if now - self.last_recovery > RECOVERY_SECS {
            self.last_recovery = now;
            inkra_engine::files::write_recovery(&self.engine.state);
        }
        // Only while something is unsaved.
        if self.engine.state.editors().any(|(_, e)| e.doc.is_dirty()) {
            ctx.request_repaint_after(Duration::from_secs_f64(RECOVERY_SECS));
        }
        // External changes (FS-007).
        let mut changed_ids = Vec::new();
        if let Some(w) = &mut self.watcher {
            let files: Vec<PathBuf> = self
                .engine
                .state
                .editors()
                .filter_map(|(_, e)| e.path.clone())
                .collect();
            w.set_files(files.iter().map(PathBuf::as_path));
            let changed = w.changed();
            if !changed.is_empty() {
                for (meta, e) in self.engine.state.editors() {
                    if e.path.as_ref().is_some_and(|p| changed.contains(p)) {
                        changed_ids.push(meta.id);
                    }
                }
            }
        }
        for id in changed_ids {
            inkra_engine::files::external_change(&mut self.engine.state, id);
        }
        // Autosave (PR-012).
        if self.engine.state.prefs.bool("general.autoSave") {
            let ids: Vec<u64> = self.windows.keys().copied().collect();
            for id in ids {
                self.engine.state.activate(id);
                let version = self.engine.state.editor.doc.version();
                let w = self.windows.get_mut(&id).unwrap_or_else(|| unreachable!());
                if version != w.last_version {
                    w.last_version = version;
                    w.last_edit = now;
                }
                let due = now - w.last_edit > AUTOSAVE_SECS;
                let (dirty, has_path) = {
                    let e = &self.engine.state.editor;
                    (e.doc.is_dirty(), e.path.is_some())
                };
                if due && dirty && has_path {
                    let _ = self.engine.execute("file.save", &Value::Null);
                    self.engine.take_effects();
                }
                if !due && dirty {
                    ctx.request_repaint_after(Duration::from_secs_f64(AUTOSAVE_SECS));
                }
            }
        }
    }

    /// Draws the root window; other windows are drawn as immediate viewports.
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        if !self.setup_done {
            // Fonts installed now take effect on the next pass.
            self.setup(&ctx);
            ctx.request_repaint();
            return;
        }
        if std::env::var_os("INKRA_DEBUG_REPAINT").is_some() {
            log::info!(
                "frame t={:.2} causes={:?}",
                ctx.input(|i| i.time),
                ctx.repaint_causes()
            );
        }
        self.handle_dialogs(&ctx);
        self.frame_tasks(&ctx);

        // Secondary windows.
        let others: Vec<u64> = self
            .windows
            .keys()
            .copied()
            .filter(|&id| id != self.root)
            .collect();
        for id in others {
            self.engine.state.activate(id);
            let title = format!("{} - Inkra", self.title_of(&self.engine.state.editor));
            let mut builder = egui::ViewportBuilder::default()
                .with_title(title)
                .with_app_id("io.inkra.Inkra");
            // Window state memory: restored windows reopen where they were; new
            // ones cascade from the root window.
            let initial = match self.windows.get(&id).and_then(|w| w.initial) {
                Some(i) => i,
                None => {
                    let k = self.windows.len().saturating_sub(1) as f32;
                    let base = ctx
                        .input(|i| i.viewport().outer_rect)
                        .map_or(egui::pos2(80.0, 80.0), |r| r.min);
                    let p = base + egui::vec2(28.0, 28.0) * k;
                    let i = ([p.x, p.y, 1000.0, 720.0], false);
                    if let Some(w) = self.windows.get_mut(&id) {
                        w.initial = Some(i);
                    }
                    i
                }
            };
            let ([x, y, w, h], max) = initial;
            builder = builder
                .with_position([x, y])
                .with_inner_size([w, h])
                .with_maximized(max);
            if let Ok(icon) = eframe_icon() {
                builder = builder.with_icon(icon);
            }
            let vid = egui::ViewportId::from_hash_of(("inkra-window", id));
            if self.focus_request == Some(id) {
                ctx.send_viewport_cmd_to(vid, egui::ViewportCommand::Focus);
                self.focus_request = None;
            }
            ctx.show_viewport_immediate(vid, builder, |vui, _class| {
                self.engine.state.activate(id);
                let (outer, inner, max) = vui.ctx().input(|i| {
                    (
                        i.viewport().outer_rect,
                        i.viewport().inner_rect,
                        i.viewport().maximized,
                    )
                });
                if let Some(w) = self.windows.get_mut(&id) {
                    // Outer position, inner size: what the builder takes.
                    if let (Some(o), Some(n)) = (outer, inner) {
                        w.rect = Some([o.min.x, o.min.y, n.width(), n.height()]);
                    }
                    w.maximized = max.unwrap_or(false);
                }
                self.window_ui(vui, id);
            });
        }

        // Root window.
        let root = self.root;
        self.engine.state.activate(root);
        if self.focus_request == Some(root) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            self.focus_request = None;
        }
        let title = format!("{} - Inkra", self.title_of(&self.engine.state.editor));
        if self.windows.get(&root).is_some_and(|w| w.title != title) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            if let Some(w) = self.windows.get_mut(&root) {
                w.title = title;
            }
        }
        self.window_ui(ui, root);
        modals::recovery_offer(self, &ctx);
        if self.close_root {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    /// The UI of one window (`id` is active).
    fn window_ui(&mut self, ui: &mut egui::Ui, id: u64) {
        let ctx = ui.ctx().clone();
        // Close requests from the OS (FS-005).
        if ctx.input(|i| i.viewport().close_requested()) && !self.close_root {
            if self.engine.state.editor.doc.is_dirty() {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.confirm = Some(Confirm { id, quit: false });
            } else if self.windows.len() > 1 && id == self.root {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.close_window(&ctx, id);
                return;
            } else if id != self.root {
                self.close_window(&ctx, id);
                return;
            } else {
                self.remember_session();
                self.quit_requested = true;
            }
        }
        let focused = ctx.input(|i| i.viewport().focused.unwrap_or(true));
        if let Some(w) = self.windows.get_mut(&id) {
            // Autosave on focus loss (FS-004).
            if w.was_focused && !focused && self.engine.state.prefs.bool("general.autoSave") {
                let e = &self.engine.state.editor;
                let should = e.doc.is_dirty() && e.path.is_some();
                if should {
                    let _ = self.engine.execute("file.save", &Value::Null);
                    self.engine.take_effects();
                }
            }
            if let Some(w) = self.windows.get_mut(&id) {
                w.was_focused = focused;
            }
        }
        let had_input = ctx.input(|i| {
            i.events.iter().any(|e| {
                matches!(
                    e,
                    egui::Event::Text(_)
                        | egui::Event::Key { pressed: true, .. }
                        | egui::Event::Ime(_)
                        | egui::Event::Paste(_)
                )
            })
        });
        if id == self.root {
            self.perf.begin(had_input);
        }
        if focused {
            self.handle_raw_paste(&ctx);
        }
        self.handle_drops(&ctx);
        self.handle_shortcuts(&ctx);

        #[cfg(target_os = "macos")]
        if let Some(menu) = &self.native_menu {
            let invocations = menu.poll();
            for inv in invocations {
                self.invoke(&ctx, &inv.id, &inv.params);
            }
            if let Some(menu) = &self.native_menu {
                menu.refresh(&mut self.engine);
            }
        }

        // The menu bar can be hidden (TH-008); holding Alt shows it, as do
        // fullscreen-style windows via Alt (WM-003).
        let alt = ctx.input(|i| i.modifiers.alt && !i.modifiers.command && !i.modifiers.shift);
        let show_menu = (self.engine.state.view.menu_bar && !self.engine.state.view.fullscreen)
            || alt
            || egui::Popup::is_any_open(&ctx);
        if !self.uses_native_menu() && show_menu {
            let mut invocation = None;
            let data = self.menu_data();
            egui::Panel::top("inkra-menubar").show(ui, |ui| {
                invocation = menubar::show(ui, &mut self.engine, &self.i18n, self.os, &data);
            });
            if let Some(inv) = invocation {
                self.invoke(&ctx, &inv.id, &inv.params);
            }
        }

        if self.engine.state.view.sidebar_visible {
            let style = self.engine.state.style.clone();
            let mut actions = Vec::new();
            egui::Panel::left("inkra-sidebar")
                .resizable(true)
                .default_size(240.0)
                .size_range(180.0..=(ctx.content_rect().width() * 0.5).max(180.0))
                .frame(egui::Frame::new().fill(style.sidebar_bg))
                .show(ui, |ui| {
                    let top = self.windows.get(&id).map_or(0, |w| w.editor.viewport_top());
                    actions = self.sidebar.show(ui, &self.engine, &self.i18n, &style, top);
                });
            self.sidebar_actions(&ctx, actions);
        }

        let style_bg = self.win().editor.style.bg;
        let style = self.win().editor.style.clone();
        if self.engine.state.view.status_bar {
            egui::Panel::bottom("inkra-status")
                .frame(
                    egui::Frame::new()
                        .fill(style_bg)
                        .inner_margin(egui::Margin::symmetric(10, 3)),
                )
                .show(ui, |ui| {
                    let w = self.windows.entry(id).or_insert_with(WinUi::new);
                    if let Some(cmd) =
                        status_bar::show(ui, &mut w.status, &self.engine, &self.i18n, &style)
                    {
                        self.invoke(&ctx, &cmd, &Value::Null);
                    }
                });
        }

        // Filled: secondary viewports have no clear color behind the panels.
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(style_bg))
            .show(ui, |ui| {
                modals::external_banner(self, ui);
                if self.engine.state.editor.find.open {
                    let editor_id = self.win().editor.id();
                    let mut focus = self.win().find_focus;
                    let cmds =
                        find_bar::show(ui, &mut self.engine, &self.i18n, &mut focus, editor_id);
                    self.win().find_focus = focus;
                    for c in cmds {
                        self.invoke(&ctx, &c, &Value::Null);
                    }
                }
                let w = self.windows.entry(id).or_insert_with(WinUi::new);
                let out = w
                    .editor
                    .show(ui, &mut self.engine, &mut self.clipboard, &self.i18n);
                for (cmd, params) in out.commands {
                    self.invoke(&ctx, &cmd, &params);
                }
            });

        if self.engine.state.view.debug_overlay && id == self.root {
            let ms = self.win().editor.last_ms;
            self.perf.overlay(&ctx, ms);
        }
        if self.quick_open.is_open()
            && (id == self.root || ctx.input(|i| i.viewport().focused.unwrap_or(false)))
        {
            let style = self.engine.state.style.clone();
            let recent = self.engine.state.session.recent.clone();
            if let Some(c) = self.quick_open.show(
                &ctx,
                self.sidebar.index.as_ref(),
                &recent,
                &self.i18n,
                &style,
            ) {
                let id = if c.new_window {
                    "sidebar.openInNewWindow"
                } else {
                    "file.open"
                };
                self.invoke(&ctx, id, &json!({ "path": c.path }));
            }
        }
        modals::table_dialog(self, &ctx);
        modals::confirm_close(self, &ctx, id);
        modals::about_dialog(self, &ctx);
        modals::page_setup(self, &ctx);
        modals::update_banner(self, &ctx);
        modals::import_offer(self, &ctx);
        modals::prefs_problem_dialog(self, &ctx);
        modals::toasts(self, &ctx);
    }

    fn uses_native_menu(&self) -> bool {
        #[cfg(target_os = "macos")]
        {
            self.native_menu.is_some()
        }
        #[cfg(not(target_os = "macos"))]
        {
            false
        }
    }
}

fn inkra_engine_files_blank(app: &inkra_engine::AppState) -> bool {
    inkra_engine::files::active_is_blank(app)
}

fn eframe_icon() -> Result<egui::IconData, ()> {
    let img = image::load_from_memory(ICON_PNG)
        .map_err(|_| ())?
        .to_rgba8();
    Ok(egui::IconData {
        width: img.width(),
        height: img.height(),
        rgba: img.into_raw(),
    })
}

fn with_default_extension(path: PathBuf) -> PathBuf {
    if path.extension().is_some() {
        path
    } else {
        path.with_extension("md")
    }
}

/// Shows `path` in the system file manager (FS-010).
fn reveal(path: &Path) {
    let r = if cfg!(windows) {
        std::process::Command::new("explorer")
            .arg(format!("/select,{}", path.display()))
            .spawn()
    } else if cfg!(target_os = "macos") {
        std::process::Command::new("open")
            .arg("-R")
            .arg(path)
            .spawn()
    } else {
        std::process::Command::new("xdg-open")
            .arg(path.parent().unwrap_or(path))
            .spawn()
    };
    if let Err(e) = r {
        log::warn!("cannot reveal {}: {e}", path.display());
    }
}

/// Image source for `path` relative to the document (ED-092 "prefer relative paths").
pub fn relative_src(doc: Option<&Path>, path: &Path) -> String {
    let rel = doc
        .and_then(|d| d.parent())
        .and_then(|dir| path.strip_prefix(dir).ok())
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| path.to_path_buf());
    let s = rel.to_string_lossy().replace('\\', "/");
    if s.contains(' ') {
        s.replace(' ', "%20")
    } else {
        s
    }
}

/// Applies egui visuals matching the document style.
pub fn apply_visuals(ctx: &egui::Context, style: &inkra_layout::Style) {
    let mut v = if style.dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    v.panel_fill = style.bg;
    v.window_fill = style.bg;
    v.extreme_bg_color = if style.dark {
        style.bg.gamma_multiply(0.8)
    } else {
        egui::Color32::WHITE
    };
    v.faint_bg_color = style.sidebar_bg;
    v.code_bg_color = style.code_bg;
    v.selection.bg_fill = style.selection;
    v.selection.stroke.color = style.fg;
    v.hyperlink_color = style.accent;
    v.text_cursor.stroke.color = style.cursor;
    v.window_stroke.color = style.code_border;
    v.widgets.noninteractive.bg_stroke.color = style.hr;
    v.widgets.noninteractive.fg_stroke.color = style.fg;
    v.widgets.inactive.fg_stroke.color = style.fg;
    v.widgets.inactive.weak_bg_fill = style.code_bg;
    v.widgets.hovered.weak_bg_fill = style.sidebar_active_bg;
    v.widgets.active.weak_bg_fill = style.sidebar_active_bg;
    // The UI chrome follows the document theme (TH-001), not the OS preference.
    let theme = if style.dark {
        egui::Theme::Dark
    } else {
        egui::Theme::Light
    };
    ctx.set_theme(theme);
    ctx.set_visuals_of(theme, v);
}

#[cfg(test)]
mod tests;
