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
    session_file: std::path::PathBuf,
    save_session: bool,
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.frames += 1;
        if let Some(cpu) = frame.info().cpu_usage {
            self.shell.record_frame_cpu(cpu * 1000.0);
        }
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
        // NF-002 / iron rule 10: a bug in a frame must never take the documents
        // with it. Panics are caught here, unsaved content goes to the recovery
        // directory immediately, and the next frame carries on.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.shell.ui(ui)));
        if let Err(payload) = result {
            let msg = inkra_engine::command::panic_message(payload.as_ref());
            log::error!("frame panicked: {msg}");
            inkra_engine::files::write_recovery(&self.shell.engine.state);
            self.shell.notify_failure(&msg);
            ui.ctx().request_repaint();
        }
    }

    fn on_exit(&mut self) {
        if self.save_session && !self.measure_startup {
            let s = self.shell.session();
            if let Err(e) = s.save(&self.session_file) {
                log::warn!("cannot save session: {e}");
            }
        }
        // Clean exit: recovery entries of unmodified windows are removed, modified
        // ones were confirmed by the user (FS-005) or are kept for recovery.
        inkra_engine::files::write_recovery(&self.shell.engine.state);
    }
}

/// wgpu setup: one backend per platform (DX12 on Windows, which falls back to the
/// WARP software adapter without a GPU; Metal on macOS; Vulkan or GL on Linux)
/// and our own adapter choice, which saves wgpu a second adapter enumeration
/// (~100 ms on Windows). `WGPU_BACKEND` still overrides the backend.
fn wgpu_options() -> eframe::egui_wgpu::WgpuConfiguration {
    use eframe::egui_wgpu::{WgpuConfiguration, WgpuSetup, WgpuSetupCreateNew};
    use eframe::wgpu;
    let mut setup = WgpuSetupCreateNew::without_display_handle();
    let default_backends = if cfg!(windows) {
        wgpu::Backends::DX12
    } else if cfg!(target_os = "macos") {
        wgpu::Backends::METAL
    } else {
        wgpu::Backends::VULKAN | wgpu::Backends::GL
    };
    setup.instance_descriptor.backends = wgpu::Backends::from_env().unwrap_or(default_backends);
    setup.native_adapter_selector = Some(Arc::new(|adapters, surface| {
        let rank = |t: wgpu::DeviceType| match t {
            wgpu::DeviceType::IntegratedGpu => 0,
            wgpu::DeviceType::DiscreteGpu => 1,
            wgpu::DeviceType::VirtualGpu => 2,
            wgpu::DeviceType::Other => 3,
            wgpu::DeviceType::Cpu => 4,
        };
        adapters
            .iter()
            .filter(|a| surface.is_none_or(|s| a.is_surface_supported(s)))
            .min_by_key(|a| rank(a.get_info().device_type))
            .cloned()
            .ok_or_else(|| "no wgpu adapter can present to this window".to_owned())
    }));
    WgpuConfiguration {
        wgpu_setup: WgpuSetup::CreateNew(setup),
        ..Default::default()
    }
}

/// Forwards all events to eframe, noting `Ctrl+V` first: egui's winit
/// integration only reports pastes when the clipboard has text, but Inkra also
/// pastes images, files and HTML (ED-016).
struct Hooked<'a> {
    inner: eframe::EframeWinitApplication<'a>,
    mods: winit::keyboard::ModifiersState,
    raw: Arc<inkra_ui::RawKeys>,
}

impl winit::application::ApplicationHandler<eframe::UserEvent> for Hooked<'_> {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        self.inner.resumed(event_loop);
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        window_id: winit::window::WindowId,
        event: winit::event::WindowEvent,
    ) {
        use std::sync::atomic::Ordering;
        use winit::event::{ElementState, WindowEvent};
        use winit::keyboard::{Key, KeyCode, PhysicalKey};
        match &event {
            WindowEvent::ModifiersChanged(m) => self.mods = m.state(),
            WindowEvent::KeyboardInput { event: k, .. }
                if k.state == ElementState::Pressed && !k.repeat =>
            {
                let primary = if cfg!(target_os = "macos") {
                    self.mods.super_key()
                } else {
                    self.mods.control_key()
                };
                let is_v = matches!(&k.logical_key, Key::Character(c) if c.eq_ignore_ascii_case("v"))
                    || k.physical_key == PhysicalKey::Code(KeyCode::KeyV);
                if primary && is_v {
                    self.raw
                        .paste_plain
                        .store(self.mods.shift_key(), Ordering::SeqCst);
                    self.raw.paste.store(true, Ordering::SeqCst);
                }
            }
            _ => {}
        }
        self.inner.window_event(event_loop, window_id, event);
    }

    fn new_events(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        cause: winit::event::StartCause,
    ) {
        self.inner.new_events(event_loop, cause);
    }

    fn user_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        event: eframe::UserEvent,
    ) {
        self.inner.user_event(event_loop, event);
    }

    fn device_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        device_id: winit::event::DeviceId,
        event: winit::event::DeviceEvent,
    ) {
        self.inner.device_event(event_loop, device_id, event);
    }

    fn about_to_wait(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        self.inner.about_to_wait(event_loop);
    }

    fn suspended(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        self.inner.suspended(event_loop);
    }

    fn exiting(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        self.inner.exiting(event_loop);
    }

    fn memory_warning(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        self.inner.memory_warning(event_loop);
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
    let mut engine = Engine::new(prefs);
    engine.state.paths = Some(paths.clone());
    engine.state.reduce_motion = reduce_motion();
    if engine.state.reduce_motion {
        log::info!("system asks for reduced motion: animations off (UI-033)");
    }
    for p in engine.load_keybindings() {
        log::warn!("{p}");
    }
    let session_file = paths.session_file();
    let session = if args.no_session {
        Default::default()
    } else {
        inkra_workspace::session::Session::load(&session_file)
    };
    engine.state.session = session.clone();
    engine.state.session.windows.clear();
    let title = format!("{} - Inkra", i18n.tr("untitled"));
    // Files and folders from the command line (CM-004, FS-002); otherwise the
    // last session (FS-006, PR-010).
    let files: Vec<std::path::PathBuf> =
        args.paths.iter().filter(|p| p.is_file()).cloned().collect();
    let folder = args.paths.iter().find(|p| p.is_dir()).cloned();
    let restore = files.is_empty()
        && folder.is_none()
        && !args.new
        && !args.no_session
        && engine.state.prefs.string("general.startup") == "restore";
    let source_mode = args.source_mode;
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
        // TH-008: remembering the window size can be turned off.
        persist_window: engine.state.prefs.bool("appearance.rememberWindowSize"),
        wgpu_options: wgpu_options(),
        ..Default::default()
    };
    let measure_startup = args.measure_startup;
    let raw = Arc::new(inkra_ui::RawKeys::default());
    let raw_for_app = raw.clone();
    let event_loop =
        match winit::event_loop::EventLoop::<eframe::UserEvent>::with_user_event().build() {
            Ok(l) => l,
            Err(e) => {
                log::error!("fatal: cannot create event loop: {e}");
                eprintln!("inkra: {e}");
                return std::process::ExitCode::from(2);
            }
        };
    let inner = eframe::create_native(
        APP_ID,
        options,
        Box::new(move |cc| {
            log::info!(
                "startup: window and renderer ready after {:.1} ms",
                started.elapsed().as_secs_f64() * 1e3
            );
            let mut shell = Shell::new(engine, i18n, problem);
            shell.raw_keys = raw_for_app;
            shell.setup(&cc.egui_ctx);
            if let Some(dir) = &folder {
                let s = &mut shell.engine.state.session;
                s.touch_folder(dir);
                s.root = Some(inkra_workspace::paths::canonical(dir));
                let v = &mut shell.engine.state.view;
                v.sidebar_visible = true;
                v.sidebar_view = inkra_engine::state::SidebarView::Files;
            }
            if restore {
                let n = shell.restore_session(&session);
                log::info!("restored {n} window(s) from the last session");
            }
            for f in &files {
                if let Err(e) = shell
                    .engine
                    .execute("file.open", &serde_json::json!({ "path": f }))
                {
                    log::error!("{e}");
                }
                let effects = shell.engine.take_effects();
                shell.apply_startup_effects(effects);
            }
            if source_mode {
                shell.engine.state.editor.mode = inkra_engine::editor::ViewMode::Source;
            }
            log::info!(
                "startup: UI set up after {:.1} ms",
                started.elapsed().as_secs_f64() * 1e3
            );
            Ok(Box::new(App {
                shell,
                started,
                frames: 0,
                measure_startup,
                session_file,
                save_session: !args.no_session,
            }))
        }),
        &event_loop,
    );
    let mut hooked = Hooked {
        inner,
        mods: Default::default(),
        raw,
    };
    let result = event_loop.run_app(&mut hooked);
    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            log::error!("fatal: {e}");
            eprintln!("inkra: {e}");
            std::process::ExitCode::from(2)
        }
    }
}

/// Whether the system asks for reduced motion (UI-033): Windows' animation
/// setting, GNOME's `enable-animations`, macOS' Reduce Motion.
fn reduce_motion() -> bool {
    #[cfg(windows)]
    {
        platform_windows::reduce_motion()
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("defaults")
            .args(["read", "com.apple.universalaccess", "reduceMotion"])
            .output()
            .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).trim() == "1")
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::process::Command::new("gsettings")
            .args(["get", "org.gnome.desktop.interface", "enable-animations"])
            .output()
            .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).trim() == "false")
    }
}
