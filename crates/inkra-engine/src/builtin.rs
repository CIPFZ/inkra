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
            if key.starts_with("appearance.") || key == "markdown.mathNumbering" || key == "markdown.diagrams" || key == "markdown.tocDepth" || key == "editor.spellcheck" {
                let _ = cx.app.reload_theme();
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
