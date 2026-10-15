use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};

use super::*;

fn shell(lang: &str) -> Shell {
    let mut prefs = inkra_engine::Preferences::default();
    // Jumps land in one frame (the harness stops after a few frames).
    prefs.set("editor.smoothScroll", serde_json::json!(false));
    // A blinking caret keeps asking for frames; the harness wants quiet frames.
    prefs.set("editor.cursorBlink", serde_json::json!(false));
    Shell::new(Engine::new(prefs), I18n::new(lang), None)
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
    h.get_by_label("Outline");
    h.get_by_label("No headings");
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
    h.get_by_label(concat!("Version ", env!("CARGO_PKG_VERSION")));
    h.get_by_label("OK").click();
    h.run();
    assert!(!h.state().about_open());

    h.get_by_label("File").click();
    h.run();
    h.get_by_label_contains("Page Setup").click();
    h.run();
    h.get_by_label("Landscape").click();
    h.run();
    h.get_by_label("OK").click();
    h.run();
    assert_eq!(
        h.state()
            .engine
            .state
            .prefs
            .string("export.pdf.orientation"),
        "landscape"
    );

    h.get_by_label("Format").click();
    h.run();
    h.get_by_label("Image ⏵").hover();
    h.run();
    let ops = h.get_by_label_contains("Image Operations");
    assert!(ops.accesskit_node().is_disabled(), "no image at the caret");
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

#[test]
fn session_restores_sidebar_scroll_and_caret() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("long.md");
    let body: String = (0..300).map(|i| format!("Paragraph {i}\n\n")).collect();
    std::fs::write(&p, &body).unwrap();
    let top = body.find("Paragraph 120").unwrap();
    let session = inkra_workspace::session::Session {
        windows: vec![inkra_workspace::session::WindowSession {
            path: Some(p.clone()),
            caret: top + 3,
            sidebar: true,
            top: Some(top),
            ..Default::default()
        }],
        ..Default::default()
    };
    let mut h = harness("en-US");
    assert_eq!(h.state_mut().restore_session(&session), 1);
    h.run_steps(4);
    let sh = h.state();
    assert!(sh.engine.state.view.sidebar_visible);
    assert_eq!(sh.engine.state.editor.sel.head, top + 3);
    let saved = sh.session();
    assert!(saved.windows[0].sidebar);
    let t = saved.windows[0].top.expect("scrolled");
    assert!(
        (top.saturating_sub(40)..=top + 40).contains(&t),
        "top {t} vs {top}"
    );
}

#[test]
fn emoji_and_symbols_picker_types_the_choice() {
    let mut h = editor_harness("");
    let ctx = h.ctx.clone();
    h.state_mut()
        .invoke(&ctx, "edit.emojiSymbols", &serde_json::json!({}));
    h.run();
    h.get_by_label("Emoji & Symbols");
    h.get_by_label("→").click();
    h.run();
    assert_eq!(text(&h), "→");
    h.state_mut()
        .invoke(&ctx, "edit.emojiSymbols", &serde_json::json!({}));
    h.run();
    h.event(egui::Event::Text("tada".into()));
    h.run();
    h.key_press(egui::Key::Enter);
    h.run();
    assert_eq!(text(&h), "→🎉");
}

#[test]
fn failed_exports_show_the_whole_error() {
    let mut h = editor_harness("# Doc\n");
    let ctx = h.ctx.clone();
    let bad = std::env::temp_dir().join("inkra-no-such-dir/sub/x.html");
    h.state_mut()
        .invoke(&ctx, "file.exportHtml", &serde_json::json!({ "path": bad }));
    for _ in 0..100 {
        h.step();
        if h.query_by_label("Export failed").is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    h.get_by_label("Export failed");
    h.get_by_label("Copy");
    h.key_press(egui::Key::Enter);
    h.run();
    assert!(h.query_by_label("Export failed").is_none());
}

#[test]
fn integrated_title_bar_renames_on_file_name_click() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("t.md");
    std::fs::write(&p, "x\n").unwrap();
    let mut sh = shell("en-US");
    sh.engine
        .state
        .prefs
        .set("appearance.titleBar", serde_json::json!("integrated"));
    sh.engine
        .execute("file.open", &serde_json::json!({ "path": p }))
        .unwrap();
    sh.engine.take_effects();
    let mut h = Harness::builder()
        .with_size(egui::vec2(900.0, 600.0))
        .build_ui_state(|ui, shell: &mut Shell| shell.ui(ui), sh);
    h.run();
    h.get_by_label("Minimize");
    // The title "t.md - <folder> - Inkra" is centered; its file name starts it.
    let title = format!(
        "t.md - {} - Inkra",
        d.path().file_name().unwrap().to_string_lossy()
    );
    let w = h.ctx.fonts_mut(|f| {
        f.layout_no_wrap(
            title.clone(),
            egui::FontId::proportional(13.0),
            egui::Color32::WHITE,
        )
        .size()
        .x
    });
    let x = 450.0 - w / 2.0 + 8.0;
    h.event(egui::Event::PointerMoved(egui::pos2(x, 15.0)));
    h.event(egui::Event::PointerButton {
        pos: egui::pos2(x, 15.0),
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: Default::default(),
    });
    h.event(egui::Event::PointerButton {
        pos: egui::pos2(x, 15.0),
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: Default::default(),
    });
    h.run();
    assert!(
        h.state().rename_open(),
        "clicking the file name opens the rename dialog"
    );
}

#[test]
fn reloading_the_window_applies_the_language() {
    let mut h = harness("en-US");
    h.state_mut()
        .engine
        .state
        .prefs
        .set("general.language", serde_json::json!("zh-CN"));
    let ctx = h.ctx.clone();
    h.state_mut().reload_window(&ctx);
    h.run();
    h.get_by_label("文件");
}

#[test]
fn closing_the_last_window_can_keep_the_app_running() {
    let mut sh = shell("en-US");
    sh.engine
        .state
        .prefs
        .set("general.lastWindowClosed", serde_json::json!("keep"));
    sh.engine.state.editor = inkra_engine::editor::Editor::from_text("saved text");
    let mut h = Harness::builder()
        .with_size(egui::vec2(900.0, 600.0))
        .build_ui_state(|ui, shell: &mut Shell| shell.ui(ui), sh);
    h.run();
    let ctx = h.ctx.clone();
    h.state_mut()
        .invoke(&ctx, "file.close", &serde_json::json!({}));
    h.run_steps(3);
    assert!(!h.state().quit_requested());
    assert_eq!(text(&h), "");
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

// ---- M3 acceptance cases (SPEC 13, docs/acceptance/m3.md) ----

fn live_harness(content: &str) -> Harness<'static, Shell> {
    let mut h = editor_harness(content);
    h.state_mut().engine.state.editor.mode = inkra_engine::editor::ViewMode::Live;
    h.run();
    h
}

fn key(h: &mut Harness<'_, Shell>, k: egui::Key) {
    h.key_press(k);
    h.step();
}

fn rendered(h: &Harness<'_, Shell>) -> Vec<String> {
    h.state()
        .editor_view()
        .map(|v| v.rendered_text())
        .unwrap_or_default()
}

#[test]
fn m3_case1_heading_marker_shows_only_in_active_heading() {
    let mut h = live_harness("");
    type_str(&mut h, "# Hello");
    key(&mut h, egui::Key::Enter);
    type_str(&mut h, "World");
    h.run();
    assert_eq!(text(&h), "# Hello\n\nWorld");
    let blocks = h.state().engine.state.editor.doc.ast().to_blocks();
    assert!(matches!(
        blocks[0].kind,
        inkra_markdown::BlockKind::Heading { level: 1, .. }
    ));
    assert!(matches!(
        blocks[1].kind,
        inkra_markdown::BlockKind::Paragraph
    ));
    let r = rendered(&h);
    assert!(
        r.iter().any(|t| t == "Hello"),
        "marker hidden while the caret is in the paragraph: {r:?}"
    );
    // back into the heading
    h.state_mut().engine.state.editor.sel = inkra_doc::Selection::caret(4);
    h.run();
    let r = rendered(&h);
    assert!(
        r.iter().any(|t| t == "# Hello"),
        "marker shown in the active heading: {r:?}"
    );
}

#[test]
fn m3_case2_bold_markers_follow_the_caret() {
    let mut h = live_harness("");
    type_str(&mut h, "**bold** text");
    h.run();
    // caret at the end, outside the bold node
    let r = rendered(&h);
    assert!(r.iter().any(|t| t == "bold text"), "{r:?}");
    h.state_mut().engine.state.editor.sel = inkra_doc::Selection::caret(4);
    h.run();
    let r = rendered(&h);
    assert!(r.iter().any(|t| t == "**bold** text"), "{r:?}");
}

#[test]
fn m3_case3_nested_list_then_paragraph() {
    let mut h = live_harness("");
    type_str(&mut h, "- a");
    key(&mut h, egui::Key::Enter);
    type_str(&mut h, "b");
    key(&mut h, egui::Key::Tab);
    type_str(&mut h, "c");
    key(&mut h, egui::Key::Enter);
    key(&mut h, egui::Key::Enter);
    key(&mut h, egui::Key::Enter);
    type_str(&mut h, "para");
    h.run();
    assert_eq!(text(&h), "- a\n  - bc\n\npara");
    let blocks = h.state().engine.state.editor.doc.ast().to_blocks();
    assert_eq!(blocks.len(), 2);
    assert!(matches!(
        blocks[1].kind,
        inkra_markdown::BlockKind::Paragraph
    ));
}

#[test]
fn m3_case4_code_block_highlight_and_leaving_with_down() {
    inkra_highlight::load_now();
    let mut h = live_harness("");
    type_str(&mut h, "```rust");
    key(&mut h, egui::Key::Enter);
    let code: String = (0..50).map(|i| format!("let x{i} = {i}; // n\n")).collect();
    h.event(egui::Event::Paste(code.trim_end().to_owned()));
    h.run();
    let t = text(&h);
    assert!(t.starts_with("```rust\nlet x0 = 0;"), "{t:?}");
    assert!(t.ends_with("// n\n```"), "{t:?}");
    let pos = 10;
    h.state_mut().engine.state.editor.sel = inkra_doc::Selection::caret(pos);
    h.run();
    assert!(
        h.state().editor_view().unwrap().colors_at(pos) >= 3,
        "highlighted"
    );
    // to the last code line, then Down leaves the block (a paragraph is created)
    let last = t.len() - 4;
    h.state_mut().engine.state.editor.sel = inkra_doc::Selection::caret(last);
    h.run();
    key(&mut h, egui::Key::ArrowDown);
    h.run();
    let head = h.state().engine.state.editor.sel.head;
    assert!(
        head >= t.len(),
        "caret left the code block: {head} vs {}",
        t.len()
    );
}

#[test]
fn m3_case7_save_changes_only_the_edited_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("doc.md");
    let original = "# Title\r\n\r\nSome  text with trailing spaces   \r\n\r\n- item\r\n\r\n```\r\ncode\r\n```\r\n";
    std::fs::write(&p, original).unwrap();
    let mut h = live_harness("");
    h.state_mut()
        .engine
        .execute("file.open", &serde_json::json!({ "path": p }))
        .unwrap();
    h.state_mut().engine.take_effects();
    h.run();
    let at = original.find("item").unwrap() + 4;
    h.state_mut().engine.state.editor.sel = inkra_doc::Selection::caret(at);
    type_str(&mut h, "!");
    h.state_mut()
        .engine
        .execute("file.save", &serde_json::Value::Null)
        .unwrap();
    let saved = std::fs::read_to_string(&p).unwrap();
    let mut expected = original.to_owned();
    expected.insert(at, '!');
    assert_eq!(saved, expected, "only the edit differs");
}

#[test]
fn m3_case9_closing_a_modified_window_asks() {
    let mut h = live_harness("");
    type_str(&mut h, "unsaved");
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::W);
    h.run();
    assert!(h.state().confirm_pending());
    h.get_by_label("Save");
    h.get_by_label("Don’t Save");
    h.get_by_label("Cancel").click();
    h.run();
    assert!(!h.state().confirm_pending());
    assert_eq!(text(&h), "unsaved");
}

#[test]
fn m3_case6_scrolling_a_large_document_is_cheap() {
    let unit = "## 标题\n\n这是一段用于性能测试的中文段落，包含 **粗体**、*斜体* 和 `code`。Some English words too.\n\n- 列表项一\n- 列表项二\n\n";
    let mut doc = String::new();
    while doc.chars().count() < 100_000 {
        doc.push_str(unit);
    }
    let mut h = live_harness(&doc);
    let mut times = Vec::new();
    for _ in 0..60 {
        h.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, -120.0),
            modifiers: Default::default(),
            phase: egui::TouchPhase::Move,
        });
        let t = std::time::Instant::now();
        h.step();
        times.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    times.sort_by(f64::total_cmp);
    let p95 = times[times.len() * 95 / 100 - 1];
    eprintln!(
        "scroll frame (CPU, headless): p50 {:.2} ms, p95 {p95:.2} ms",
        times[times.len() / 2]
    );
    if !cfg!(debug_assertions) {
        assert!(p95 < 16.0, "p95 {p95}");
    }
}

// ---- M4 acceptance cases (docs/acceptance/m4.md) ----

fn cmd_shift() -> egui::Modifiers {
    egui::Modifiers {
        command: true,
        ctrl: !cfg!(target_os = "macos"),
        shift: true,
        ..Default::default()
    }
}

fn cmd() -> egui::Modifiers {
    egui::Modifiers {
        command: true,
        ctrl: !cfg!(target_os = "macos"),
        ..Default::default()
    }
}

#[test]
fn m4_table_dialog_tab_rows_align_sort() {
    let mut h = live_harness("");
    h.key_press_modifiers(cmd(), egui::Key::T);
    h.run();
    h.get_by_label("OK").click();
    h.run();
    let t = text(&h);
    assert_eq!(
        t.lines().count(),
        5,
        "3x3: header, delimiter, 3 rows: {t:?}"
    );
    // Header cell is selected → type over it, Tab to the next cell.
    let first = t.find("Column 1").unwrap();
    h.state_mut().engine.state.editor.sel =
        inkra_doc::Selection::new(first, first + "Column 1".len());
    type_str(&mut h, "Name");
    key(&mut h, egui::Key::Tab);
    type_str(&mut h, "Qty");
    key(&mut h, egui::Key::Tab);
    type_str(&mut h, "Price");
    for v in ["b", "2", "x", "a", "10", "y", "c", "1", "z"] {
        key(&mut h, egui::Key::Tab);
        type_str(&mut h, v);
    }
    // Tab in the last cell adds a row; Ctrl+Enter too.
    key(&mut h, egui::Key::Tab);
    h.key_press_modifiers(cmd(), egui::Key::Enter);
    h.run();
    let t = text(&h);
    assert_eq!(t.lines().count(), 7, "{t}");
    // Sort by Qty (numeric) and align it right.
    let at = t.find("| 2").unwrap() + 2;
    h.state_mut().engine.state.editor.sel = inkra_doc::Selection::caret(at);
    h.state_mut()
        .engine
        .execute("table.sortByColumnAsc", &serde_json::Value::Null)
        .unwrap();
    h.state_mut()
        .engine
        .execute("table.alignRight", &serde_json::Value::Null)
        .unwrap();
    h.run();
    let t = text(&h);
    let qty: Vec<&str> = t
        .lines()
        .skip(2)
        .map(|l| l.split('|').nth(2).unwrap().trim())
        .filter(|s| !s.is_empty())
        .collect();
    assert_eq!(qty, ["1", "2", "10"], "{t}");
    assert!(t.lines().nth(1).unwrap().contains("| --: |"), "{t}");
    // Every row has its pipes at the same columns (ED-085).
    let widths: Vec<usize> = t.lines().map(|l| l.chars().count()).collect();
    assert!(widths.windows(2).all(|w| w[0] == w[1]), "{t}");
}

#[test]
fn m4_outline_click_jumps_to_heading() {
    let mut doc = String::from("# Top\n\n");
    for i in 0..60 {
        doc.push_str(&format!("para {i}\n\n"));
    }
    doc.push_str("## Far Away\n\n");
    for i in 0..40 {
        doc.push_str(&format!("after {i}\n\n"));
    }
    let mut h = live_harness(&doc);
    h.key_press_modifiers(cmd_shift(), egui::Key::Num1);
    h.run();
    h.get_by_label("Far Away").click();
    h.run();
    let at = doc.find("## Far Away").unwrap();
    assert_eq!(
        h.state().engine.state.editor.sel.head,
        at + "## Far Away".len()
    );
    let top = h.state().editor_view().unwrap().viewport_top();
    assert_eq!(top, at, "heading at the top of the viewport");
}

#[test]
fn m4_quick_open_and_file_tree_move() {
    let d = tempfile::tempdir().unwrap();
    let r = inkra_workspace::paths::canonical(d.path());
    std::fs::create_dir_all(r.join("notes/deep")).unwrap();
    for i in 0..300 {
        std::fs::write(r.join(format!("notes/n{i}.md")), format!("# {i}\n")).unwrap();
    }
    std::fs::write(r.join("notes/deep/target-file.md"), "# Target\n").unwrap();
    let mut h = live_harness("");
    h.state_mut()
        .engine
        .execute("sidebar.setRoot", &serde_json::json!({ "path": r }))
        .unwrap();
    let eff = h.state_mut().engine.take_effects();
    h.state_mut().apply_startup_effects(eff);
    h.run();
    // wait for the background index
    for _ in 0..200 {
        h.step();
        if h.state().index_ready() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    h.key_press_modifiers(cmd(), egui::Key::P);
    h.run();
    type_str(&mut h, "trgtfl");
    h.run();
    assert!(h.state().quick_open_ms() < 50.0);
    key(&mut h, egui::Key::Enter);
    h.run();
    let opened = h.state().engine.state.editor.path.clone();
    assert_eq!(
        opened.as_deref(),
        Some(r.join("notes/deep/target-file.md").as_path()),
        "blank window reused"
    );
    // Move it to the root through the sidebar command; the open document follows.
    h.state_mut()
        .engine
        .execute(
            "sidebar.move",
            &serde_json::json!({ "path": r.join("notes/deep/target-file.md"), "dest": r }),
        )
        .unwrap();
    assert!(r.join("target-file.md").exists());
    assert_eq!(
        h.state().engine.state.editor.path.as_deref(),
        Some(r.join("target-file.md").as_path())
    );
    // The file tree shows it.
    h.key_press_modifiers(cmd_shift(), egui::Key::Num3);
    for _ in 0..40 {
        h.step();
    }
    h.get_by_label("target-file.md");
}

#[test]
fn m5_toc_and_footnotes_render() {
    let doc =
        "# Notes\n\n[TOC]\n\n## First\n\nText[^b] and[^a].\n\n## Second\n\n[^a]: A.\n[^b]: B.\n";
    let mut h = live_harness(doc);
    h.state_mut().engine.state.editor.sel = inkra_doc::Selection::caret(0);
    for _ in 0..4 {
        h.step();
    }
    assert!(
        h.state().engine.state.editor.sel.head <= 2,
        "the caret stays in the heading"
    );
    let r = rendered(&h);
    assert!(
        r.iter()
            .any(|t| t.contains("First") && t.contains("Second") && t.contains("Notes")),
        "TOC lists headings: {r:?}"
    );
    assert!(
        r.iter().any(|t| t.contains("Text1 and2")),
        "footnotes numbered by first reference: {r:?}"
    );
}

#[test]
fn theme_numbers_headings_without_writing_them() {
    let doc = "# Intro\n\ntext\n\n## Part\n\n## Next\n\n# End\n";
    let mut h = live_harness(doc);
    {
        let st = &mut h.state_mut().engine.state;
        st.style.heading_numbers = true;
        st.style.version += 2;
        st.editor.sel = inkra_doc::Selection::caret(doc.len());
    }
    for _ in 0..4 {
        h.step();
    }
    let r = rendered(&h);
    for n in ["1. Intro", "1.1. Part", "1.2. Next", "2. End"] {
        assert!(r.iter().any(|t| t.contains(n)), "{n} in {r:?}");
    }
    assert_eq!(h.state().engine.state.editor.doc.text().to_string(), doc);
}

#[test]
fn m5_opened_file_keeps_caret_at_start() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("fn.md");
    std::fs::write(
        &p,
        "# Notes and Contents\n\n[TOC]\n\n## First section\n\nText[^b].\n\n[^b]: B.\n",
    )
    .unwrap();
    let mut h = harness("en-US");
    h.state_mut()
        .engine
        .execute("file.open", &serde_json::json!({ "path": p }))
        .unwrap();
    let eff = h.state_mut().engine.take_effects();
    h.state_mut().apply_startup_effects(eff);
    for _ in 0..4 {
        h.step();
    }
    assert!(
        h.state().engine.state.editor.sel.head <= 2,
        "{}",
        h.state().engine.state.editor.sel.head
    );
}

#[test]
fn every_command_has_a_title_in_both_languages() {
    let engine = Engine::new(Default::default());
    for lang in ["en-US", "zh-CN"] {
        let i18n = I18n::new(lang);
        let missing: Vec<&str> = engine
            .registry
            .iter()
            .map(|c| c.spec().id)
            .filter(|id| !i18n.has(&inkra_engine::command::title_key(id)))
            .collect();
        assert!(missing.is_empty(), "{lang}: {missing:?}");
    }
}

#[test]
fn preferences_window_changes_settings() {
    let mut h = harness("en-US");
    h.get_by_label("File").click();
    h.run();
    h.get_by_label_contains("Preferences").click();
    h.run();
    h.get_by_label("Save automatically").click();
    h.run();
    assert!(h.state().engine.state.prefs.bool("general.autoSave"));
    h.get_by_label("Editor").click();
    h.run();
    h.get_by_label("Auto-pair Markdown symbols").click();
    h.run();
    assert!(h.state().engine.state.prefs.bool("editor.autoPairMarkdown"));
    assert!(
        h.state().engine.state.editor.prefs.pair_markdown,
        "applied to the open document"
    );
}

#[test]
fn m7_rename_and_properties_of_the_current_file() {
    let d = tempfile::tempdir().unwrap();
    let r = inkra_workspace::paths::canonical(d.path());
    std::fs::write(r.join("old.md"), "# Hello\n\nsome words here\n").unwrap();
    let mut h = harness("en-US");
    h.state_mut()
        .engine
        .execute(
            "file.open",
            &serde_json::json!({ "path": r.join("old.md") }),
        )
        .unwrap();
    h.run();
    h.get_by_label("File").click();
    h.run();
    h.get_by_label_contains("Rename").click();
    h.run();
    assert_eq!(
        h.state().rename.as_deref(),
        Some("old.md"),
        "the dialog starts with the name"
    );
    h.state_mut().rename = Some("new.md".into());
    h.run();
    h.get_by_label("OK").click();
    h.run();
    assert!(r.join("new.md").is_file() && !r.join("old.md").exists());
    assert_eq!(
        h.state().engine.state.editor.path.as_deref(),
        Some(r.join("new.md").as_path())
    );
    h.get_by_label("File").click();
    h.run();
    h.get_by_label_contains("Properties").click();
    h.run();
    h.get_by_label_contains("new.md");
    h.get_by_label("Words");
}

#[test]
fn m7_document_is_exposed_to_assistive_technology() {
    let h = live_harness("# Hello\n\nworld\n");
    let node = h.get_by_role(egui::accesskit::Role::MultilineTextInput);
    assert_eq!(
        node.accesskit_node().value().as_deref(),
        Some("# Hello\n\nworld\n")
    );
    assert!(
        node.accesskit_node().description().is_some(),
        "caret line and column"
    );
    // Visible blocks are child nodes with roles and the displayed text.
    let heading = h.get_by_role(egui::accesskit::Role::Heading);
    assert_eq!(heading.accesskit_node().level(), Some(1));
    let para = h.get_by_role(egui::accesskit::Role::Paragraph);
    assert_eq!(para.accesskit_node().value().as_deref(), Some("world"));
}

/// CM-003 end to end: a TCP client drives the shell through the control
/// channel while the harness runs frames.
#[test]
fn control_channel_drives_the_shell() {
    use std::io::{BufRead, BufReader, Write};
    let d = tempfile::tempdir().unwrap();
    let mut sh = shell("en-US");
    sh.engine.state.paths = Some(inkra_workspace::Paths::rooted(d.path()));
    let mut h = Harness::builder()
        .with_size(egui::vec2(900.0, 600.0))
        .build_ui_state(|ui, shell: &mut Shell| shell.ui(ui), sh);
    h.run();
    let ctx = h.ctx.clone();
    let port = h.state_mut().start_control(&ctx, 0).unwrap();
    let dir = inkra_engine::control::control_dir(&d.path().join("data"));
    let info = inkra_engine::control::ConnectionInfo::list(&dir)[0]
        .1
        .clone();
    let doc = d.path().join("note.md");
    std::fs::write(&doc, "# Note\n\nbody\n").unwrap();
    let doc2 = doc.clone();
    let client = std::thread::spawn(move || {
        let mut c = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        let mut r = BufReader::new(c.try_clone().unwrap());
        let mut call = |id: u64, method: &str, params: serde_json::Value| {
            writeln!(c, "{}", inkra_engine::control::request(id, method, &params)).unwrap();
            let mut line = String::new();
            r.read_line(&mut line).unwrap();
            inkra_engine::control::parse_response(&line)
        };
        let hello = call(1, "hello", serde_json::json!({ "token": info.token })).unwrap();
        assert_eq!(hello["version"], 1);
        call(2, "open", serde_json::json!({ "path": doc2 })).unwrap();
        call(
            3,
            "execute",
            serde_json::json!({ "command": "edit.insertText", "params": { "text": "X" } }),
        )
        .unwrap();
        // Dialog commands answer with an error instead of opening a dialog.
        let e = call(
            4,
            "execute",
            serde_json::json!({ "command": "file.saveAs" }),
        )
        .unwrap_err();
        assert_eq!(e.code, inkra_engine::control::code::FAILED);
        call(5, "key", serde_json::json!({ "text": "Y" })).unwrap();
        let v = call(
            6,
            "inspect_document",
            serde_json::json!({ "include": ["text"] }),
        )
        .unwrap();
        call(7, "save", serde_json::json!({})).unwrap();
        let unknown = call(8, "bogus", serde_json::json!({})).unwrap_err();
        assert_eq!(unknown.code, inkra_engine::control::code::METHOD_NOT_FOUND);
        v["text"].as_str().unwrap().to_owned()
    });
    // Frames until the client is done; injected input goes in through the
    // same hook the app uses (`raw_input_hook`).
    for _ in 0..500 {
        let mut raw = egui::RawInput::default();
        h.state_mut().control_raw_input(&mut raw);
        for e in raw.events {
            h.event(e);
        }
        h.step();
        if client.is_finished() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let text = client.join().unwrap();
    assert!(text.contains('X') && text.contains('Y'), "{text:?}");
    let saved = std::fs::read_to_string(&doc).unwrap();
    assert!(saved.contains('X') && saved.contains('Y'), "{saved:?}");
    assert_ne!(saved, "# Note\n\nbody\n");
}

/// T8 / AD-5: typing inside a large diagram in a 100k-character document
/// stays responsive: the diagram is laid out off the UI thread.
#[test]
fn typing_in_a_large_diagram_stays_responsive() {
    let mut diagram = String::from("```mermaid\nflowchart TD\n");
    let mut x: u64 = 7;
    for i in 0..160 {
        for _ in 0..3 {
            x = x
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            diagram.push_str(&format!("  n{i} -.-> n{}\n", (x >> 33) % 160));
        }
    }
    diagram.push_str("```\n\n");
    let t = std::time::Instant::now();
    let pal = inkra_diagram::Palette::light();
    let _ = inkra_diagram::render_uncached(
        diagram
            .trim_start_matches("```mermaid\n")
            .trim_end_matches("```\n\n"),
        &pal,
    );
    let sync_ms = t.elapsed().as_secs_f64() * 1000.0;
    let unit =
        "这是一段用于性能测试的中文段落，包含 **粗体** 和 `code`。Some English words too.\n\n";
    let mut doc = String::new();
    while doc.chars().count() < 50_000 {
        doc.push_str(unit);
    }
    let at = doc.len() + "```mermaid\nflowchart TD\n".len();
    doc.push_str(&diagram);
    while doc.chars().count() < 100_000 {
        doc.push_str(unit);
    }
    let mut h = editor_harness(&doc);
    // The caret inside the diagram: its source is shown with a live preview,
    // and every keystroke changes the diagram.
    h.state_mut().engine.state.editor.sel = inkra_doc::Selection::caret(at);
    h.run_steps(5);
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
        "typing in a diagram: p50 {:.2} ms, p95 {p95:.2} ms (one synchronous layout: {sync_ms:.0} ms)",
        times[times.len() / 2]
    );
    assert!(h.state().engine.state.editor.text().contains("n0 -.->"));
    if !cfg!(debug_assertions) {
        assert!(p95 < 16.0, "p95 {p95} ms");
    }
}
