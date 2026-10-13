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
