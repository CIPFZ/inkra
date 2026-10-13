//! Built-in command definitions.

use crate::command::{Category, CommandRegistry, FnCommand, Outcome, spec};
use crate::state::Effect;

/// Registers all built-in commands.
pub fn register_all(r: &mut CommandRegistry) {
    r.register(FnCommand::new(
        {
            let mut s = spec("app.setPreference", Category::App, "PR-001", "Set a preference (validated against the preference table) and save.");
            s.params = Some(r#"{"type":"object","required":["key","value"],"properties":{"key":{"type":"string"},"value":{}}}"#);
            s
        },
        |cx, p| {
            let key = p.get("key").and_then(serde_json::Value::as_str).unwrap_or_default().to_owned();
            let value = p.get("value").cloned().unwrap_or(serde_json::Value::Null);
            if !cx.app.set_pref(&key, value) {
                return Err(crate::command::CommandError::Failed { id: "app.setPreference".into(), msg: format!("invalid value for `{key}`") });
            }
            const STYLE_KEYS: &[&str] = &[
                "markdown.mathNumbering",
                "markdown.diagrams",
                "markdown.tocDepth",
                "editor.spellcheck",
                "markdown.codeWrap",
                "markdown.codeLineNumbers",
                "editor.firstLineIndent",
                "markdown.preserveBlankLines",
                "image.showTitle",
                "image.maxWidthFollowsEditor",
            ];
            if key.starts_with("appearance.") || STYLE_KEYS.contains(&key.as_str()) {
                let _ = cx.app.reload_theme();
            }
            if key == "general.markdownExtensions" {
                inkra_workspace::tree::set_markdown_exts(&cx.app.prefs.strings(&key));
                cx.effects.push(Effect::RootChanged);
            }
            if key.starts_with("markdown.") || key.starts_with("editor.") {
                // Parse options and editing preferences apply to every open
                // document right away (MD-080).
                let o = crate::markdown_options(&cx.app.prefs);
                let ep = crate::edit_prefs(&cx.app.prefs);
                cx.app.editor.doc.set_options(o.clone());
                cx.app.editor.prefs = ep.clone();
                for w in &mut cx.app.others {
                    w.editor.doc.set_options(o.clone());
                    w.editor.prefs = ep.clone();
                }
            }
            Ok(Outcome::Done)
        },
    ));
    use crate::help::Topic;
    r.register(FnCommand::new(
        spec(
            "help.quickStart",
            Category::Help,
            "UI-052",
            "Open the Quick Start guide.",
        ),
        |cx, _| crate::help::open(cx, Topic::QuickStart),
    ));
    r.register(FnCommand::new(
        spec(
            "help.markdownReference",
            Category::Help,
            "UI-052",
            "Open the Markdown reference.",
        ),
        |cx, _| crate::help::open(cx, Topic::MarkdownReference),
    ));
    r.register(FnCommand::new(
        spec(
            "help.customThemes",
            Category::Help,
            "TH-005",
            "Open the guide to writing themes.",
        ),
        |cx, _| crate::help::open(cx, Topic::Themes),
    ));
    r.register(FnCommand::new(
        spec(
            "help.shortcuts",
            Category::Help,
            "UI-020",
            "Open the list of commands and keyboard shortcuts.",
        ),
        |cx, _| crate::help::open(cx, Topic::Shortcuts),
    ));
    r.register(FnCommand::new(
        spec(
            "help.changelog",
            Category::Help,
            "UI-052",
            "Open the change log.",
        ),
        |cx, _| crate::help::open(cx, Topic::Changelog),
    ));
    r.register(FnCommand::new(
        spec(
            "help.reportIssue",
            Category::Help,
            "UI-052",
            "Open the issue tracker in the browser.",
        ),
        |cx, _| {
            cx.effects.push(Effect::OpenUrl(format!(
                "{}/issues/new",
                env!("CARGO_PKG_REPOSITORY")
            )));
            Ok(Outcome::Done)
        },
    ));
    r.register(FnCommand::new(
        spec(
            "app.preferences",
            Category::App,
            "PR-001",
            "Open the Preferences window.",
        ),
        |cx, _| {
            cx.effects.push(Effect::ShowPreferences);
            Ok(Outcome::Done)
        },
    ));
    r.register(FnCommand::new(
        {
            let mut s = spec(
                "app.exportSettings",
                Category::App,
                "PR-002",
                "Write the settings to a JSON file (asks where unless `path` is given).",
            );
            s.params = Some(r#"{"type":"object","properties":{"path":{"type":"string"}}}"#);
            s
        },
        |cx, p| {
            let Some(path) = p.get("path").and_then(serde_json::Value::as_str) else {
                cx.effects.push(Effect::SettingsDialog { import: false });
                return Ok(Outcome::Done);
            };
            let text =
                serde_json::to_string_pretty(cx.app.prefs.raw()).unwrap_or_else(|_| "{}".into());
            std::fs::write(path, format!("{text}\n")).map_err(|e| {
                crate::command::CommandError::Failed {
                    id: "app.exportSettings".into(),
                    msg: e.to_string(),
                }
            })?;
            Ok(Outcome::Done)
        },
    ));
    r.register(FnCommand::new(
        {
            let mut s = spec(
                "app.importSettings",
                Category::App,
                "PR-002",
                "Merge settings from a JSON file (asks for it unless `path` is given).",
            );
            s.params = Some(r#"{"type":"object","properties":{"path":{"type":"string"}}}"#);
            s
        },
        |cx, p| {
            let Some(path) = p.get("path").and_then(serde_json::Value::as_str) else {
                cx.effects.push(Effect::SettingsDialog { import: true });
                return Ok(Outcome::Done);
            };
            let fail = |msg: String| crate::command::CommandError::Failed {
                id: "app.importSettings".into(),
                msg,
            };
            let text = std::fs::read_to_string(path).map_err(|e| fail(e.to_string()))?;
            let rejected = cx.app.prefs.import(&text).map_err(fail)?;
            for k in rejected {
                log::warn!("imported settings: invalid value for `{k}`");
            }
            cx.app.save_prefs();
            cx.app.apply_prefs();
            cx.effects.push(Effect::RootChanged);
            Ok(Outcome::Done)
        },
    ));
    r.register(FnCommand::new(
        spec(
            "app.resetSettings",
            Category::App,
            "PR-021",
            "Reset every setting to its default.",
        ),
        |cx, _| {
            cx.app.prefs.reset();
            cx.app.save_prefs();
            cx.app.apply_prefs();
            cx.effects.push(Effect::RootChanged);
            Ok(Outcome::Done)
        },
    ));
    r.register(FnCommand::new(
        {
            let mut s = spec("app.openFolder", Category::App, "PR-021", "Open the configuration, themes, keybindings or logs location in the file manager.");
            s.params = Some(r#"{"type":"object","required":["which"],"properties":{"which":{"enum":["config","themes","keybindings","logs"]}}}"#);
            s
        },
        |cx, p| {
            let Some(paths) = cx.app.paths.clone() else { return Ok(Outcome::Done) };
            let target = match p.get("which").and_then(serde_json::Value::as_str).unwrap_or("config") {
                "themes" => paths.themes_dir(),
                "keybindings" => {
                    let f = paths.keybindings_file();
                    if !f.exists() {
                        let _ = std::fs::create_dir_all(&paths.config);
                        let _ = std::fs::write(&f, "[]\n");
                    }
                    f
                }
                "logs" => paths.logs_dir(),
                _ => paths.config.clone(),
            };
            if target.extension().is_none() {
                let _ = std::fs::create_dir_all(&target);
            }
            cx.effects.push(Effect::OpenUrl(target.to_string_lossy().into_owned()));
            Ok(Outcome::Done)
        },
    ));
    r.register(FnCommand::new(
        spec(
            "help.checkUpdates",
            Category::Help,
            "PR-020",
            "Check GitHub Releases for a newer version (nothing is downloaded).",
        ),
        |cx, _| {
            cx.effects.push(Effect::CheckUpdates { manual: true });
            Ok(Outcome::Done)
        },
    ));
    r.register(FnCommand::new(
        spec(
            "app.quit",
            Category::App,
            "FS-005",
            "Quit the application, closing all windows.",
        ),
        |cx, _| {
            cx.effects.push(Effect::Quit);
            Ok(Outcome::Done)
        },
    ));
    r.register(FnCommand::new(
        spec(
            "app.about",
            Category::App,
            "UI-052",
            "Show the About dialog.",
        ),
        |cx, _| {
            cx.effects.push(Effect::ShowAbout);
            Ok(Outcome::Done)
        },
    ));
    r.register(
        FnCommand::new(
            spec(
                "view.toggleSidebar",
                Category::View,
                "FS-040",
                "Show or hide the sidebar.",
            ),
            |cx, _| {
                cx.app.view.sidebar_visible = !cx.app.view.sidebar_visible;
                Ok(Outcome::Done)
            },
        )
        .checked(|cx| Some(cx.app.view.sidebar_visible)),
    );
    r.register(
        FnCommand::new(
            spec(
                "view.debugOverlay",
                Category::View,
                "FS-040",
                "Show the performance overlay (frame and input latency).",
            ),
            |cx, _| {
                cx.app.view.debug_overlay = !cx.app.view.debug_overlay;
                Ok(Outcome::Done)
            },
        )
        .checked(|cx| Some(cx.app.view.debug_overlay)),
    );
}

#[cfg(test)]
mod settings_tests {
    use serde_json::json;

    #[test]
    fn export_import_and_reset_settings() {
        let d = tempfile::tempdir().unwrap();
        let file = d.path().join("s.json");
        let mut e = crate::Engine::new(crate::Preferences::default());
        e.execute(
            "app.setPreference",
            &json!({ "key": "editor.tabWidth", "value": 2 }),
        )
        .unwrap();
        e.execute("app.exportSettings", &json!({ "path": file }))
            .unwrap();
        e.execute("app.resetSettings", &json!({})).unwrap();
        assert_eq!(e.state.prefs.f64("editor.tabWidth"), 4.0);
        e.execute("app.importSettings", &json!({ "path": file }))
            .unwrap();
        assert_eq!(e.state.prefs.f64("editor.tabWidth"), 2.0);
        assert_eq!(
            e.state.editor.prefs.tab_width, 2,
            "applied to the open document"
        );
    }
}
