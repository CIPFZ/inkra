//! Renders a document in the full Inkra UI to a PNG without a window (NF-010):
//! for regression images and documentation pictures.
//!
//! ```sh
//! cargo run -p inkra-ui --example ui_shot -- doc.md out.png [--size 1100x760] [--theme Ink] [--themes DIR] [--pref KEY=JSON] [--source] [--sidebar] [--lang zh-CN] [--scale 2]
//! ```

use std::process::ExitCode;

use egui_kittest::Harness;
use inkra_engine::{Engine, Preferences};
use inkra_i18n::I18n;
use inkra_ui::Shell;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut pos = Vec::new();
    let (mut w, mut h) = (1100.0f32, 760.0f32);
    let mut theme = None;
    let mut themes_dir = None;
    let mut extra: Vec<(String, serde_json::Value)> = Vec::new();
    let mut source = false;
    let mut sidebar = false;
    let mut lang = "en-US".to_owned();
    let mut scale = 1.0f32;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--size" => {
                if let Some((a, b)) = it.next().and_then(|s| s.split_once('x')) {
                    w = a.parse().unwrap_or(w);
                    h = b.parse().unwrap_or(h);
                }
            }
            "--theme" => theme = it.next().cloned(),
            "--themes" => themes_dir = it.next().map(std::path::PathBuf::from),
            // `--pref key=json`, e.g. `--pref appearance.titleBar='"integrated"'`.
            "--pref" => {
                if let Some((k, v)) = it.next().and_then(|s| s.split_once('=')) {
                    let v = serde_json::from_str(v).unwrap_or_else(|_| serde_json::json!(v));
                    extra.push((k.to_owned(), v));
                }
            }
            "--lang" => lang = it.next().cloned().unwrap_or(lang),
            "--scale" => scale = it.next().and_then(|s| s.parse().ok()).unwrap_or(1.0),
            "--source" => source = true,
            "--sidebar" => sidebar = true,
            _ => pos.push(a.clone()),
        }
    }
    let [input, output] = pos.as_slice() else {
        eprintln!(
            "usage: ui_shot <doc.md> <out.png> [--size WxH] [--theme NAME] [--themes DIR] [--pref KEY=JSON] [--source] [--sidebar] [--lang TAG] [--scale K]"
        );
        return ExitCode::from(1);
    };
    let mut prefs = Preferences::default();
    // Still frames: no blinking caret, no animations, no network.
    prefs.set("editor.cursorBlink", serde_json::json!(false));
    prefs.set("editor.smoothScroll", serde_json::json!(false));
    prefs.set("general.checkUpdates", serde_json::json!(false));
    if let Some(t) = &theme {
        prefs.set("appearance.theme", serde_json::json!(t));
    }
    for (k, v) in extra {
        prefs.set(&k, v);
    }
    let mut engine = Engine::new(prefs);
    if let Err(e) = engine.execute("file.open", &serde_json::json!({ "path": input })) {
        eprintln!("ui_shot: {input}: {e}");
        return ExitCode::from(2);
    }
    engine.take_effects();
    // A user theme from `--themes DIR` (theme development).
    if let (Some(t), Some(d)) = (&theme, &themes_dir) {
        match inkra_engine::theme::load(t, Some(d)) {
            Ok(s) => engine.state.style = s,
            Err(e) => eprintln!("ui_shot: theme {t}: {e}"),
        }
    }
    if source {
        engine.state.editor.mode = inkra_engine::editor::ViewMode::Source;
    }
    engine.state.view.sidebar_visible = sidebar;
    inkra_highlight::load_now();
    let shell = Shell::new(engine, I18n::new(&lang), None);
    let mut harness = Harness::builder()
        .with_size(egui::vec2(w, h))
        .with_pixels_per_point(scale)
        .wgpu()
        .build_ui_state(|ui, shell: &mut Shell| shell.ui(ui), shell);
    // Fonts, syntax definitions and images arrive over a few frames.
    for _ in 0..12 {
        harness.step();
        std::thread::sleep(std::time::Duration::from_millis(30));
    }
    match harness.render() {
        Ok(img) => match img.save(output) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("ui_shot: {output}: {e}");
                ExitCode::from(2)
            }
        },
        Err(e) => {
            eprintln!("ui_shot: rendering failed: {e}");
            ExitCode::from(3)
        }
    }
}
