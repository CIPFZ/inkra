//! # inkra-ui
//!
//! **Layer L6** (see SPEC 2.2).
//!
//! egui front end: windows, sidebar, editing canvas, menus, dialogs, toolbar and
//! status bar. The UI renders engine state and turns user input into command
//! invocations; it never edits documents directly.

pub mod fonts;
pub mod keys;
pub mod menubar;
#[cfg(target_os = "macos")]
pub mod native_menu;

use std::time::Duration;

use inkra_engine::keymap::Os;
use inkra_engine::prefs::LoadProblem;
use inkra_engine::{Effect, Engine, Notice};
use inkra_i18n::I18n;
use serde_json::Value;

pub use menubar::Invocation;

/// The 256 px application icon (UI-050, UI-052).
pub const ICON_PNG: &[u8] = include_bytes!("../../../assets/brand/inkra-256.png");

/// How long a toast stays visible (UI-041).
const TOAST_SECS: f64 = 3.0;

struct Toast {
    text: String,
    until: f64,
}

/// The application shell of one window.
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
        Self {
            engine,
            i18n,
            os: Os::current(),
            about_open: false,
            prefs_problem,
            toasts: Vec::new(),
            icon: None,
            quit_requested: false,
            #[cfg(target_os = "macos")]
            native_menu: None,
        }
    }

    /// One-time setup of fonts, style and (on macOS) the native menu.
    pub fn setup(&mut self, ctx: &egui::Context) {
        fonts::install(ctx);
        self.os = keys::os_of(ctx.os());
        #[cfg(target_os = "macos")]
        {
            self.native_menu = Some(native_menu::NativeMenu::install(
                &mut self.engine,
                &self.i18n,
            ));
        }
    }

    /// Whether `app.quit` was executed.
    pub fn quit_requested(&self) -> bool {
        self.quit_requested
    }

    /// Whether the About dialog is open.
    pub fn about_open(&self) -> bool {
        self.about_open
    }

    /// Runs a command and applies its effects.
    pub fn invoke(&mut self, ctx: &egui::Context, id: &str, params: &Value) {
        if let Err(e) = self.engine.execute(id, params) {
            log::debug!("{e}");
        }
        self.apply_effects(ctx);
    }

    fn apply_effects(&mut self, ctx: &egui::Context) {
        for effect in self.engine.take_effects() {
            match effect {
                Effect::Quit => {
                    self.quit_requested = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                Effect::ShowAbout => self.about_open = true,
                Effect::Notify(Notice::OperationFailed { command }) => {
                    let text = self
                        .i18n
                        .tr_args("notice-operation-failed", &[("command", command.into())]);
                    let now = ctx.input(|i| i.time);
                    self.toasts.push(Toast {
                        text,
                        until: now + TOAST_SECS,
                    });
                }
            }
        }
    }

    /// Dispatches key presses bound in the keymap and removes them from the input.
    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        let os = self.os;
        let mut hits = Vec::new();
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
                match keymap.resolve(&chord, false) {
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

    /// Draws the whole window.
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
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

        if !self.uses_native_menu() {
            let mut invocation = None;
            egui::Panel::top("inkra-menubar").show(ui, |ui| {
                invocation = menubar::show(ui, &mut self.engine, &self.i18n, self.os);
            });
            if let Some(inv) = invocation {
                self.invoke(&ctx, &inv.id, &inv.params);
            }
        }

        if self.engine.state.view.sidebar_visible {
            egui::Panel::left("inkra-sidebar")
                .resizable(true)
                .default_size(240.0)
                .size_range(180.0..=(ctx.content_rect().width() * 0.5).max(180.0))
                .show(ui, |ui| {
                    ui.weak(self.i18n.tr("sidebar-placeholder"));
                });
        }

        egui::CentralPanel::default().show(ui, |_ui| {});

        self.about_dialog(&ctx);
        self.prefs_problem_dialog(&ctx);
        self.toasts(&ctx);
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

    fn icon(&mut self, ctx: &egui::Context) -> Option<egui::TextureHandle> {
        if self.icon.is_none() {
            match image::load_from_memory(ICON_PNG) {
                Ok(img) => {
                    let rgba = img.to_rgba8();
                    let size = [rgba.width() as usize, rgba.height() as usize];
                    let color = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
                    self.icon = Some(ctx.load_texture("inkra-icon", color, Default::default()));
                }
                Err(e) => log::warn!("cannot decode icon: {e}"),
            }
        }
        self.icon.clone()
    }

    fn about_dialog(&mut self, ctx: &egui::Context) {
        if !self.about_open {
            return;
        }
        let icon = self.icon(ctx);
        let i18n = &self.i18n;
        let response = egui::Modal::new(egui::Id::new("inkra-about")).show(ctx, |ui| {
            ui.set_width(320.0);
            ui.vertical_centered(|ui| {
                if let Some(icon) = &icon {
                    ui.add(egui::Image::new(icon).fit_to_exact_size(egui::vec2(128.0, 128.0)));
                }
                ui.heading(i18n.tr("app-name"));
                ui.label(i18n.tr("app-name-zh"));
                ui.label(i18n.tr_args(
                    "about-version",
                    &[("version", env!("CARGO_PKG_VERSION").into())],
                ));
                ui.weak(i18n.tr("app-tagline"));
                ui.weak(i18n.tr("about-license"));
                ui.add_space(8.0);
                ui.button(i18n.tr("dialog-ok")).clicked()
            })
            .inner
        });
        if response.inner || response.should_close() {
            self.about_open = false;
        }
    }

    fn prefs_problem_dialog(&mut self, ctx: &egui::Context) {
        let Some(text) = &self.prefs_problem else {
            return;
        };
        let ok = self.i18n.tr("dialog-ok");
        let response = egui::Modal::new(egui::Id::new("inkra-prefs-problem")).show(ctx, |ui| {
            ui.set_max_width(420.0);
            ui.label(text.as_str());
            ui.add_space(8.0);
            ui.vertical_centered(|ui| ui.button(ok).clicked()).inner
        });
        if response.inner || response.should_close() {
            self.prefs_problem = None;
        }
    }

    fn toasts(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        self.toasts.retain(|t| t.until > now);
        if self.toasts.is_empty() {
            return;
        }
        egui::Area::new(egui::Id::new("inkra-toasts"))
            .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-16.0, -16.0))
            .show(ctx, |ui| {
                for t in &self.toasts {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.set_max_width(360.0);
                        ui.label(&t.text);
                    });
                }
            });
        let next = self
            .toasts
            .iter()
            .map(|t| t.until)
            .fold(f64::INFINITY, f64::min);
        ctx.request_repaint_after(Duration::from_secs_f64((next - now).max(0.0)));
    }
}

#[cfg(test)]
mod tests;
