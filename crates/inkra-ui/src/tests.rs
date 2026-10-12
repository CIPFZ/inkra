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
    let setup = h.get_by_label_contains("Page Setup");
    assert!(setup.accesskit_node().is_disabled());
}

#[test]
fn quit_closes_viewport() {
    let mut h = harness("en-US");
    h.get_by_label("File").click();
    h.run();
    h.get_by_label_contains("Quit").click();
    // The harness has no OS window to close, so the close request repeats.
    h.run_steps(3);
    assert!(h.state().quit_requested());
}

fn text(h: &Harness<'_, Shell>) -> String {
    h.state().engine.state.editor.text()
}

fn type_str(h: &mut Harness<'_, Shell>, s: &str) {
    for c in s.chars() {
        h.event(egui::Event::Text(c.to_string()));
        h.step();
    }
}

fn editor_harness(content: &str) -> Harness<'static, Shell> {
    let mut sh = shell("en-US");
    sh.engine.state.editor = inkra_engine::editor::Editor::from_text(content);
    let mut h = Harness::builder()
        .with_size(egui::vec2(900.0, 600.0))
        .build_ui_state(|ui, shell: &mut Shell| shell.ui(ui), sh);
    h.run();
    h
}

#[test]
fn editor_typing_lists_and_undo() {
    let mut h = editor_harness("");
    type_str(&mut h, "- a");
    h.key_press(egui::Key::Enter);
    h.step();
    type_str(&mut h, "b");
    assert_eq!(text(&h), "- a\n- b");
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Z);
    h.step();
    assert_eq!(text(&h), "- a\n- ");
    h.key_press_modifiers(
        egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
        egui::Key::Z,
    );
    h.step();
    assert_eq!(text(&h), "- a\n- b");
}

#[test]
fn editor_arrows_and_selection() {
    let mut h = editor_harness("hello world");
    h.state_mut().engine.state.editor.sel = inkra_doc::Selection::caret(11);
    h.key_press_modifiers(egui::Modifiers::SHIFT, egui::Key::ArrowLeft);
    h.key_press_modifiers(egui::Modifiers::SHIFT, egui::Key::ArrowLeft);
    h.step();
    assert_eq!(
        h.state().engine.state.editor.sel,
        inkra_doc::Selection::new(11, 9)
    );
    type_str(&mut h, "X");
    assert_eq!(text(&h), "hello worX");
    h.key_press(egui::Key::Home);
    h.step();
    assert_eq!(h.state().engine.state.editor.sel.head, 0);
    h.key_press(egui::Key::End);
    h.step();
    assert_eq!(h.state().engine.state.editor.sel.head, 10);
}

#[test]
fn editor_vertical_movement_keeps_column() {
    let mut h = editor_harness("abcdef\nab\nabcdef");
    h.state_mut().engine.state.editor.sel = inkra_doc::Selection::caret(5);
    h.key_press(egui::Key::ArrowDown);
    h.step();
    assert_eq!(
        h.state().engine.state.editor.sel.head,
        9,
        "clamped to the short line end"
    );
    h.key_press(egui::Key::ArrowDown);
    h.step();
    assert_eq!(
        h.state().engine.state.editor.sel.head,
        15,
        "back to the desired column"
    );
}

#[test]
fn editor_ime_preedit_and_commit() {
    let mut h = editor_harness("");
    h.event(egui::Event::Ime(egui::ImeEvent::Preedit {
        text: "zhong".into(),
        active_range_chars: None,
    }));
    h.step();
    assert_eq!(
        h.state().engine.state.editor.preedit.as_deref(),
        Some("zhong")
    );
    assert_eq!(text(&h), "", "pre-edit text is not part of the document");
    h.event(egui::Event::Ime(egui::ImeEvent::Commit("中文".into())));
    h.step();
    assert_eq!(text(&h), "中文");
    assert!(h.state().engine.state.editor.preedit.is_none());
    assert!(h.state().caret_screen().is_some());
}

#[test]
fn editor_find_bar() {
    let mut h = editor_harness("one two one");
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::F);
    h.run();
    assert!(h.state().engine.state.editor.find.open);
    type_str(&mut h, "one");
    h.run();
    let e = &h.state().engine.state.editor;
    assert_eq!(e.find.matches.len(), 2);
    assert_eq!(e.find.counter(), "1/2");
}

#[test]
fn typing_latency_100k_chars() {
    // 10万字: ~100k CJK + Latin characters across headings, lists and paragraphs.
    let unit = "## 标题\n\n这是一段用于性能测试的中文段落，包含 **粗体**、*斜体* 和 `code`。Some English words too.\n\n- 列表项一\n- 列表项二\n\n";
    let mut doc = String::new();
    while doc.chars().count() < 100_000 {
        doc.push_str(unit);
    }
    let mut h = editor_harness(&doc);
    let mid = doc.len() / 2;
    let mid = (mid..doc.len()).find(|&p| doc.is_char_boundary(p)).unwrap();
    h.state_mut().engine.state.editor.sel = inkra_doc::Selection::caret(mid);
    h.run();
    let mut times = Vec::new();
    for c in "abcdefghijklmnopqrstuvwxyz".chars().cycle().take(60) {
        h.event(egui::Event::Text(c.to_string()));
        let t = std::time::Instant::now();
        h.step();
        times.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    times.sort_by(f64::total_cmp);
    let p95 = times[(times.len() * 95) / 100 - 1];
    eprintln!(
        "typing latency (CPU, headless): p50 {:.2} ms, p95 {p95:.2} ms",
        times[times.len() / 2]
    );
    if !cfg!(debug_assertions) {
        assert!(p95 < 16.0, "p95 {p95} ms");
    }
}
