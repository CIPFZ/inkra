//! Application state that commands operate on, and the effects they request.

use crate::editor::Editor;
use crate::prefs::Preferences;

/// View flags of a window.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ViewState {
    /// Whether the sidebar is shown (`view.toggleSidebar`).
    pub sidebar_visible: bool,
    /// Performance overlay (`view.debugOverlay`, FS-040).
    pub debug_overlay: bool,
}

/// State shared by all commands.
#[derive(Debug, Clone, Default)]
pub struct AppState {
    /// User preferences.
    pub prefs: Preferences,
    /// View flags.
    pub view: ViewState,
    /// The document editor of the window.
    pub editor: Editor,
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
