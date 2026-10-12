//! Built-in command definitions.

use crate::command::{Category, CommandRegistry, FnCommand, Outcome, spec};
use crate::state::Effect;

/// Registers all built-in commands.
pub fn register_all(r: &mut CommandRegistry) {
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
