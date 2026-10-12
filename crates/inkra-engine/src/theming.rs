//! Theme commands (TH-002, TH-003).

use serde_json::Value;

use crate::command::{
    Category, CommandError, CommandRegistry, CommandSpec, FnCommand, Outcome, spec,
};
use crate::state::Effect;

fn params(mut s: CommandSpec, p: &'static str) -> CommandSpec {
    s.params = Some(p);
    s
}

/// Registers theme commands.
pub fn register(r: &mut CommandRegistry) {
    use Category::Theme;
    r.register(FnCommand::new(
        params(
            spec(
                "theme.select",
                Theme,
                "TH-003",
                "Switch to a theme (instantly, no restart).",
            ),
            r#"{"type":"object","required":["name"],"properties":{"name":{"type":"string"}}}"#,
        ),
        |cx, p| {
            let name = p
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            let dir = cx.app.paths.as_ref().map(|p| p.themes_dir());
            crate::theme::load(&name, dir.as_deref()).map_err(|msg| CommandError::Failed {
                id: "theme.select".into(),
                msg,
            })?;
            // Choosing a theme explicitly also sets the Auto slot of its brightness.
            let dark = crate::theme::list(dir.as_deref())
                .iter()
                .any(|e| e.name == name && e.dark);
            cx.app
                .set_pref("appearance.theme", Value::from(name.clone()));
            cx.app.set_pref(
                if dark {
                    "appearance.darkTheme"
                } else {
                    "appearance.lightTheme"
                },
                Value::from(name),
            );
            cx.app.set_pref("appearance.themeAuto", Value::from(false));
            let _ = cx.app.reload_theme();
            Ok(Outcome::Done)
        },
    ));
    r.register(
        FnCommand::new(
            spec(
                "theme.auto",
                Theme,
                "TH-002",
                "Follow the system light/dark appearance.",
            ),
            |cx, _| {
                let on = !cx.app.prefs.bool("appearance.themeAuto");
                cx.app.set_pref("appearance.themeAuto", Value::from(on));
                let _ = cx.app.reload_theme();
                Ok(Outcome::Done)
            },
        )
        .checked(|cx| Some(cx.app.prefs.bool("appearance.themeAuto"))),
    );
    r.register(
        FnCommand::new(
            spec(
                "theme.openFolder",
                Theme,
                "TH-003",
                "Open the user theme folder.",
            ),
            |cx, _| {
                if let Some(p) = &cx.app.paths {
                    let dir = p.themes_dir();
                    let _ = std::fs::create_dir_all(&dir);
                    cx.effects
                        .push(Effect::OpenUrl(dir.to_string_lossy().into_owned()));
                }
                Ok(Outcome::Done)
            },
        )
        .enabled(|cx| cx.app.paths.is_some()),
    );
    r.register(FnCommand::new(
        spec(
            "theme.reload",
            Theme,
            "TH-003",
            "Reload the current theme from disk.",
        ),
        |cx, _| {
            cx.app.reload_theme().map_err(|msg| CommandError::Failed {
                id: "theme.reload".into(),
                msg,
            })?;
            Ok(Outcome::Done)
        },
    ));
}
