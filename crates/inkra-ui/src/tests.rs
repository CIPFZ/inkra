use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};

use super::*;

fn shell(lang: &str) -> Shell {
    Shell::new(Engine::new(Default::default()), I18n::new(lang), None)
}

fn harness(lang: &str) -> Harness<'static, Shell> {
    let mut h = Harness::builder()
        .with_size(egui::vec2(900.0, 600.0))
        .build_ui_state(|ui, shell: &mut Shell| shell.ui(ui), shell(lang));
    h.run();
    h
}

#[test]
fn menu_bar_shows_localized_top_level_menus() {
    let h = harness("en-US");
    for name in [
        "File",
        "Edit",
        "Paragraph",
        "Format",
        "View",
        "Themes",
        "Help",
    ] {
        h.get_by_label(name);
    }
    let h = harness("zh-CN");
    for name in ["文件", "编辑", "段落", "格式", "视图", "主题", "帮助"] {
        h.get_by_label(name);
    }
}

#[test]
fn toggle_sidebar_via_menu() {
    let mut h = harness("en-US");
    assert!(!h.state().engine.state.view.sidebar_visible);
    h.get_by_label("View").click();
    h.run();
    h.get_by_label_contains("Sidebar").click();
    h.run();
    assert!(h.state().engine.state.view.sidebar_visible);
    h.get_by_label("Sidebar");
}

#[test]
fn toggle_sidebar_via_shortcut() {
    let mut h = harness("en-US");
    h.key_press_modifiers(
        egui::Modifiers {
            command: true,
            ctrl: !cfg!(target_os = "macos"),
            shift: true,
            ..Default::default()
        },
        egui::Key::L,
    );
    h.run();
    assert!(h.state().engine.state.view.sidebar_visible);
}

#[test]
fn unimplemented_items_are_disabled_and_about_opens() {
    let mut h = harness("en-US");
    h.get_by_label("Help").click();
    h.run();
    h.get_by_label_contains("About Inkra").click();
    h.run();
    assert!(h.state().about_open());
    h.get_by_label("Version 0.1.0");
    h.get_by_label("OK").click();
    h.run();
    assert!(!h.state().about_open());

    h.get_by_label("File").click();
    h.run();
    let save = h.get_by_label_contains("Save All");
    assert!(save.accesskit_node().is_disabled());
}

#[test]
fn quit_closes_viewport() {
    let mut h = harness("en-US");
    h.get_by_label("File").click();
    h.run();
    h.get_by_label_contains("Quit").click();
    h.run();
    assert!(h.state().quit_requested());
}
