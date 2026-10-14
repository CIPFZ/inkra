//! Native file dialogs (rfd, UI-040). They run off the UI thread where the platform
//! allows it (macOS requires the main thread) and report back through a channel.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};

/// What a dialog was opened for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Purpose {
    /// Insert an image (ED-040).
    InsertImage,
    /// Open a Markdown file (FS-002).
    Open,
    /// Open a folder (FS-002).
    OpenFolder,
    /// Choose the folder to move a file into (FS-022).
    MoveTo(PathBuf),
    /// Where to export (EX-001 … EX-004).
    Export {
        /// Format.
        format: inkra_engine::exporting::ExportFormat,
        /// Window being exported.
        window: u64,
    },
    /// A document to import (EX-020).
    Import,
    /// The new file of a broken image (ED-090).
    RelinkImage(String),
    /// A settings file to import (PR-002).
    ImportSettings,
    /// Where to export the settings (PR-002).
    ExportSettings,
    /// Save as (FS-003); then close the window if `close_after`.
    SaveAs {
        /// Close after saving.
        close_after: bool,
        /// Then continue quitting.
        quit: bool,
        /// Window being saved.
        window: u64,
    },
}

/// Pending and finished dialogs.
pub struct Dialogs {
    tx: Sender<(Purpose, Option<PathBuf>)>,
    rx: Receiver<(Purpose, Option<PathBuf>)>,
    open: usize,
    /// Localized filter names: images, all files, documents (NF-007).
    names: [String; 3],
}

impl Default for Dialogs {
    fn default() -> Self {
        let (tx, rx) = channel();
        Self {
            tx,
            rx,
            open: 0,
            names: ["Images".into(), "All files".into(), "Documents".into()],
        }
    }
}

/// File extensions recognized as Markdown (FS-002).
pub use inkra_workspace::tree::MARKDOWN_EXTS;

const IMAGE_EXTS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", "bmp", "svg", "tif", "tiff",
];

impl Dialogs {
    /// Uses `i18n` for the filter names.
    pub fn set_language(&mut self, i18n: &inkra_i18n::I18n) {
        self.names = [
            i18n.tr("filter-images"),
            i18n.tr("filter-all"),
            i18n.tr("filter-documents"),
        ];
    }

    /// Whether a dialog is showing.
    pub fn busy(&self) -> bool {
        self.open > 0
    }

    /// Opens a dialog for `purpose`; `dir` is the starting directory and `name`
    /// the suggested file name for save dialogs.
    pub fn open(
        &mut self,
        ctx: &egui::Context,
        purpose: Purpose,
        dir: Option<PathBuf>,
        name: Option<String>,
    ) {
        if self.open > 0 {
            return;
        }
        self.open += 1;
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        let [images, all, documents] = self.names.clone();
        let run = move || {
            let mut d = rfd::FileDialog::new();
            if let Some(dir) = dir {
                d = d.set_directory(dir);
            }
            let result = match &purpose {
                Purpose::InsertImage => d.add_filter(&images, IMAGE_EXTS).pick_file(),
                Purpose::Open => d
                    .add_filter("Markdown", &inkra_workspace::tree::markdown_exts())
                    .add_filter(&all, &["*"])
                    .pick_file(),
                Purpose::OpenFolder | Purpose::MoveTo(_) => d.pick_folder(),
                Purpose::Import => d
                    .add_filter(&documents, inkra_export::pandoc::IMPORT_EXTS)
                    .pick_file(),
                Purpose::ImportSettings => d.add_filter("JSON", &["json"]).pick_file(),
                Purpose::RelinkImage(_) => d.add_filter(&images, IMAGE_EXTS).pick_file(),
                Purpose::ExportSettings => {
                    if let Some(n) = name {
                        d = d.set_file_name(n);
                    }
                    d.add_filter("JSON", &["json"]).save_file()
                }
                Purpose::Export { format, .. } => {
                    // The suggested name carries the extension (custom exports
                    // define their own).
                    let ext = name
                        .as_deref()
                        .and_then(|n| n.rsplit_once('.'))
                        .map_or_else(|| format.ext().to_owned(), |(_, e)| e.to_owned());
                    if let Some(n) = name {
                        d = d.set_file_name(n);
                    }
                    let mut d = d.add_filter(ext.to_uppercase(), &[ext.as_str()]);
                    if *format == inkra_engine::exporting::ExportFormat::Block {
                        d = d.add_filter("SVG", &["svg"]);
                    } else if ext == "png" {
                        d = d.add_filter("JPEG", &["jpg", "jpeg"]);
                    }
                    d.save_file()
                }
                Purpose::SaveAs { .. } => {
                    if let Some(n) = name {
                        d = d.set_file_name(n);
                    }
                    d.add_filter("Markdown", &["md", "markdown", "txt"])
                        .save_file()
                }
            };
            let _ = tx.send((purpose, result));
            ctx.request_repaint();
        };
        if cfg!(target_os = "macos") {
            run();
        } else if let Err(e) = std::thread::Builder::new()
            .name("inkra-dialog".into())
            .spawn(run)
        {
            log::warn!("cannot open dialog: {e}");
            self.open -= 1;
        }
    }

    /// Finished dialogs.
    pub fn poll(&mut self) -> Vec<(Purpose, Option<PathBuf>)> {
        let mut out = Vec::new();
        while let Ok(r) = self.rx.try_recv() {
            self.open = self.open.saturating_sub(1);
            out.push(r);
        }
        out
    }
}

/// URL schemes that never reach the system handler (NF-005): script and inline
/// data URLs, and Windows protocol handlers known to start programs.
const BLOCKED_SCHEMES: &[&str] = &[
    "javascript",
    "vbscript",
    "data",
    "ms-msdt",
    "search-ms",
    "search",
    "ms-officecmd",
    "ms-appinstaller",
    "shell",
];

/// The scheme of `url` in lower case, if it has one. A single letter before
/// `:` is a Windows drive (`C:\x`), not a scheme.
fn scheme(url: &str) -> Option<String> {
    let (s, _) = url.split_once(':')?;
    let valid = s.len() > 1
        && s.starts_with(|c: char| c.is_ascii_alphabetic())
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    valid.then(|| s.to_ascii_lowercase())
}

/// Whether `url` may be handed to the system handler (NF-005): not empty, not
/// a blocked scheme, not a program (`file:` URL or plain path), and not
/// something the opener would read as an option (`-x`, Windows `/switch`).
pub fn may_open(url: &str) -> bool {
    let u = url.trim();
    if u.is_empty() || u.starts_with('-') || (cfg!(windows) && u.starts_with('/')) {
        return false;
    }
    // Control characters (a newline in a link) are never part of a target.
    if u.chars().any(char::is_control) {
        return false;
    }
    match scheme(u).as_deref() {
        Some(s) if BLOCKED_SCHEMES.contains(&s) => false,
        Some("file") => {
            let path = u[5..].trim_start_matches('/');
            let path = crate::images::percent_decode(path);
            !is_program(Path::new(&path))
        }
        Some(_) => true,
        None => !is_program(Path::new(u)),
    }
}

/// Opens `url` with the system handler (browser, mail client, default app).
/// Every target goes through [`may_open`] first (NF-005).
pub fn open_external(url: &str) -> bool {
    let u = url.trim();
    if !may_open(u) {
        log::warn!("refused to open {u:?} (NF-005)");
        return false;
    }
    let result = if cfg!(windows) {
        std::process::Command::new("explorer").arg(u).spawn()
    } else if cfg!(target_os = "macos") {
        std::process::Command::new("open").arg(u).spawn()
    } else {
        std::process::Command::new("xdg-open").arg(u).spawn()
    };
    match result {
        Ok(_) => true,
        Err(e) => {
            log::warn!("cannot open {u}: {e}");
            false
        }
    }
}

/// Whether a file would run as a program when opened (NF-005: links never
/// start programs).
pub fn is_program(p: &Path) -> bool {
    p.extension().and_then(|e| e.to_str()).is_some_and(|e| {
        matches!(
            e.to_ascii_lowercase().as_str(),
            "exe"
                | "com"
                | "bat"
                | "cmd"
                | "msi"
                | "ps1"
                | "vbs"
                | "vbe"
                | "js"
                | "jse"
                | "wsf"
                | "wsh"
                | "scr"
                | "pif"
                | "lnk"
                | "reg"
                | "cpl"
                | "sh"
                | "command"
                | "app"
                | "jar"
                | "appimage"
                | "run"
                | "desktop"
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn programs_and_scripts_are_not_opened() {
        assert!(is_program(Path::new("C:/x/setup.EXE")));
        assert!(is_program(Path::new("/tmp/run.sh")));
        assert!(!is_program(Path::new("/tmp/notes.md")));
        assert!(!is_program(Path::new("/tmp/report.pdf")));
        assert!(!open_external("javascript:alert(1)"));
    }

    #[test]
    fn dangerous_link_targets_are_refused() {
        // Script and data URLs, in any case and with surrounding space.
        for u in [
            "javascript:alert(1)",
            " JavaScript:alert(1)",
            "vbscript:msgbox(1)",
            "VBScript:x",
            "data:text/html,<script>alert(1)</script>",
            "DATA:image/svg+xml;base64,PHN2Zz4=",
            "ms-msdt:/id PCWDiagnostic",
            "search-ms:query=x",
        ] {
            assert!(!may_open(u), "{u}");
        }
        // file: URLs and plain paths to programs.
        for u in [
            "file:///C:/Windows/System32/calc.exe",
            "file:///tmp/run.sh",
            "FILE:///tmp/Setup.MSI",
            "file:///C:/x/evil%2Eexe",
            "/usr/local/bin/tool.AppImage",
            "C:\\Users\\x\\a.bat",
        ] {
            assert!(!may_open(u), "{u}");
        }
        // Option-like and malformed targets.
        for u in ["", "  ", "--help", "-e", "https://x\nexample.com"] {
            assert!(!may_open(u), "{u:?}");
        }
        // Ordinary targets still open.
        for u in [
            "https://example.com/a?b=c#d",
            "http://example.com",
            "mailto:me@example.com",
            "file:///tmp/report.pdf",
            "zotero://select/items/1",
            "C:\\Users\\x\\notes.pdf",
        ] {
            assert!(may_open(u), "{u}");
        }
    }
}
