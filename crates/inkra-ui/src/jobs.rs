//! Background export, print and import jobs (AD-5: never on the UI thread).

use std::sync::mpsc::{Receiver, Sender, channel};

use inkra_engine::exporting::{ExportJob, JobResult};

type Done = (ExportJob, Result<JobResult, String>);

/// Running jobs.
pub struct Jobs {
    tx: Sender<Done>,
    rx: Receiver<Done>,
    running: usize,
}

impl Default for Jobs {
    fn default() -> Self {
        let (tx, rx) = channel();
        Self { tx, rx, running: 0 }
    }
}

impl Jobs {
    /// Starts `job`.
    pub fn spawn(&mut self, ctx: &egui::Context, job: ExportJob) {
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        self.running += 1;
        let spawned = std::thread::Builder::new()
            .name("inkra-export".into())
            .spawn(move || {
                let t = std::time::Instant::now();
                // A panic in an exporter must not take the app down.
                let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    inkra_engine::exporting::run(&job)
                }))
                .unwrap_or_else(|_| Err("internal error during export".into()));
                log::info!(
                    "job {} finished in {:?}: {:?}",
                    job.out.display(),
                    t.elapsed(),
                    r.as_ref().map(|_| ())
                );
                let _ = tx.send((job, r));
                ctx.request_repaint();
            });
        if let Err(e) = spawned {
            log::warn!("cannot start export: {e}");
            self.running -= 1;
        }
    }

    /// Jobs still running.
    pub fn running(&self) -> usize {
        self.running
    }

    /// Finished jobs.
    pub fn poll(&mut self) -> Vec<Done> {
        let mut out = Vec::new();
        while let Ok(d) = self.rx.try_recv() {
            self.running = self.running.saturating_sub(1);
            out.push(d);
        }
        out
    }
}

/// Hands a PDF to the system's print function (EX-002: print via PDF).
pub fn print_pdf(path: &std::path::Path) -> std::io::Result<()> {
    if cfg!(windows) {
        std::process::Command::new("powershell")
            .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command"])
            .arg(format!(
                "Start-Process -FilePath '{}' -Verb Print",
                path.display().to_string().replace('\'', "''")
            ))
            .spawn()
            .map(|_| ())
    } else if cfg!(target_os = "macos") {
        // Preview shows its print dialog for `-p`.
        std::process::Command::new("open")
            .args(["-a", "Preview"])
            .arg(path)
            .spawn()
            .map(|_| ())
    } else {
        std::process::Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map(|_| ())
    }
}
