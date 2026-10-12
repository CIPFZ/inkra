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
