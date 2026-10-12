//! Inkra desktop application entry point.

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod args;
mod logging;
#[cfg(windows)]
mod platform_windows;

use std::sync::Arc;
use std::time::Instant;

use eframe::egui;
use inkra_engine::{Engine, Preferences};
use inkra_i18n::I18n;
use inkra_ui::Shell;
use inkra_workspace::Paths;

/// Application id used for `.desktop`, Flatpak, bundle id and AppUserModelID.
const APP_ID: &str = "io.inkra.Inkra";

struct App {
    shell: Shell,
    started: Instant,
    frames: u64,
    measure_startup: bool,
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.frames += 1;
        if self.frames == 2 {
            // The second frame starts after the first one was presented.
            let ms = self.started.elapsed().as_secs_f64() * 1000.0;
            log::info!("startup: first frame presented after {ms:.1} ms");
            if self.measure_startup {
                println!("startup_ms={ms:.1}");
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
        if self.measure_startup && self.frames == 1 {
            ui.ctx().request_repaint();
        }
        self.shell.ui(ui);
    }
}

fn icon() -> Option<egui::IconData> {
    match eframe::icon_data::from_png_bytes(inkra_ui::ICON_PNG) {
        Ok(i) => Some(i),
        Err(e) => {
            log::warn!("cannot decode window icon: {e}");
            None
        }
    }
}

fn main() -> std::process::ExitCode {
    let started = Instant::now();
    let args = match args::parse(std::env::args().skip(1)) {
        Ok(a) => a,
        Err(e) => {
            #[cfg(windows)]
            platform_windows::attach_parent_console();
            eprintln!("inkra: {e}\n\n{}", args::USAGE);
            return std::process::ExitCode::from(1);
        }
    };
    #[cfg(windows)]
    if args.version || args.help || args.measure_startup {
        platform_windows::attach_parent_console();
    }
    if args.version {
        println!("Inkra {}", env!("CARGO_PKG_VERSION"));
        return std::process::ExitCode::SUCCESS;
    }
    if args.help {
        print!("{}", args::USAGE);
        return std::process::ExitCode::SUCCESS;
    }
    #[cfg(windows)]
    platform_windows::set_app_user_model_id();

    let paths = args
        .config
        .as_ref()
        .map(Paths::rooted)
        .unwrap_or_else(Paths::platform);
    logging::init(&paths.logs_dir());
    log::info!("Inkra {} starting", env!("CARGO_PKG_VERSION"));

    let (prefs, problem) = Preferences::load(&paths.preferences_file());
    let lang = match prefs.string("general.language").as_str() {
        "" | "system" => inkra_i18n::system_language(),
        other => other.to_owned(),
    };
    let i18n = I18n::new(&lang);
    let engine = Engine::new(prefs);
    let title = format!("{} - Inkra", i18n.tr("untitled"));

    let mut viewport = egui::ViewportBuilder::default()
        .with_title(title)
        .with_app_id(APP_ID)
        .with_inner_size([1100.0, 760.0])
        .with_min_inner_size([480.0, 320.0]);
    if let Some(icon) = icon() {
        viewport = viewport.with_icon(Arc::new(icon));
    }
    let options = eframe::NativeOptions {
        viewport,
        persistence_path: Some(paths.data.join("egui.ron")),
        ..Default::default()
    };
    let measure_startup = args.measure_startup;
    let result = eframe::run_native(
        APP_ID,
        options,
        Box::new(move |cc| {
            let mut shell = Shell::new(engine, i18n, problem);
            shell.setup(&cc.egui_ctx);
            log::info!(
                "startup: UI set up after {:.1} ms",
                started.elapsed().as_secs_f64() * 1e3
            );
            Ok(Box::new(App {
                shell,
                started,
                frames: 0,
                measure_startup,
            }))
        }),
    );
    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            log::error!("fatal: {e}");
            eprintln!("inkra: {e}");
            std::process::ExitCode::from(2)
        }
    }
}
