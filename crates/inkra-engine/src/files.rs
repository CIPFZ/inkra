//! File and window lifecycle commands (FS-001 … FS-008).

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::command::{
    Category, CommandError, CommandRegistry, CommandResult, CommandSpec, FnCommand, Outcome, spec,
};
use crate::editor::{Editor, ViewMode};
use crate::state::{AppState, CommandContext, Effect, ExternalChange};
use inkra_workspace::files;

fn params(mut s: CommandSpec, p: &'static str) -> CommandSpec {
    s.params = Some(p);
    s
}

fn err(id: &str, e: impl std::fmt::Display) -> CommandError {
    CommandError::Failed {
        id: id.to_owned(),
        msg: e.to_string(),
    }
}

/// Whether the active window holds an untouched, empty Untitled document (it is
/// reused when opening a file, FS-001).
pub fn active_is_blank(app: &AppState) -> bool {
    app.editor.path.is_none() && !app.editor.doc.is_dirty() && app.editor.doc.text().is_empty()
}

/// Loads `path` into a new editor.
pub fn load_editor(
    app: &AppState,
    path: &Path,
) -> Result<(Editor, files::DiskState), files::FileError> {
    let fallback = inkra_text::Encoding::from_label(&app.prefs.string("general.defaultEncoding"));
    let (doc, disk) = files::load(path, crate::markdown_options(&app.prefs), fallback)?;
    let mut e = Editor::new(doc);
    e.path = Some(std::fs::canonicalize(path).unwrap_or_else(|_| path.to_owned()));
    if app.prefs.string("editor.defaultMode") == "source" {
        e.mode = ViewMode::Source;
    }
    Ok((e, disk))
}

/// Opens `path` following FS-001: an already open file is focused; a blank active
/// window is reused; otherwise a new window opens.
pub fn open_path(cx: &mut CommandContext<'_>, path: &Path) -> CommandResult {
    let canon = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_owned());
    if let Some((meta, _)) = cx
        .app
        .editors()
        .find(|(_, e)| e.path.as_deref() == Some(canon.as_path()))
    {
        let id = meta.id;
        cx.effects.push(Effect::FocusWindow(id));
        return Ok(Outcome::Done);
    }
    let (editor, disk) = load_editor(cx.app, &canon).map_err(|e| err("file.open", e))?;
    cx.app.session.touch_recent(&canon);
    let reuse = match cx.app.prefs.string("general.openFileIn").as_str() {
        "newWindow" => false,
        // Never replaces unsaved changes (iron rule: no content loss).
        "current" => !cx.app.editor.doc.is_dirty(),
        _ => active_is_blank(cx.app),
    };
    if reuse && let Some(p) = &cx.app.paths {
        inkra_workspace::recovery::Recovery::new(p.recovery_dir()).remove(&cx.app.meta.recovery_id);
    }
    if reuse {
        cx.app.editor = editor;
        cx.app.meta.disk = Some(disk);
        cx.app.meta.external = None;
        cx.effects.push(Effect::ScrollToCaret);
    } else {
        let id = cx.app.new_window(editor);
        if let Some(p) = cx.app.others.iter_mut().find(|p| p.meta.id == id) {
            p.meta.disk = Some(disk);
        }
        cx.effects.push(Effect::NewWindow(id));
    }
    Ok(Outcome::Done)
}

/// Saves the active document to `path` (FS-003).
pub fn save_to(cx: &mut CommandContext<'_>, path: &Path) -> CommandResult {
    let disk = files::save(path, &cx.app.editor.doc).map_err(|e| err("file.save", e))?;
    let e = &mut cx.app.editor;
    e.doc.mark_saved();
    e.path = Some(std::fs::canonicalize(path).unwrap_or_else(|_| path.to_owned()));
    cx.app.meta.disk = Some(disk);
    cx.app.meta.external = None;
    if let Some(p) = &cx.app.paths {
        inkra_workspace::recovery::Recovery::new(p.recovery_dir()).remove(&cx.app.meta.recovery_id);
    }
    if let Some(p) = cx.app.editor.path.clone() {
        cx.app.session.touch_recent(&p);
    }
    Ok(Outcome::Done)
}

fn save(cx: &mut CommandContext<'_>, close_after: bool, quit: bool) -> CommandResult {
    match cx.app.editor.path.clone() {
        Some(p)
            if cx.app.meta.external != Some(ExternalChange::Deleted)
                || p.parent().is_some_and(Path::exists) =>
        {
            save_to(cx, &p)?;
            if close_after {
                cx.effects.push(Effect::CloseWindow(cx.app.active()));
            }
            Ok(Outcome::Done)
        }
        _ => {
            cx.effects.push(Effect::SaveAsDialog { close_after, quit });
            Ok(Outcome::Done)
        }
    }
}

/// Registers file commands.
pub fn register(r: &mut CommandRegistry) {
    use Category::File;
    for (id, doc) in [
        ("file.new", "New blank document in a new window (FS-001)."),
        ("file.newWindow", "New window (same as file.new)."),
    ] {
        r.register(FnCommand::new(spec(id, File, "FS-001", doc), |cx, _| {
            let mut e = Editor::default();
            if cx.app.prefs.string("editor.defaultMode") == "source" {
                e.mode = ViewMode::Source;
            }
            let id = cx.app.new_window(e);
            cx.effects.push(Effect::NewWindow(id));
            Ok(Outcome::Done)
        }));
    }
    r.register(FnCommand::new(
        params(
            spec(
                "file.open",
                File,
                "FS-002",
                "Open a file (dialog without `path`).",
            ),
            r#"{"type":"object","properties":{"path":{"type":"string"}}}"#,
        ),
        |cx, p| match p.get("path").and_then(Value::as_str) {
            Some(path) => open_path(cx, Path::new(path)),
            None => {
                cx.effects.push(Effect::OpenDialog);
                Ok(Outcome::Done)
            }
        },
    ));
    r.register(FnCommand::new(
        params(
            spec(
                "file.openRecent",
                File,
                "FS-002",
                "Open a recently used file.",
            ),
            r#"{"type":"object","required":["path"],"properties":{"path":{"type":"string"}}}"#,
        ),
        |cx, p| {
            let path = PathBuf::from(p.get("path").and_then(Value::as_str).unwrap_or_default());
            if !path.exists() {
                cx.app.session.recent.retain(|r| r != &path);
                return Err(err(
                    "file.openRecent",
                    format!("{} no longer exists", path.display()),
                ));
            }
            open_path(cx, &path)
        },
    ));
    r.register(
        FnCommand::new(
            spec(
                "file.clearRecent",
                File,
                "FS-002",
                "Forget the recent files list.",
            ),
            |cx, _| {
                cx.app.session.recent.clear();
                Ok(Outcome::Done)
            },
        )
        .enabled(|cx| !cx.app.session.recent.is_empty()),
    );
    r.register(FnCommand::new(
        spec(
            "file.openFolder",
            File,
            "FS-002",
            "Open a folder as the sidebar root.",
        ),
        |cx, _| {
            cx.effects.push(Effect::OpenFolderDialog);
            Ok(Outcome::Done)
        },
    ));
    r.register(FnCommand::new(
        spec(
            "file.save",
            File,
            "FS-003",
            "Save (asks for a name for new documents).",
        ),
        |cx, _| save(cx, false, false),
    ));
    r.register(FnCommand::new(
        params(
            spec(
                "file.saveAs",
                File,
                "FS-003",
                "Save under a new name (dialog without `path`).",
            ),
            r#"{"type":"object","properties":{"path":{"type":"string"}}}"#,
        ),
        |cx, p| match p.get("path").and_then(Value::as_str) {
            Some(path) => save_to(cx, Path::new(path)),
            None => {
                cx.effects.push(Effect::SaveAsDialog {
                    close_after: false,
                    quit: false,
                });
                Ok(Outcome::Done)
            }
        },
    ));
    r.register(FnCommand::new(
        spec(
            "file.saveAll",
            File,
            "FS-003",
            "Save every modified document that has a file.",
        ),
        |cx, _| {
            let ids = cx.app.window_ids();
            let active = cx.app.active();
            let mut first_err = None;
            for id in ids {
                cx.app.activate(id);
                if cx.app.editor.doc.is_dirty()
                    && let Some(p) = cx.app.editor.path.clone()
                    && let Err(e) = save_to(cx, &p)
                {
                    first_err.get_or_insert(e);
                }
            }
            cx.app.activate(active);
            first_err.map_or(Ok(Outcome::Done), Err)
        },
    ));
    r.register(FnCommand::new(
        spec(
            "file.close",
            File,
            "FS-005",
            "Close the window (asks to save changes).",
        ),
        |cx, _| {
            let id = cx.app.active();
            if cx.app.editor.doc.is_dirty() {
                cx.effects.push(Effect::ConfirmClose { id, quit: false });
            } else {
                cx.effects.push(Effect::CloseWindow(id));
            }
            Ok(Outcome::Done)
        },
    ));
    r.register(
        FnCommand::new(
            spec(
                "file.revealInFolder",
                File,
                "FS-010",
                "Show the file in the system file manager.",
            ),
            |cx, _| {
                if let Some(p) = cx.app.editor.path.clone() {
                    cx.effects.push(Effect::Reveal(p));
                }
                Ok(Outcome::Done)
            },
        )
        .enabled(|cx| cx.app.editor.path.is_some()),
    );
    r.register(FnCommand::new(
        spec(
            "file.reload",
            File,
            "FS-007",
            "Reload the file from disk, discarding local changes.",
        ),
        |cx, _| {
            reload(cx.app).map_err(|e| err("file.reload", e))?;
            Ok(Outcome::Done)
        },
    ));
    r.register(FnCommand::new(
        spec(
            "file.keepLocal",
            File,
            "FS-007",
            "Keep the local version after an external change.",
        ),
        |cx, _| {
            cx.app.meta.external = None;
            // Saving later overwrites the disk version only on explicit save.
            cx.app.meta.disk = cx.app.editor.path.as_deref().and_then(files::disk_state);
            Ok(Outcome::Done)
        },
    ));
    r.register(FnCommand::new(
        params(
            spec(
                "view.switchWindow",
                Category::View,
                "FS-040",
                "Bring a window to the front.",
            ),
            r#"{"type":"object","required":["window"],"properties":{"window":{"type":"integer"}}}"#,
        ),
        |cx, p| {
            let id = p.get("window").and_then(Value::as_u64).unwrap_or_default();
            cx.effects.push(Effect::FocusWindow(id));
            Ok(Outcome::Done)
        },
    ));
}

/// Replaces the active document with the disk version (FS-007), keeping the
/// caret (clamped). Done as one transaction so it can be undone.
pub fn reload(app: &mut AppState) -> Result<(), files::FileError> {
    let Some(path) = app.editor.path.clone() else {
        return Ok(());
    };
    let (fresh, disk) = load_editor(app, &path)?;
    let new_text = fresh.doc.text().to_string();
    let e = &mut app.editor;
    let old_len = e.doc.text().len();
    let caret = e.sel.head;
    let tx = inkra_doc::Transaction::single(0..old_len, &new_text)
        .with_selection_before(e.sel)
        .with_selection_after(inkra_doc::Selection::caret(caret.min(new_text.len())));
    e.doc.break_undo_group();
    e.apply(tx, inkra_doc::UndoPolicy::NewGroup);
    e.sel = e.sel.clamp(e.doc.text().len());
    let t = e.doc.text();
    e.sel = inkra_doc::Selection::new(t.snap(e.sel.anchor), t.snap(e.sel.head));
    e.doc.mark_saved();
    app.meta.disk = Some(disk);
    app.meta.external = None;
    Ok(())
}

/// Reacts to a changed file on disk for window `id` (FS-007): silently reloads
/// clean documents, flags dirty ones.
pub fn external_change(app: &mut AppState, id: u64) {
    let active = app.active();
    if !app.activate(id) {
        return;
    }
    if let Some(path) = app.editor.path.clone() {
        match files::disk_state(&path) {
            None => app.meta.external = Some(ExternalChange::Deleted),
            Some(now) => {
                let changed = app.meta.disk.as_ref().is_none_or(|d| d.hash != now.hash);
                if changed {
                    let auto = app.prefs.bool("general.autoReload");
                    if !app.editor.doc.is_dirty() && auto {
                        if let Err(e) = reload(app) {
                            log::warn!("reload failed: {e}");
                        }
                    } else {
                        app.meta.external = Some(ExternalChange::Modified);
                    }
                }
            }
        }
    }
    app.activate(active);
}

/// Writes recovery data for every modified window (FS-004).
pub fn write_recovery(app: &AppState) {
    let Some(paths) = &app.paths else { return };
    let rec = inkra_workspace::recovery::Recovery::new(paths.recovery_dir());
    for (meta, e) in app.editors() {
        if e.doc.is_dirty() {
            let title = e
                .path
                .as_ref()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "Untitled".into());
            if let Err(err) = rec.write(
                &meta.recovery_id,
                e.path.as_deref(),
                &title,
                &e.doc.text().to_string(),
            ) {
                log::warn!("recovery write failed: {err}");
            }
        } else {
            rec.remove(&meta.recovery_id);
        }
    }
}
