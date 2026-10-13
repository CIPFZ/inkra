//! Sidebar commands (FS-020 … FS-024, FS-030, FS-031) and file operations from
//! the file tree (FS-022).

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::command::{
    Category, CommandError, CommandRegistry, CommandResult, CommandSpec, FnCommand, Outcome, spec,
};
use crate::state::{AppState, CommandContext, Effect, ExternalChange, SidebarView};
use inkra_workspace::fileops;

fn params(mut s: CommandSpec, p: &'static str) -> CommandSpec {
    s.params = Some(p);
    s
}

fn fail(id: &str, e: impl std::fmt::Display) -> CommandError {
    CommandError::Failed {
        id: id.to_owned(),
        msg: e.to_string(),
    }
}

fn path_param(p: &Value, key: &str) -> Option<PathBuf> {
    p.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
}

fn show(cx: &mut CommandContext<'_>, view: SidebarView, toggle: bool) -> CommandResult {
    let v = &mut cx.app.view;
    if toggle && v.sidebar_visible && v.sidebar_view == view {
        v.sidebar_visible = false;
    } else {
        v.sidebar_visible = true;
        v.sidebar_view = view;
    }
    Ok(Outcome::Done)
}

/// Updates every open document whose path is `from` or below it after a rename
/// or move to `to`.
pub fn retarget_open_documents(app: &mut AppState, from: &Path, to: &Path) {
    let fix = |p: &mut Option<PathBuf>| {
        if let Some(cur) = p.as_ref()
            && let Ok(rest) = cur.strip_prefix(from)
        {
            *p = Some(if rest.as_os_str().is_empty() {
                to.to_owned()
            } else {
                to.join(rest)
            });
        }
    };
    fix(&mut app.editor.path);
    for w in &mut app.others {
        fix(&mut w.editor.path);
    }
    for r in &mut app.session.recent {
        if let Ok(rest) = r.strip_prefix(from) {
            *r = if rest.as_os_str().is_empty() {
                to.to_owned()
            } else {
                to.join(rest)
            };
        }
    }
    if let Some(root) = &app.session.root
        && root == from
    {
        app.session.root = Some(to.to_owned());
    }
}

fn mark_deleted(app: &mut AppState, gone: &Path) {
    if app
        .editor
        .path
        .as_deref()
        .is_some_and(|p| p.starts_with(gone))
    {
        app.meta.external = Some(ExternalChange::Deleted);
    }
    for w in &mut app.others {
        if w.editor
            .path
            .as_deref()
            .is_some_and(|p| p.starts_with(gone))
        {
            w.meta.external = Some(ExternalChange::Deleted);
        }
    }
}

/// Registers sidebar commands.
pub fn register(r: &mut CommandRegistry) {
    use Category::{File, View};
    r.register(
        FnCommand::new(
            spec(
                "view.outline",
                View,
                "FS-020",
                "Show the outline in the sidebar (again: hide).",
            ),
            |cx, _| show(cx, SidebarView::Outline, true),
        )
        .checked(|cx| {
            Some(cx.app.view.sidebar_visible && cx.app.view.sidebar_view == SidebarView::Outline)
        }),
    );
    r.register(
        FnCommand::new(
            spec(
                "view.articles",
                View,
                "FS-021",
                "Show the article list in the sidebar (again: hide).",
            ),
            |cx, _| show(cx, SidebarView::Articles, true),
        )
        .checked(|cx| {
            Some(cx.app.view.sidebar_visible && cx.app.view.sidebar_view == SidebarView::Articles)
        }),
    );
    r.register(
        FnCommand::new(
            spec(
                "view.fileTree",
                View,
                "FS-022",
                "Show the file tree in the sidebar (again: hide).",
            ),
            |cx, _| show(cx, SidebarView::Files, true),
        )
        .checked(|cx| {
            Some(cx.app.view.sidebar_visible && cx.app.view.sidebar_view == SidebarView::Files)
        }),
    );
    r.register(FnCommand::new(
        spec(
            "search.global",
            Category::Edit,
            "FS-031",
            "Search all files of the root folder.",
        ),
        |cx, _| {
            show(cx, SidebarView::Search, false)?;
            cx.effects.push(Effect::FocusGlobalSearch);
            Ok(Outcome::Done)
        },
    ));
    r.register(FnCommand::new(
        spec(
            "file.quickOpen",
            File,
            "FS-030",
            "Fuzzy-find a file of the root folder or the recent files.",
        ),
        |cx, _| {
            cx.effects.push(Effect::ShowQuickOpen);
            Ok(Outcome::Done)
        },
    ));
    r.register(
        FnCommand::new(
            spec(
                "file.revealInSidebar",
                File,
                "FS-023",
                "Show the current file in the sidebar file tree.",
            ),
            |cx, _| {
                let Some(p) = cx.app.editor.path.clone() else {
                    return Ok(Outcome::Noop);
                };
                if cx
                    .app
                    .session
                    .root
                    .as_ref()
                    .is_none_or(|r| !p.starts_with(r))
                {
                    cx.app.session.root = p.parent().map(Path::to_owned);
                    cx.effects.push(Effect::RootChanged);
                }
                show(cx, SidebarView::Files, false)?;
                cx.effects.push(Effect::RevealInTree(p));
                Ok(Outcome::Done)
            },
        )
        .enabled(|cx| cx.app.editor.path.is_some()),
    );
    r.register(FnCommand::new(
        params(
            spec(
                "sidebar.setRoot",
                File,
                "FS-022",
                "Use a folder as the sidebar root.",
            ),
            r#"{"type":"object","required":["path"],"properties":{"path":{"type":"string"}}}"#,
        ),
        |cx, p| {
            let path = path_param(p, "path").ok_or_else(|| fail("sidebar.setRoot", "no path"))?;
            if !path.is_dir() {
                return Err(fail(
                    "sidebar.setRoot",
                    format!("{} is not a folder", path.display()),
                ));
            }
            let path = inkra_workspace::paths::canonical(&path);
            cx.app.session.touch_folder(&path);
            cx.app.session.root = Some(path);
            cx.effects.push(Effect::RootChanged);
            Ok(Outcome::Done)
        },
    ));
    r.register(FnCommand::new(
        spec(
            "sidebar.refresh",
            File,
            "FS-022",
            "Re-read the root folder.",
        ),
        |cx, _| {
            cx.effects.push(Effect::RootChanged);
            Ok(Outcome::Done)
        },
    ));
    r.register(FnCommand::new(
        params(
            spec(
                "sidebar.newFile",
                File,
                "FS-022",
                "Create `Untitled.md` in a folder and start renaming it.",
            ),
            r#"{"type":"object","required":["dir"],"properties":{"dir":{"type":"string"}}}"#,
        ),
        |cx, p| {
            let dir = path_param(p, "dir").ok_or_else(|| fail("sidebar.newFile", "no folder"))?;
            let ext = cx.app.prefs.string("general.defaultExtension");
            let ext = if ext.is_empty() {
                "md".to_owned()
            } else {
                ext.trim_start_matches('.').to_owned()
            };
            let f = fileops::new_file(&dir, &format!("Untitled.{ext}"))
                .map_err(|e| fail("sidebar.newFile", e))?;
            cx.effects.push(Effect::RevealInTree(f.clone()));
            cx.effects.push(Effect::RenameInTree(f));
            Ok(Outcome::Done)
        },
    ));
    r.register(FnCommand::new(
        params(
            spec(
                "sidebar.newFolder",
                File,
                "FS-022",
                "Create a folder and start renaming it.",
            ),
            r#"{"type":"object","required":["dir"],"properties":{"dir":{"type":"string"}}}"#,
        ),
        |cx, p| {
            let dir = path_param(p, "dir").ok_or_else(|| fail("sidebar.newFolder", "no folder"))?;
            let f = fileops::new_folder(&dir, "New Folder")
                .map_err(|e| fail("sidebar.newFolder", e))?;
            cx.effects.push(Effect::RevealInTree(f.clone()));
            cx.effects.push(Effect::RenameInTree(f));
            Ok(Outcome::Done)
        },
    ));
    r.register(FnCommand::new(
        params(
            spec("sidebar.rename", File, "FS-022", "Rename a file or folder (open documents follow)."),
            r#"{"type":"object","required":["path","name"],"properties":{"path":{"type":"string"},"name":{"type":"string"}}}"#,
        ),
        |cx, p| {
            let path = path_param(p, "path").ok_or_else(|| fail("sidebar.rename", "no path"))?;
            let name = p.get("name").and_then(Value::as_str).unwrap_or_default();
            let to = fileops::rename(&path, name.trim()).map_err(|e| fail("sidebar.rename", e))?;
            retarget_open_documents(cx.app, &path, &to);
            cx.effects.push(Effect::RevealInTree(to));
            Ok(Outcome::Done)
        },
    ));
    r.register(FnCommand::new(
        params(
            spec("sidebar.move", File, "FS-022", "Move a file or folder into another folder (open documents follow)."),
            r#"{"type":"object","required":["path","dest"],"properties":{"path":{"type":"string"},"dest":{"type":"string"}}}"#,
        ),
        |cx, p| {
            let path = path_param(p, "path").ok_or_else(|| fail("sidebar.move", "no path"))?;
            let dest = path_param(p, "dest").ok_or_else(|| fail("sidebar.move", "no destination"))?;
            let to = fileops::move_into(&path, &dest).map_err(|e| fail("sidebar.move", e))?;
            retarget_open_documents(cx.app, &path, &to);
            cx.effects.push(Effect::RevealInTree(to));
            Ok(Outcome::Done)
        },
    ));
    r.register(FnCommand::new(
        params(
            spec(
                "sidebar.delete",
                File,
                "FS-022",
                "Move a file or folder to the recycle bin.",
            ),
            r#"{"type":"object","required":["path"],"properties":{"path":{"type":"string"}}}"#,
        ),
        |cx, p| {
            let path = path_param(p, "path").ok_or_else(|| fail("sidebar.delete", "no path"))?;
            fileops::to_trash(&path).map_err(|e| fail("sidebar.delete", e))?;
            mark_deleted(cx.app, &path);
            Ok(Outcome::Done)
        },
    ));
    r.register(FnCommand::new(
        params(
            spec("sidebar.copyPath", File, "FS-022", "Copy the absolute (or root-relative) path."),
            r#"{"type":"object","required":["path"],"properties":{"path":{"type":"string"},"relative":{"type":"boolean"}}}"#,
        ),
        |cx, p| {
            let path = path_param(p, "path").ok_or_else(|| fail("sidebar.copyPath", "no path"))?;
            let rel = p.get("relative").and_then(Value::as_bool).unwrap_or(false);
            let text = match (&cx.app.session.root, rel) {
                (Some(root), true) => path.strip_prefix(root).unwrap_or(&path).to_string_lossy().replace('\\', "/"),
                _ => path.to_string_lossy().into_owned(),
            };
            cx.effects.push(Effect::SetClipboard { text, html: None });
            Ok(Outcome::Done)
        },
    ));
    r.register(FnCommand::new(
        params(
            spec(
                "sidebar.openInNewWindow",
                File,
                "FS-022",
                "Open a file in a new window.",
            ),
            r#"{"type":"object","required":["path"],"properties":{"path":{"type":"string"}}}"#,
        ),
        |cx, p| {
            let path =
                path_param(p, "path").ok_or_else(|| fail("sidebar.openInNewWindow", "no path"))?;
            let (editor, disk) = crate::files::load_editor(cx.app, &path)
                .map_err(|e| fail("sidebar.openInNewWindow", e))?;
            cx.app.session.touch_recent(&path);
            let id = cx.app.new_window(editor);
            if let Some(w) = cx.app.others.iter_mut().find(|w| w.meta.id == id) {
                w.meta.disk = Some(disk);
            }
            cx.effects.push(Effect::NewWindow(id));
            Ok(Outcome::Done)
        },
    ));
    r.register(FnCommand::new(
        params(
            spec(
                "sidebar.reveal",
                File,
                "FS-022",
                "Show a file or folder in the system file manager.",
            ),
            r#"{"type":"object","required":["path"],"properties":{"path":{"type":"string"}}}"#,
        ),
        |cx, p| {
            let path = path_param(p, "path").ok_or_else(|| fail("sidebar.reveal", "no path"))?;
            cx.effects.push(Effect::Reveal(path));
            Ok(Outcome::Done)
        },
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Engine;
    use crate::prefs::Preferences;
    use serde_json::json;

    #[test]
    fn rename_and_move_follow_open_documents() {
        let d = tempfile::tempdir().unwrap();
        let r = d.path().canonicalize().unwrap();
        std::fs::create_dir_all(r.join("sub")).unwrap();
        std::fs::write(r.join("a.md"), "# A").unwrap();
        let mut e = Engine::new(Preferences::new());
        e.execute("file.open", &json!({ "path": r.join("a.md") }))
            .unwrap();
        e.execute(
            "sidebar.rename",
            &json!({ "path": r.join("a.md"), "name": "b.md" }),
        )
        .unwrap();
        assert_eq!(
            e.state.editor.path.as_deref(),
            Some(r.join("b.md").as_path())
        );
        e.execute(
            "sidebar.move",
            &json!({ "path": r.join("b.md"), "dest": r.join("sub") }),
        )
        .unwrap();
        assert_eq!(
            e.state.editor.path.as_deref(),
            Some(r.join("sub/b.md").as_path())
        );
        e.execute(
            "sidebar.rename",
            &json!({ "path": r.join("sub"), "name": "notes" }),
        )
        .unwrap();
        assert_eq!(
            e.state.editor.path.as_deref(),
            Some(r.join("notes/b.md").as_path())
        );
        e.execute("sidebar.newFile", &json!({ "dir": r })).unwrap();
        assert!(r.join("Untitled.md").exists());
        assert!(
            e.execute(
                "sidebar.rename",
                &json!({ "path": r.join("Untitled.md"), "name": "x/y" })
            )
            .is_err()
        );
        // views
        e.execute("view.fileTree", &Value::Null).unwrap();
        assert!(e.state.view.sidebar_visible);
        assert_eq!(e.state.view.sidebar_view, SidebarView::Files);
        e.execute("view.fileTree", &Value::Null).unwrap();
        assert!(!e.state.view.sidebar_visible, "the same shortcut hides it");
    }
}
