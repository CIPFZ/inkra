//! System clipboard access with HTML and file lists (ED-016) via `arboard`.
//!
//! The `arboard::Clipboard` is kept alive for the application's lifetime: on X11
//! the clipboard contents are served by the owning process.

use inkra_engine::editor::ClipboardContent;

/// Marker added to HTML Inkra puts on the clipboard, so pasting it back inserts
/// the Markdown source instead of converting the HTML.
const INKRA_MARK: &str = "<!-- inkra -->";

/// Clipboard handle.
#[derive(Default)]
pub struct Clipboard {
    inner: Option<arboard::Clipboard>,
    failed: bool,
}

impl Clipboard {
    fn get(&mut self) -> Option<&mut arboard::Clipboard> {
        if self.inner.is_none() && !self.failed {
            match arboard::Clipboard::new() {
                Ok(c) => self.inner = Some(c),
                Err(e) => {
                    log::warn!("clipboard unavailable: {e}");
                    self.failed = true;
                }
            }
        }
        self.inner.as_mut()
    }

    /// Writes plain text and optional HTML.
    pub fn set(&mut self, text: &str, html: Option<&str>) {
        let Some(c) = self.get() else { return };
        let result = match html {
            Some(h) => c.set_html(format!("{INKRA_MARK}{h}"), Some(text.to_owned())),
            None => c.set_text(text),
        };
        if let Err(e) = result {
            log::warn!("clipboard write failed: {e}");
        }
    }

    /// Reads all supported formats. `fallback_text` is the text egui already read
    /// (used if arboard cannot read the clipboard).
    pub fn read(&mut self, fallback_text: Option<String>) -> ClipboardContent {
        let Some(c) = self.get() else {
            return ClipboardContent {
                text: fallback_text,
                ..Default::default()
            };
        };
        let text = c.get_text().ok().or(fallback_text);
        let html = c.get().html().ok().filter(|h| !h.trim().is_empty());
        let files = c.get().file_list().unwrap_or_default();
        let from_inkra = html.as_deref().is_some_and(|h| h.contains(INKRA_MARK));
        ClipboardContent {
            text,
            html,
            files,
            from_inkra,
        }
    }
}
