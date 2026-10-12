//! Command registry (AD-3, CM-001).
//!
//! Every user-triggerable action is a [`Command`] with a stable dotted id. Menus,
//! shortcuts, toolbars, context menus, the CLI and the control channel are only
//! entry points that resolve to command ids and call [`CommandRegistry::execute`].

use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind};

use serde_json::Value;

use crate::state::{CommandContext, Effect};

/// Menu/category a command belongs to; used for grouping in `docs/commands.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Category {
    /// Application-level commands (`app.*`).
    App,
    /// File and window lifecycle (`file.*`).
    File,
    /// Editing, clipboard, selection, find (`edit.*`).
    Edit,
    /// Block-level formatting (`paragraph.*`).
    Paragraph,
    /// Inline formatting (`format.*`).
    Format,
    /// Table editing (`table.*`).
    Table,
    /// Views, panels and modes (`view.*`).
    View,
    /// Themes (`theme.*`).
    Theme,
    /// Searching across files (`search.*`).
    Search,
    /// Help and documentation (`help.*`).
    Help,
}

impl Category {
    /// English name used in generated documentation.
    pub fn name(self) -> &'static str {
        match self {
            Self::App => "App",
            Self::File => "File",
            Self::Edit => "Edit",
            Self::Paragraph => "Paragraph",
            Self::Format => "Format",
            Self::Table => "Table",
            Self::View => "View",
            Self::Theme => "Theme",
            Self::Search => "Search",
            Self::Help => "Help",
        }
    }
}

/// How a command's document edits are grouped on the undo stack (ED-015).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UndoGroup {
    /// The command never edits the document.
    None,
    /// The command's edits form one undo group of their own.
    Own,
    /// Edits merge with adjacent typing (used by text input itself).
    MergeTyping,
}

/// Static description of a command.
#[derive(Debug, Clone)]
pub struct CommandSpec {
    /// Stable dotted lowerCamel id, e.g. `view.toggleSidebar`.
    pub id: &'static str,
    /// Category for grouping.
    pub category: Category,
    /// One-line English description for `docs/commands.md`.
    pub doc: &'static str,
    /// Requirement id(s) implemented by this command, e.g. `UI-001`.
    pub req: &'static str,
    /// JSON Schema of the `params` object, or `None` when it takes no parameters.
    pub params: Option<&'static str>,
    /// Undo grouping behaviour.
    pub undo_group: UndoGroup,
}

impl CommandSpec {
    /// The Fluent message id holding the localized title: `cmd-` + id with `.`
    /// replaced by `-` (e.g. `cmd-view-toggleSidebar`).
    pub fn title_key(&self) -> String {
        title_key(self.id)
    }
}

/// Fluent message id for the title of command `id`.
pub fn title_key(id: &str) -> String {
    format!("cmd-{}", id.replace('.', "-"))
}

/// Successful result of running a command.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum Outcome {
    /// The command ran; nothing to report.
    #[default]
    Done,
    /// The command ran and produced a value (used by inspection commands and the CLI).
    Value(Value),
    /// The command was not applicable in the current state and did nothing.
    Noop,
}

/// Errors returned by [`CommandRegistry::execute`].
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum CommandError {
    /// No command with this id is registered.
    #[error("unknown command `{0}`")]
    Unknown(String),
    /// The command exists but is disabled in the current context.
    #[error("command `{0}` is disabled")]
    Disabled(String),
    /// Parameters did not match the schema.
    #[error("invalid parameters for `{id}`: {msg}")]
    InvalidParams {
        /// Command id.
        id: String,
        /// Human readable reason.
        msg: String,
    },
    /// The command reported a failure.
    #[error("command `{id}` failed: {msg}")]
    Failed {
        /// Command id.
        id: String,
        /// Human readable reason.
        msg: String,
    },
    /// The command panicked; the panic was caught at the command boundary (NF-002).
    #[error("command `{id}` panicked: {msg}")]
    Panicked {
        /// Command id.
        id: String,
        /// Panic payload, if it was a string.
        msg: String,
    },
}

/// Result type of [`Command::run`].
pub type CommandResult = Result<Outcome, CommandError>;

/// An executable command.
pub trait Command: Send + Sync + 'static {
    /// Static description.
    fn spec(&self) -> &CommandSpec;

    /// Whether the command can run in the current context. Disabled commands are
    /// shown greyed out in menus.
    fn enabled(&self, cx: &CommandContext<'_>) -> bool {
        let _ = cx;
        true
    }

    /// `Some(state)` for toggle commands (shown with a check mark in menus).
    fn checked(&self, cx: &CommandContext<'_>) -> Option<bool> {
        let _ = cx;
        None
    }

    /// Runs the command.
    fn run(&self, cx: &mut CommandContext<'_>, params: &Value) -> CommandResult;
}

type EnabledFn = fn(&CommandContext<'_>) -> bool;
type CheckedFn = fn(&CommandContext<'_>) -> Option<bool>;
type RunFn = fn(&mut CommandContext<'_>, &Value) -> CommandResult;

/// A [`Command`] built from plain functions; the common way to define commands.
pub struct FnCommand {
    spec: CommandSpec,
    enabled: Option<EnabledFn>,
    checked: Option<CheckedFn>,
    run: RunFn,
}

impl FnCommand {
    /// Creates a command from its spec and run function.
    pub fn new(spec: CommandSpec, run: RunFn) -> Self {
        Self {
            spec,
            enabled: None,
            checked: None,
            run,
        }
    }

    /// Sets the availability predicate.
    pub fn enabled(mut self, f: EnabledFn) -> Self {
        self.enabled = Some(f);
        self
    }

    /// Sets the toggle-state query.
    pub fn checked(mut self, f: CheckedFn) -> Self {
        self.checked = Some(f);
        self
    }
}

impl Command for FnCommand {
    fn spec(&self) -> &CommandSpec {
        &self.spec
    }
    fn enabled(&self, cx: &CommandContext<'_>) -> bool {
        self.enabled.is_none_or(|f| f(cx))
    }
    fn checked(&self, cx: &CommandContext<'_>) -> Option<bool> {
        self.checked.and_then(|f| f(cx))
    }
    fn run(&self, cx: &mut CommandContext<'_>, params: &Value) -> CommandResult {
        (self.run)(cx, params)
    }
}

/// The set of all registered commands, keyed by id.
#[derive(Default)]
pub struct CommandRegistry {
    commands: BTreeMap<&'static str, Box<dyn Command>>,
}

impl std::fmt::Debug for CommandRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CommandRegistry")
            .field("len", &self.commands.len())
            .finish()
    }
}

impl CommandRegistry {
    /// An empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a command.
    ///
    /// # Panics
    /// If a command with the same id is already registered, or the id is not a
    /// dotted lowerCamel identifier. Both are programming errors caught by tests.
    pub fn register(&mut self, cmd: impl Command) {
        let id = cmd.spec().id;
        assert!(is_valid_id(id), "invalid command id `{id}`");
        let prev = self.commands.insert(id, Box::new(cmd));
        assert!(prev.is_none(), "duplicate command id `{id}`");
    }

    /// Looks up a command.
    pub fn get(&self, id: &str) -> Option<&dyn Command> {
        self.commands.get(id).map(|b| b.as_ref())
    }

    /// Whether `id` is registered.
    pub fn contains(&self, id: &str) -> bool {
        self.commands.contains_key(id)
    }

    /// All commands in id order.
    pub fn iter(&self) -> impl Iterator<Item = &dyn Command> {
        self.commands.values().map(|b| b.as_ref())
    }

    /// Number of registered commands.
    pub fn len(&self) -> usize {
        self.commands.len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    /// Whether `id` is registered and enabled. Unregistered ids are disabled.
    pub fn is_enabled(&self, id: &str, cx: &CommandContext<'_>) -> bool {
        self.get(id)
            .is_some_and(|c| guard(|| c.enabled(cx)).unwrap_or(false))
    }

    /// Toggle state of `id`, if it is a toggle command.
    pub fn is_checked(&self, id: &str, cx: &CommandContext<'_>) -> Option<bool> {
        self.get(id)
            .and_then(|c| guard(|| c.checked(cx)).ok().flatten())
    }

    /// Executes `id` with `params` (`Value::Null` when there are none).
    ///
    /// Panics inside the command are caught here (NF-002): they are logged, turned
    /// into [`CommandError::Panicked`] and the UI shows an "operation failed" notice.
    /// The document source is never left half-written because commands only
    /// produce whole transactions.
    pub fn execute(&self, id: &str, cx: &mut CommandContext<'_>, params: &Value) -> CommandResult {
        let cmd = self
            .get(id)
            .ok_or_else(|| CommandError::Unknown(id.to_owned()))?;
        if !self.is_enabled(id, cx) {
            return Err(CommandError::Disabled(id.to_owned()));
        }
        log::debug!("execute {id}");
        match catch_unwind(AssertUnwindSafe(|| cmd.run(cx, params))) {
            Ok(result) => {
                if let Err(e) = &result {
                    log::warn!("{e}");
                }
                result
            }
            Err(payload) => {
                let msg = panic_message(payload.as_ref());
                log::error!("command `{id}` panicked: {msg}");
                cx.effects
                    .push(Effect::Notify(crate::state::Notice::OperationFailed {
                        command: id.to_owned(),
                    }));
                Err(CommandError::Panicked {
                    id: id.to_owned(),
                    msg,
                })
            }
        }
    }
}

fn guard<R>(f: impl FnOnce() -> R) -> std::thread::Result<R> {
    catch_unwind(AssertUnwindSafe(f))
}

/// Extracts a human readable message from a panic payload.
pub fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_owned()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic".to_owned()
    }
}

/// Whether `id` is `segment(.segment)+` where each segment is lowerCamel ASCII.
pub fn is_valid_id(id: &str) -> bool {
    let mut parts = id.split('.');
    let mut n = 0;
    let ok = parts.all(|p| {
        n += 1;
        p.chars().next().is_some_and(|c| c.is_ascii_lowercase())
            && p.chars().all(|c| c.is_ascii_alphanumeric())
    });
    ok && n >= 2
}

/// Convenience constructor for a [`CommandSpec`] without parameters.
pub const fn spec(
    id: &'static str,
    category: Category,
    req: &'static str,
    doc: &'static str,
) -> CommandSpec {
    CommandSpec {
        id,
        category,
        doc,
        req,
        params: None,
        undo_group: UndoGroup::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;

    fn registry() -> CommandRegistry {
        let mut r = CommandRegistry::new();
        r.register(FnCommand::new(
            spec("test.ok", Category::App, "", ""),
            |_, _| Ok(Outcome::Done),
        ));
        r.register(FnCommand::new(
            spec("test.boom", Category::App, "", ""),
            |_, _| panic!("boom"),
        ));
        r.register(
            FnCommand::new(spec("test.off", Category::App, "", ""), |_, _| {
                Ok(Outcome::Done)
            })
            .enabled(|_| false),
        );
        r
    }

    #[test]
    fn executes_and_catches_panics() {
        let r = registry();
        let mut app = AppState::default();
        let mut effects = Vec::new();
        let mut cx = CommandContext::new(&mut app, &mut effects);
        assert_eq!(
            r.execute("test.ok", &mut cx, &Value::Null),
            Ok(Outcome::Done)
        );
        assert!(matches!(
            r.execute("test.boom", &mut cx, &Value::Null),
            Err(CommandError::Panicked { msg, .. }) if msg == "boom"
        ));
        assert!(matches!(
            r.execute("test.off", &mut cx, &Value::Null),
            Err(CommandError::Disabled(_))
        ));
        assert!(matches!(
            r.execute("nope.x", &mut cx, &Value::Null),
            Err(CommandError::Unknown(_))
        ));
        assert!(matches!(effects.as_slice(), [Effect::Notify(_)]));
    }

    #[test]
    #[should_panic(expected = "duplicate command id")]
    fn duplicate_ids_panic() {
        let mut r = registry();
        r.register(FnCommand::new(
            spec("test.ok", Category::App, "", ""),
            |_, _| Ok(Outcome::Done),
        ));
    }

    #[test]
    fn id_validation() {
        assert!(is_valid_id("view.toggleSidebar"));
        assert!(is_valid_id("paragraph.heading1"));
        assert!(!is_valid_id("view"));
        assert!(!is_valid_id("View.x"));
        assert!(!is_valid_id("view.toggle-sidebar"));
        assert!(!is_valid_id("view..x"));
    }

    #[test]
    fn title_keys() {
        assert_eq!(title_key("view.toggleSidebar"), "cmd-view-toggleSidebar");
    }
}
