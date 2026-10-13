//! Reviewed allowlist of direct third-party dependencies.
//!
//! Every crate a workspace member depends on directly must be listed here with the
//! reason and license (agent prompt: "each new dependency lists reason and
//! license"). `scope` restricts where it may be used.

/// Where a dependency may be used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// Any member.
    Any,
    /// Only the listed members.
    Only(&'static [&'static str]),
}

/// `(crate, scope, license, reason)`.
pub const ALLOWED: &[(&str, Scope, &str, &str)] = &[
    ("egui", Scope::Any, "MIT OR Apache-2.0", "GUI (SPEC 2.1)"),
    (
        "epaint",
        Scope::Only(&["inkra-layout", "inkra-render", "inkra-export"]),
        "MIT OR Apache-2.0",
        "text shaping/layout and shapes (SPEC 2.1)",
    ),
    (
        "eframe",
        Scope::Only(&["inkra"]),
        "MIT OR Apache-2.0",
        "native window + wgpu (SPEC 2.1)",
    ),
    (
        "egui_kittest",
        Scope::Any,
        "MIT OR Apache-2.0",
        "UI tests (SPEC 2.1)",
    ),
    (
        "image",
        Scope::Any,
        "MIT OR Apache-2.0",
        "image decoding (SPEC 2.1)",
    ),
    (
        "winit",
        Scope::Only(&["inkra"]),
        "Apache-2.0",
        "same winit as eframe; raw key hook for clipboard formats egui drops (ED-016)",
    ),
    (
        "syntect",
        Scope::Only(&["inkra-highlight"]),
        "MIT",
        "code highlighting (SPEC 2.1), pure-Rust fancy-regex",
    ),
    (
        "ttf-parser",
        Scope::Only(&["inkra-math", "inkra-diagram"]),
        "MIT OR Apache-2.0",
        "OpenType MATH table and glyph outlines for the math engine (SPEC 2.4)",
    ),
    (
        "tiny-skia-path",
        Scope::Only(&[
            "inkra-math",
            "inkra-diagram",
            "inkra-layout",
            "inkra-render",
            "inkra-export",
        ]),
        "BSD-3-Clause",
        "vector paths shared by math, diagrams, rasterizer and PDF (krilla)",
    ),
    (
        "tiny-skia",
        Scope::Only(&[
            "inkra-render",
            "inkra-export",
            "inkra-math",
            "inkra-diagram",
        ]),
        "BSD-3-Clause",
        "rasterizing formulas and diagrams (also used by resvg); math/diagram examples only",
    ),
    (
        "nucleo-matcher",
        Scope::Only(&["inkra-workspace"]),
        "MPL-2.0",
        "fuzzy matching for quick open and outline filter (SPEC 2.1 nucleo); file-level copyleft, used unmodified",
    ),
    (
        "grep-searcher",
        Scope::Only(&["inkra-workspace"]),
        "Unlicense OR MIT",
        "global search (SPEC 2.1, FS-031)",
    ),
    (
        "grep-regex",
        Scope::Only(&["inkra-workspace"]),
        "Unlicense OR MIT",
        "global search (SPEC 2.1, FS-031)",
    ),
    (
        "grep-matcher",
        Scope::Only(&["inkra-workspace"]),
        "Unlicense OR MIT",
        "global search match ranges (SPEC 2.1, FS-031)",
    ),
    (
        "ignore",
        Scope::Only(&["inkra-workspace"]),
        "Unlicense OR MIT",
        "directory walking with .gitignore and glob filters (SPEC 2.1)",
    ),
    (
        "trash",
        Scope::Only(&["inkra-workspace"]),
        "MIT",
        "move files to the recycle bin (SPEC 5.1)",
    ),
    (
        "notify",
        Scope::Only(&["inkra-workspace"]),
        "CC0-1.0 OR Artistic-2.0",
        "file watching (SPEC 2.1, FS-007)",
    ),
    (
        "notify-debouncer-mini",
        Scope::Only(&["inkra-workspace"]),
        "MIT OR Apache-2.0",
        "debounced watching (SPEC 2.1)",
    ),
    (
        "toml",
        Scope::Only(&["inkra-engine"]),
        "MIT OR Apache-2.0",
        "theme files (SPEC 2.1, TH-001)",
    ),
    (
        "rfd",
        Scope::Only(&["inkra-ui"]),
        "MIT",
        "native file dialogs (SPEC 2.1)",
    ),
    (
        "arboard",
        Scope::Only(&["inkra-ui"]),
        "MIT OR Apache-2.0",
        "clipboard with HTML and images (SPEC 2.1)",
    ),
    (
        "muda",
        Scope::Only(&["inkra-ui"]),
        "Apache-2.0 OR MIT",
        "macOS native menu (UI-001)",
    ),
    (
        "serde",
        Scope::Any,
        "MIT OR Apache-2.0",
        "configuration (SPEC 2.1)",
    ),
    (
        "serde_json",
        Scope::Any,
        "MIT OR Apache-2.0",
        "configuration (SPEC 2.1)",
    ),
    ("log", Scope::Any, "MIT OR Apache-2.0", "logging (SPEC 2.1)"),
    (
        "fern",
        Scope::Only(&["inkra"]),
        "MIT",
        "log sink (SPEC 2.1)",
    ),
    (
        "fluent-bundle",
        Scope::Only(&["inkra-i18n"]),
        "Apache-2.0 OR MIT",
        "i18n (SPEC 2.1)",
    ),
    (
        "unic-langid",
        Scope::Only(&["inkra-i18n"]),
        "MIT OR Apache-2.0",
        "language tags for fluent",
    ),
    (
        "sys-locale",
        Scope::Only(&["inkra-i18n"]),
        "MIT OR Apache-2.0",
        "system UI language (PR-018)",
    ),
    (
        "thiserror",
        Scope::Any,
        "MIT OR Apache-2.0",
        "error types (de-facto standard)",
    ),
    (
        "anyhow",
        Scope::Only(&["inkra", "inkra-cli", "xtask"]),
        "MIT OR Apache-2.0",
        "app-level errors only",
    ),
    (
        "tempfile",
        Scope::Any,
        "MIT OR Apache-2.0",
        "tests only (enforced: dev-dependency)",
    ),
    (
        "winresource",
        Scope::Only(&["inkra"]),
        "MIT",
        "embed inkra.ico on Windows (UI-050)",
    ),
    (
        "windows-sys",
        Scope::Only(&["inkra"]),
        "MIT OR Apache-2.0",
        "AttachConsole, AppUserModelID",
    ),
    (
        "html5ever",
        Scope::Only(&["inkra-html"]),
        "MIT OR Apache-2.0",
        "HTML parsing for paste/import (SPEC 2.1)",
    ),
    (
        "markup5ever_rcdom",
        Scope::Only(&["inkra-html"]),
        "MIT OR Apache-2.0",
        "DOM for html5ever (servo project)",
    ),
    (
        "regex",
        Scope::Any,
        "MIT OR Apache-2.0",
        "find/replace (SPEC 2.1, ED-110)",
    ),
    ("ropey", Scope::Any, "MIT", "document rope (SPEC 2.1)"),
    (
        "unicode-segmentation",
        Scope::Any,
        "MIT OR Apache-2.0",
        "grapheme/word boundaries (ED-011)",
    ),
    (
        "unicode-width",
        Scope::Any,
        "MIT OR Apache-2.0",
        "CJK display width for table alignment (ED-085)",
    ),
    (
        "encoding_rs",
        Scope::Only(&["inkra-text"]),
        "(Apache-2.0 OR MIT) AND BSD-3-Clause",
        "UTF-16/GBK/GB18030 (ED-106)",
    ),
    (
        "proptest",
        Scope::Any,
        "MIT OR Apache-2.0",
        "property tests (SPEC 2.1, TS-003)",
    ),
    (
        "criterion",
        Scope::Any,
        "Apache-2.0 OR MIT",
        "benchmarks (SPEC 2.1, TS-006)",
    ),
];

/// Whether `member` may depend on `dep`.
pub fn is_allowed(member: &str, dep: &str) -> bool {
    ALLOWED.iter().any(|(name, scope, _, _)| {
        *name == dep
            && match scope {
                Scope::Any => true,
                Scope::Only(list) => list.contains(&member),
            }
    })
}
