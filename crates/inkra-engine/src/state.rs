//! Application state that commands operate on, and the effects they request.

use crate::editor::Editor;
use crate::prefs::Preferences;

/// View flags of a window.
#[derive(Debug, Clone, PartialEq)]
pub struct ViewState {
    /// Whether the sidebar is shown (`view.toggleSidebar`).
    pub sidebar_visible: bool,
    /// The sidebar view.
    pub sidebar_view: SidebarView,
    /// Performance overlay (`view.debugOverlay`, FS-040).
    pub debug_overlay: bool,
    /// Status bar shown (PR-043).
    pub status_bar: bool,
    /// Menu bar shown (TH-008, Windows/Linux).
    pub menu_bar: bool,
    /// Editor toolbar shown (FS-041).
    pub toolbar: bool,
    /// Spaces, tabs and line ends drawn visibly in source mode.
    pub show_whitespace: bool,
    /// Focus mode (WM-001).
    pub focus: bool,
    /// Typewriter mode (WM-002).
    pub typewriter: bool,
    /// Editor zoom factor (FS-040), 0.5 ..= 3.0.
    pub zoom: f32,
    /// Fullscreen (WM-003).
    pub fullscreen: bool,
    /// Always on top.
    pub always_on_top: bool,
}

impl Default for ViewState {
    fn default() -> Self {
        Self {
            sidebar_visible: false,
            sidebar_view: SidebarView::Outline,
            debug_overlay: false,
            status_bar: true,
            menu_bar: true,
            toolbar: false,
            show_whitespace: false,
            focus: false,
            typewriter: false,
            zoom: 1.0,
            fullscreen: false,
            always_on_top: false,
        }
    }
}

/// The sidebar's view (FS-020 … FS-024).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SidebarView {
    /// Document outline.
    #[default]
    Outline,
    /// Flat article list of the root folder.
    Articles,
    /// File tree of the root folder.
    Files,
    /// Global search.
    Search,
}

/// An external change to the open file (FS-007).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternalChange {
    /// Changed on disk while there are unsaved local edits: ask the user.
    Modified,
    /// Deleted or moved away.
    Deleted,
}

/// Per-window bookkeeping besides the editor.
#[derive(Debug, Clone, Default)]
pub struct WindowMeta {
    /// Window id (stable for the session).
    pub id: u64,
    /// What the file looked like when last loaded or saved.
    pub disk: Option<inkra_workspace::files::DiskState>,
    /// Pending external change notice.
    pub external: Option<ExternalChange>,
    /// Recovery entry id.
    pub recovery_id: String,
}

/// A window that is not currently active.
#[derive(Debug, Clone)]
pub struct Parked {
    /// Its editor.
    pub editor: Editor,
    /// Its view flags.
    pub view: ViewState,
    /// Its metadata.
    pub meta: WindowMeta,
}

/// State shared by all commands. The active window's state lives in `editor`,
/// `view` and `meta`; other windows are parked in `others` and swapped in by
/// [`AppState::activate`].
#[derive(Debug, Clone, Default)]
pub struct AppState {
    /// User preferences.
    pub prefs: Preferences,
    /// View flags of the active window.
    pub view: ViewState,
    /// The document editor of the active window.
    pub editor: Editor,
    /// Metadata of the active window.
    pub meta: WindowMeta,
    /// Other windows.
    pub others: Vec<Parked>,
    /// Session data: recent files and folders, sidebar root.
    pub session: inkra_workspace::session::Session,
    /// Platform directories.
    pub paths: Option<inkra_workspace::Paths>,
    /// Next window id.
    pub next_window: u64,
    /// The resolved theme.
    pub style: inkra_layout::Style,
    /// The UI language in use (`zh-CN`, `en-US`), set by the host.
    pub language: String,
    /// The system asks for reduced motion (UI-033): no animated scrolling or
    /// fading.
    pub reduce_motion: bool,
    /// The system uses a dark appearance (for the Auto theme).
    pub system_dark: bool,
}

impl AppState {
    /// Saves the preferences file (atomic, PR-001).
    pub fn save_prefs(&self) {
        if let Some(p) = &self.paths
            && let Err(e) = self.prefs.save(&p.preferences_file())
        {
            log::warn!("cannot save preferences: {e}");
        }
    }

    /// Applies every preference to the open windows (after import or reset).
    pub fn apply_prefs(&mut self) {
        let _ = self.reload_theme();
        let o = crate::markdown_options(&self.prefs);
        let ep = crate::edit_prefs(&self.prefs);
        self.editor.doc.set_options(o.clone());
        self.editor.prefs = ep.clone();
        for w in &mut self.others {
            w.editor.doc.set_options(o.clone());
            w.editor.prefs = ep.clone();
        }
        crate::viewing::view_from_prefs(&mut self.view, &self.prefs);
        inkra_workspace::tree::set_markdown_exts(&self.prefs.strings("general.markdownExtensions"));
    }

    /// Sets a preference and saves the file.
    pub fn set_pref(&mut self, key: &str, value: serde_json::Value) -> bool {
        let ok = self.prefs.set(key, value);
        if ok {
            self.save_prefs();
        }
        ok
    }

    /// The theme that should be active now (Auto picks by system brightness).
    pub fn wanted_theme(&self) -> String {
        if self.prefs.bool("appearance.themeAuto") {
            let key = if self.system_dark {
                "appearance.darkTheme"
            } else {
                "appearance.lightTheme"
            };
            self.prefs.string(key)
        } else {
            self.prefs.string("appearance.theme")
        }
    }

    /// (Re)loads the wanted theme into `style`; on failure keeps the current one
    /// and returns the error.
    pub fn reload_theme(&mut self) -> Result<(), String> {
        let name = self.wanted_theme();
        let dir = self.paths.as_ref().map(|p| p.themes_dir());
        match crate::theme::load(&name, dir.as_deref()) {
            Ok(mut s) => {
                // Rendering options that live in preferences, not themes.
                s.math_numbering = match self.prefs.string("markdown.mathNumbering").as_str() {
                    "ams" => 1,
                    "all" => 2,
                    _ => 0,
                };
                s.diagrams = self.prefs.bool("markdown.diagrams");
                s.toc_depth = self.prefs.f64("markdown.tocDepth").clamp(1.0, 6.0) as u8;
                s.spellcheck = self.prefs.bool("editor.spellcheck");
                let p = &self.prefs;
                s.code_wrap = p.bool("markdown.codeWrap");
                s.code_line_numbers = p.bool("markdown.codeLineNumbers");
                s.first_line_indent = p.bool("editor.firstLineIndent");
                s.preserve_blank_lines = p.bool("markdown.preserveBlankLines");
                s.image_titles = p.bool("image.showTitle");
                s.image_fit_column = p.bool("image.maxWidthFollowsEditor");
                let bits = [
                    s.code_wrap,
                    s.code_line_numbers,
                    s.first_line_indent,
                    s.preserve_blank_lines,
                    s.image_titles,
                    s.image_fit_column,
                ]
                .iter()
                .enumerate()
                .fold(0u64, |acc, (k, b)| acc | (u64::from(*b) << k));
                s.version = s.version.wrapping_add(bits << 32);
                s.version = s.version.wrapping_add(u64::from(s.spellcheck) << 24);
                s.version = s.version.wrapping_add(u64::from(s.toc_depth) << 16);
                s.version = s
                    .version
                    .wrapping_add(u64::from(s.math_numbering))
                    .wrapping_add(u64::from(s.diagrams) << 8);
                self.style = s;
                Ok(())
            }
            Err(e) => {
                log::warn!("theme {name}: {e}");
                Err(e)
            }
        }
    }

    /// Id of the active window.
    pub fn active(&self) -> u64 {
        self.meta.id
    }

    /// Ids of all windows (active first).
    pub fn window_ids(&self) -> Vec<u64> {
        std::iter::once(self.meta.id)
            .chain(self.others.iter().map(|p| p.meta.id))
            .collect()
    }

    /// Makes window `id` active; returns false if it does not exist.
    pub fn activate(&mut self, id: u64) -> bool {
        if self.meta.id == id {
            return true;
        }
        let Some(i) = self.others.iter().position(|p| p.meta.id == id) else {
            return false;
        };
        let parked = self.others.swap_remove(i);
        let old = Parked {
            editor: std::mem::replace(&mut self.editor, parked.editor),
            view: std::mem::replace(&mut self.view, parked.view),
            meta: std::mem::replace(&mut self.meta, parked.meta),
        };
        self.others.push(old);
        true
    }

    /// Creates a window (parked) and returns its id.
    pub fn new_window(&mut self, mut editor: Editor) -> u64 {
        editor.prefs = crate::edit_prefs(&self.prefs);
        if editor.doc.text().is_empty() {
            editor.doc.set_options(crate::markdown_options(&self.prefs));
            editor.doc.line_ending = crate::new_line_ending(&self.prefs);
        }
        self.next_window = self.next_window.max(self.meta.id) + 1;
        let id = self.next_window;
        let meta = WindowMeta {
            id,
            recovery_id: recovery_id(id),
            ..Default::default()
        };
        self.others.push(Parked {
            editor,
            view: self.view.clone(),
            meta,
        });
        id
    }

    /// Removes window `id`. The active window can only be removed if another
    /// exists (which becomes active).
    pub fn remove_window(&mut self, id: u64) -> bool {
        if self.meta.id == id {
            let Some(next) = self.others.pop() else {
                return false;
            };
            self.editor = next.editor;
            self.view = next.view;
            self.meta = next.meta;
            return true;
        }
        let before = self.others.len();
        self.others.retain(|p| p.meta.id != id);
        self.others.len() != before
    }

    /// Visits every window's editor (active first).
    pub fn editors(&self) -> impl Iterator<Item = (&WindowMeta, &Editor)> {
        std::iter::once((&self.meta, &self.editor))
            .chain(self.others.iter().map(|p| (&p.meta, &p.editor)))
    }
}

/// Recovery entry id for a window of this process.
pub fn recovery_id(window: u64) -> String {
    let t = inkra_workspace::time::Utc::now().compact_millis();
    format!("{t}-{}-{window}", std::process::id())
}

/// User-visible notices requested by the engine.
#[derive(Debug, Clone, PartialEq)]
pub enum Notice {
    /// A command failed unexpectedly (NF-002).
    OperationFailed {
        /// Command id.
        command: String,
    },
}

/// Side effects the engine cannot perform itself; drained by the host (UI or CLI).
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    /// Quit the application (closing all windows).
    Quit,
    /// Show the About dialog.
    ShowAbout,
    /// Show a notice.
    Notify(Notice),
    /// Put text on the clipboard (`text/plain` and optional `text/html`).
    SetClipboard {
        /// Plain text (Markdown source).
        text: String,
        /// Rendered HTML.
        html: Option<String>,
    },
    /// Read the clipboard and run `edit.paste` with its content.
    RequestPaste {
        /// Paste as plain text.
        plain: bool,
    },
    /// Scroll so the caret is visible.
    ScrollToCaret,
    /// Move keyboard focus to the find bar.
    FocusFind,
    /// Ask the user for an image file to insert (ED-040).
    PickImage,
    /// Show the insert-table dialog (ED-053).
    ShowTableDialog,
    /// Open a link target.
    OpenUrl(String),
    /// A window was created (parked in the state); show it.
    NewWindow(u64),
    /// Bring a window to the front.
    FocusWindow(u64),
    /// Close a window (no confirmation needed).
    CloseWindow(u64),
    /// Ask save / don't save / cancel before closing window `id` (FS-005);
    /// `quit` continues quitting afterwards.
    ConfirmClose {
        /// Window id.
        id: u64,
        /// Part of quitting the application.
        quit: bool,
    },
    /// Ask for a file to open.
    OpenDialog,
    /// Ask for a folder to open.
    OpenFolderDialog,
    /// Ask where to save (FS-003); optionally close the window afterwards.
    SaveAsDialog {
        /// Close after saving.
        close_after: bool,
        /// Then continue quitting.
        quit: bool,
    },
    /// Show a file in the system file manager.
    Reveal(std::path::PathBuf),
    /// Show an error message.
    Error(String),
    /// Apply window flags (fullscreen, always on top) of the active window.
    WindowFlags,
    /// Show the table resize dialog (ED-083).
    ShowTableResize,
    /// Show the quick open palette (FS-030).
    ShowQuickOpen,
    /// Focus the global search field (FS-031).
    FocusGlobalSearch,
    /// Expand the file tree to `path` and select it (FS-023).
    RevealInTree(std::path::PathBuf),
    /// Start renaming `path` in the file tree.
    RenameInTree(std::path::PathBuf),
    /// The sidebar root changed: rebuild the tree, index and article list.
    RootChanged,
    /// Ask where to export (EX-001 … EX-004), then run the format's command
    /// with the chosen `path`.
    ExportDialog {
        /// Format.
        format: crate::exporting::ExportFormat,
        /// Suggested file name.
        name: String,
        /// Starting folder.
        dir: Option<std::path::PathBuf>,
    },
    /// Put a PNG image on the clipboard.
    SetClipboardImage(Vec<u8>),
    /// Run an export, print or import job in the background.
    RunJob(Box<crate::exporting::ExportJob>),
    /// Show the page setup dialog (EX-002).
    ShowPageSetup,
    /// Show the Preferences window.
    ShowPreferences,
    /// Ask for a new name of the current file.
    RenameDialog,
    /// Ask for the folder to move `path` into.
    MoveDialog(std::path::PathBuf),
    /// Ask before moving `path` to the recycle bin.
    ConfirmDelete(std::path::PathBuf),
    /// Show the document properties (FS-009).
    ShowProperties,
    /// A document was saved (UI-041 toast).
    Saved(std::path::PathBuf),
    /// Put the image at this file path or URL on the clipboard as a bitmap.
    CopyImage(String),
    /// Open a file or URL with the system's default application.
    OpenExternal(String),
    /// Forget a loaded image so it is loaded again.
    ReloadImage(String),
    /// Download a remote image, then run `image.toLocal` with the file.
    DownloadImage(String),
    /// Edit a link's text and address (UI-040).
    LinkDialog {
        /// Link text.
        text: String,
        /// Address.
        url: String,
    },
    /// Edit an image's alt text, title and width.
    ImageDialog(crate::imaging::ImageRef),
    /// Ask for a settings file to import or a place to export to (PR-002).
    SettingsDialog {
        /// Import (else export).
        import: bool,
    },
    /// Look for a newer release (PR-020); `manual` reports "up to date" too.
    CheckUpdates {
        /// Asked for by the user.
        manual: bool,
    },
    /// Ask for a file to import (EX-020).
    ImportDialog,
}

/// Context passed to every command.
#[derive(Debug)]
pub struct CommandContext<'a> {
    /// Application state.
    pub app: &'a mut AppState,
    /// Effects to be performed by the host after the command returns.
    pub effects: &'a mut Vec<Effect>,
}

impl<'a> CommandContext<'a> {
    /// Creates a context.
    pub fn new(app: &'a mut AppState, effects: &'a mut Vec<Effect>) -> Self {
        Self { app, effects }
    }
}
