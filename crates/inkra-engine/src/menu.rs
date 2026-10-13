//! Menu bar model (UI-001, UI-002).
//!
//! The menu structure is pure data owned by the engine; the UI only renders it
//! (egui menu bar on Windows/Linux, native `muda` menu on macOS). Every leaf maps
//! to a command id; its label is the command's i18n title unless overridden.

use crate::keymap::Os;

/// Items generated at runtime from application state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dynamic {
    /// Recently opened files (`file.openRecent` with `{"path": …}`).
    RecentFiles,
    /// Installed themes (`theme.select` with `{"name": …}`).
    Themes,
    /// Open windows (`view.switchWindow` with `{"window": …}`).
    Windows,
    /// Custom export commands from preferences (`file.exportCustom` with
    /// `{"index": …}`, EX-004).
    CustomExports,
}

/// One entry of a menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuItem {
    /// A command leaf.
    Command {
        /// Command id.
        id: &'static str,
        /// JSON parameters, if any.
        params: Option<&'static str>,
        /// i18n key overriding the command title (used with `params`).
        label: Option<&'static str>,
        /// Only shown on this OS.
        only: Option<Os>,
        /// Hidden on this OS.
        not: Option<Os>,
    },
    /// A nested menu.
    Submenu {
        /// i18n key of the submenu title.
        label: &'static str,
        /// Children.
        items: &'static [MenuItem],
    },
    /// Runtime-generated items.
    Dynamic(Dynamic),
    /// A separator line.
    Separator,
}

impl MenuItem {
    /// Whether the item is visible on `os`.
    pub fn visible_on(&self, os: Os) -> bool {
        match self {
            Self::Command { only, not, .. } => {
                only.is_none_or(|o| o == os) && not.is_none_or(|o| o != os)
            }
            _ => true,
        }
    }
}

/// A top-level menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Menu {
    /// i18n key of the title.
    pub label: &'static str,
    /// Items.
    pub items: &'static [MenuItem],
}

const fn c(id: &'static str) -> MenuItem {
    MenuItem::Command {
        id,
        params: None,
        label: None,
        only: None,
        not: None,
    }
}
const fn p(id: &'static str, params: &'static str, label: &'static str) -> MenuItem {
    MenuItem::Command {
        id,
        params: Some(params),
        label: Some(label),
        only: None,
        not: None,
    }
}
const fn mac_only(id: &'static str) -> MenuItem {
    MenuItem::Command {
        id,
        params: None,
        label: None,
        only: Some(Os::Mac),
        not: None,
    }
}
const fn not_mac(id: &'static str) -> MenuItem {
    MenuItem::Command {
        id,
        params: None,
        label: None,
        only: None,
        not: Some(Os::Mac),
    }
}
const fn sub(label: &'static str, items: &'static [MenuItem]) -> MenuItem {
    MenuItem::Submenu { label, items }
}
const S: MenuItem = MenuItem::Separator;

/// The full menu bar of SPEC 10.1.
pub const MENU_BAR: &[Menu] = &[
    Menu {
        label: "menu-file",
        items: &[
            c("file.new"),
            c("file.newWindow"),
            S,
            c("file.open"),
            c("file.openFolder"),
            sub(
                "menu-file-openRecent",
                &[
                    MenuItem::Dynamic(Dynamic::RecentFiles),
                    S,
                    c("file.clearRecent"),
                ],
            ),
            c("file.quickOpen"),
            S,
            c("file.save"),
            c("file.saveAs"),
            c("file.saveAll"),
            c("file.moveTo"),
            S,
            c("file.import"),
            sub(
                "menu-file-export",
                &[
                    c("file.exportHtml"),
                    c("file.exportHtmlPlain"),
                    c("file.exportPdf"),
                    c("file.exportImage"),
                    S,
                    p(
                        "file.exportPandoc",
                        r#"{"format":"docx"}"#,
                        "export-format-docx",
                    ),
                    p(
                        "file.exportPandoc",
                        r#"{"format":"odt"}"#,
                        "export-format-odt",
                    ),
                    p(
                        "file.exportPandoc",
                        r#"{"format":"rtf"}"#,
                        "export-format-rtf",
                    ),
                    p(
                        "file.exportPandoc",
                        r#"{"format":"epub"}"#,
                        "export-format-epub",
                    ),
                    p(
                        "file.exportPandoc",
                        r#"{"format":"latex"}"#,
                        "export-format-latex",
                    ),
                    p(
                        "file.exportPandoc",
                        r#"{"format":"mediawiki"}"#,
                        "export-format-mediawiki",
                    ),
                    p(
                        "file.exportPandoc",
                        r#"{"format":"rst"}"#,
                        "export-format-rst",
                    ),
                    p(
                        "file.exportPandoc",
                        r#"{"format":"textile"}"#,
                        "export-format-textile",
                    ),
                    p(
                        "file.exportPandoc",
                        r#"{"format":"opml"}"#,
                        "export-format-opml",
                    ),
                    p(
                        "file.exportPandoc",
                        r#"{"format":"org"}"#,
                        "export-format-org",
                    ),
                    c("help.installPandoc"),
                    MenuItem::Dynamic(Dynamic::CustomExports),
                ],
            ),
            c("file.pageSetup"),
            c("file.print"),
            S,
            c("file.properties"),
            c("file.revealInFolder"),
            c("file.rename"),
            c("file.delete"),
            S,
            c("app.preferences"),
            S,
            c("file.close"),
            c("app.quit"),
        ],
    },
    Menu {
        label: "menu-edit",
        items: &[
            c("edit.undo"),
            c("edit.redo"),
            S,
            c("edit.cut"),
            c("edit.copy"),
            c("edit.paste"),
            c("edit.pastePlain"),
            sub(
                "menu-edit-copyAs",
                &[
                    c("edit.copyAsMarkdown"),
                    c("edit.copyAsHtml"),
                    c("edit.copyAsPlain"),
                    c("edit.copyAsHtmlCode"),
                ],
            ),
            S,
            c("edit.selectAll"),
            c("edit.selectLine"),
            c("edit.selectWord"),
            c("edit.selectStyleScope"),
            sub(
                "menu-edit-deleteRange",
                &[c("edit.deleteWord"), c("edit.deleteLine")],
            ),
            S,
            sub(
                "menu-edit-jump",
                &[
                    c("edit.jumpTop"),
                    c("edit.jumpSelection"),
                    c("edit.jumpBottom"),
                ],
            ),
            sub(
                "menu-edit-mathTools",
                &[c("format.inlineMath"), c("paragraph.mathBlock")],
            ),
            S,
            sub(
                "menu-edit-findReplace",
                &[
                    c("edit.find"),
                    c("edit.findNext"),
                    c("edit.findPrev"),
                    c("edit.replace"),
                ],
            ),
            sub("menu-edit-spelling", &[c("edit.toggleSpellcheck")]),
            sub(
                "menu-edit-smartPunctuation",
                &[c("edit.toggleSmartPunctuation")],
            ),
            sub(
                "menu-edit-lineEndings",
                &[
                    p("edit.lineEnding", r#"{"style":"lf"}"#, "line-ending-lf"),
                    p("edit.lineEnding", r#"{"style":"crlf"}"#, "line-ending-crlf"),
                ],
            ),
            sub("menu-edit-whitespace", &[c("view.showWhitespace")]),
            mac_only("edit.emojiSymbols"),
        ],
    },
    Menu {
        label: "menu-paragraph",
        items: &[
            c("paragraph.heading1"),
            c("paragraph.heading2"),
            c("paragraph.heading3"),
            c("paragraph.heading4"),
            c("paragraph.heading5"),
            c("paragraph.heading6"),
            S,
            c("paragraph.normal"),
            S,
            c("paragraph.headingUp"),
            c("paragraph.headingDown"),
            S,
            c("paragraph.table"),
            c("paragraph.codeFences"),
            c("paragraph.mathBlock"),
            c("paragraph.quote"),
            sub(
                "menu-paragraph-alert",
                &[
                    p("paragraph.alert", r#"{"kind":"note"}"#, "alert-note"),
                    p("paragraph.alert", r#"{"kind":"tip"}"#, "alert-tip"),
                    p(
                        "paragraph.alert",
                        r#"{"kind":"important"}"#,
                        "alert-important",
                    ),
                    p("paragraph.alert", r#"{"kind":"warning"}"#, "alert-warning"),
                    p("paragraph.alert", r#"{"kind":"caution"}"#, "alert-caution"),
                ],
            ),
            S,
            c("paragraph.orderedList"),
            c("paragraph.unorderedList"),
            c("paragraph.taskList"),
            c("paragraph.taskStatus"),
            sub(
                "menu-paragraph-listIndent",
                &[c("paragraph.indent"), c("paragraph.outdent")],
            ),
            S,
            c("paragraph.linkReference"),
            c("paragraph.footnoteDefinition"),
            S,
            c("paragraph.hr"),
            c("paragraph.toc"),
            c("paragraph.frontMatter"),
            c("paragraph.html"),
        ],
    },
    Menu {
        label: "menu-format",
        items: &[
            c("format.bold"),
            c("format.italic"),
            c("format.underline"),
            c("format.code"),
            c("format.strike"),
            c("format.comment"),
            c("format.highlight"),
            c("format.superscript"),
            c("format.subscript"),
            S,
            c("format.link"),
            c("format.editLink"),
            sub(
                "menu-format-image",
                &[
                    c("format.image"),
                    c("format.imageLocal"),
                    c("format.imageOps"),
                ],
            ),
            S,
            c("format.clear"),
            c("format.inlineMath"),
        ],
    },
    Menu {
        label: "menu-view",
        items: &[
            c("view.toggleSidebar"),
            c("view.outline"),
            c("view.articles"),
            c("view.fileTree"),
            S,
            c("view.toggleToolbar"),
            c("view.sourceMode"),
            c("view.focusMode"),
            c("view.typewriterMode"),
            S,
            c("view.toggleStatusBar"),
            not_mac("view.toggleMenuBar"),
            c("view.fullscreen"),
            not_mac("view.alwaysOnTop"),
            S,
            c("view.actualSize"),
            c("view.zoomIn"),
            c("view.zoomOut"),
            S,
            sub(
                "menu-view-switchWindow",
                &[MenuItem::Dynamic(Dynamic::Windows)],
            ),
            c("view.debugOverlay"),
        ],
    },
    Menu {
        label: "menu-themes",
        items: &[
            MenuItem::Dynamic(Dynamic::Themes),
            S,
            c("theme.auto"),
            c("theme.openFolder"),
        ],
    },
    Menu {
        label: "menu-help",
        items: &[
            c("help.quickStart"),
            c("help.markdownReference"),
            c("help.shortcuts"),
            c("help.customThemes"),
            S,
            c("help.reportIssue"),
            c("help.changelog"),
            c("help.checkUpdates"),
            S,
            c("app.about"),
        ],
    },
];

/// Visits every command leaf (including those in submenus) of the menu bar.
pub fn for_each_command(mut f: impl FnMut(&MenuItem)) {
    fn walk(items: &[MenuItem], f: &mut dyn FnMut(&MenuItem)) {
        for item in items {
            match item {
                MenuItem::Command { .. } => f(item),
                MenuItem::Submenu { items, .. } => walk(items, f),
                _ => {}
            }
        }
    }
    for menu in MENU_BAR {
        walk(menu.items, &mut f);
    }
}

/// Visits every i18n key used by the menu bar (menu titles, submenu titles and
/// label overrides; command titles are derived from ids).
pub fn for_each_label(mut f: impl FnMut(&'static str)) {
    fn walk(items: &[MenuItem], f: &mut dyn FnMut(&'static str)) {
        for item in items {
            match item {
                MenuItem::Command { label: Some(l), .. } => f(l),
                MenuItem::Submenu { label, items } => {
                    f(label);
                    walk(items, f);
                }
                _ => {}
            }
        }
    }
    for menu in MENU_BAR {
        f(menu.label);
        walk(menu.items, &mut f);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::is_valid_id;

    #[test]
    fn all_ids_valid_and_params_json() {
        for_each_command(|item| {
            if let MenuItem::Command { id, params, .. } = item {
                assert!(is_valid_id(id), "{id}");
                if let Some(p) = params {
                    serde_json::from_str::<serde_json::Value>(p).expect(p);
                }
            }
        });
    }

    #[test]
    fn has_seven_top_level_menus() {
        assert_eq!(MENU_BAR.len(), 7);
    }
}
