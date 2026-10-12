//! Native file dialogs (rfd, UI-040). They run off the UI thread where the platform
//! allows it (macOS requires the main thread) and report back through a channel.

use std::path::PathBuf;
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
    /// Save as (FS-003); then close the window if `close_after`.
    SaveAs {
        /// Close after saving.
        close_after: bool,
    },
}

/// Pending and finished dialogs.
pub struct Dialogs {
    tx: Sender<(Purpose, Option<PathBuf>)>,
    rx: Receiver<(Purpose, Option<PathBuf>)>,
    open: usize,
}

impl Default for Dialogs {
    fn default() -> Self {
        let (tx, rx) = channel();
        Self { tx, rx, open: 0 }
    }
}

/// File extensions recognized as Markdown (FS-002).
pub const MARKDOWN_EXTS: &[&str] = &[
    "md",
    "markdown",
    "mdown",
    "mmd",
    "text",
    "txt",
    "mdx",
    "rmarkdown",
    "rmd",
    "mkd",
    "mdwn",
    "mdtxt",
    "mdtext",
    "apib",
];

const IMAGE_EXTS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", "bmp", "svg", "tif", "tiff",
];

impl Dialogs {
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
        let run = move || {
            let mut d = rfd::FileDialog::new();
            if let Some(dir) = dir {
                d = d.set_directory(dir);
            }
            let result = match &purpose {
                Purpose::InsertImage => d.add_filter("Images", IMAGE_EXTS).pick_file(),
                Purpose::Open => d
                    .add_filter("Markdown", MARKDOWN_EXTS)
                    .add_filter("All files", &["*"])
                    .pick_file(),
                Purpose::OpenFolder => d.pick_folder(),
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

/// Opens `url` with the system handler (browser, mail client). `javascript:`
/// links are refused (NF-005).
pub fn open_external(url: &str) -> bool {
    let u = url.trim();
    if u.to_ascii_lowercase().starts_with("javascript:") || u.is_empty() {
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
