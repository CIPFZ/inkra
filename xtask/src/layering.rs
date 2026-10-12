//! `cargo xtask layering`: machine check of the SPEC 2.2 crate layering and of the
//! project's hard rules (iron rules 1, 4, 6 of the agent prompt).
//!
//! Data source is `cargo metadata`, which resolves `workspace = true` inheritance,
//! renames and target-specific dependencies for us.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde_json::Value;

use crate::deps;

/// Position of a workspace member in the layer stack.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    /// `(layer, sub-layer)`; a crate may depend on crates with a strictly lower pair.
    /// Sub-layers order crates inside one SPEC layer (`L1a` < `L1b`), see SPEC 2.2.
    At(u8, u8),
    /// Cross-cutting crate (`inkra-i18n`): any layer may depend on it, it depends on
    /// no internal crate.
    CrossCutting,
    /// Build tooling (`xtask`): may depend on any library crate, nothing depends on it.
    Tool,
}

/// The layer table. Mirror of SPEC 2.2; the single source for this check.
pub const LAYERS: &[(&str, Layer, &str)] = &[
    ("inkra-text", Layer::At(0, 0), "L0"),
    ("inkra-i18n", Layer::CrossCutting, "L0 (cross-cutting)"),
    ("inkra-markdown", Layer::At(1, 0), "L1a"),
    ("inkra-doc", Layer::At(1, 1), "L1b"),
    ("inkra-highlight", Layer::At(2, 0), "L2"),
    ("inkra-math", Layer::At(2, 0), "L2"),
    ("inkra-diagram", Layer::At(2, 0), "L2"),
    ("inkra-html", Layer::At(2, 0), "L2"),
    ("inkra-layout", Layer::At(3, 0), "L3a"),
    ("inkra-render", Layer::At(3, 1), "L3b"),
    ("inkra-export", Layer::At(4, 0), "L4"),
    ("inkra-workspace", Layer::At(4, 0), "L4"),
    ("inkra-engine", Layer::At(5, 0), "L5"),
    ("inkra-ui", Layer::At(6, 0), "L6"),
    ("inkra", Layer::At(7, 0), "app"),
    ("inkra-cli", Layer::At(7, 0), "app"),
    ("xtask", Layer::Tool, "tool"),
];

/// Explicitly forbidden internal edges on top of the layer rule.
const FORBIDDEN_EDGES: &[(&str, &str, &str)] = &[(
    "inkra-cli",
    "inkra-ui",
    "the CLI must run headless (CM-002)",
)];

/// Read-only Markdown parsers: allowed only as dev-dependencies (iron rule 4, AD-1).
pub const READ_ONLY_PARSERS: &[&str] = &["pulldown-cmark", "comrak", "markdown", "markdown-it"];

/// Test-only crates.
pub const DEV_ONLY: &[&str] = &["tempfile", "egui_kittest", "proptest", "criterion", "insta"];

/// Crates that would bring a web runtime into the product (iron rule 1, SPEC 1.5).
pub const WEB_TECH: &[&str] = &[
    "wry",
    "webview2-com",
    "webview2-com-sys",
    "webkit2gtk",
    "tauri",
    "tao",
    "wasm-bindgen",
    "js-sys",
    "web-sys",
    "deno_core",
    "boa_engine",
    "v8",
    "rquickjs",
    "quick-js",
    "servo",
    "cef",
    "ultralight",
];

/// Native targets whose full dependency graphs are checked for [`WEB_TECH`].
pub const TARGETS: &[&str] = &[
    "x86_64-pc-windows-msvc",
    "aarch64-pc-windows-msvc",
    "x86_64-apple-darwin",
    "aarch64-apple-darwin",
    "x86_64-unknown-linux-gnu",
    "aarch64-unknown-linux-gnu",
];

/// Members that may relax the workspace `unsafe_code = "forbid"` lint, and why.
pub const UNSAFE_EXCEPTIONS: &[(&str, &str)] = &[(
    "inkra",
    "Windows AttachConsole / AppUserModelID FFI in src/platform_windows.rs",
)];

fn layer_of(name: &str) -> Option<Layer> {
    LAYERS
        .iter()
        .find(|(n, _, _)| *n == name)
        .map(|(_, l, _)| *l)
}

/// Whether `from` may depend on `to` according to the layer rule.
pub fn edge_allowed(from: Layer, to: Layer) -> bool {
    match (from, to) {
        (Layer::CrossCutting, _) => false,
        (_, Layer::CrossCutting) => true,
        (_, Layer::Tool) => false,
        (Layer::Tool, _) => true,
        (Layer::At(a, b), Layer::At(c, d)) => (c, d) < (a, b) && !(c == a && d == b),
    }
}

/// A violation found by the check.
pub type Violation = String;

fn metadata(root: &Path, target: Option<&str>) -> Result<Value, String> {
    let mut cmd = std::process::Command::new(std::env::var("CARGO").unwrap_or("cargo".into()));
    cmd.current_dir(root)
        .args(["metadata", "--format-version", "1", "--locked"]);
    if let Some(t) = target {
        cmd.args(["--filter-platform", t]);
    }
    let out = cmd
        .output()
        .map_err(|e| format!("cannot run cargo metadata: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "cargo metadata failed: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    serde_json::from_slice(&out.stdout).map_err(|e| e.to_string())
}

/// Runs all checks; returns the list of violations.
pub fn check(root: &Path) -> Result<Vec<Violation>, String> {
    let meta = metadata(root, None)?;
    let mut v = Vec::new();
    let empty = Vec::new();
    let packages = meta["packages"].as_array().unwrap_or(&empty);
    let member_ids: BTreeSet<&str> = meta["workspace_members"]
        .as_array()
        .unwrap_or(&empty)
        .iter()
        .filter_map(Value::as_str)
        .collect();
    let members: Vec<&Value> = packages
        .iter()
        .filter(|p| p["id"].as_str().is_some_and(|id| member_ids.contains(id)))
        .collect();
    let member_names: BTreeSet<&str> = members.iter().filter_map(|p| p["name"].as_str()).collect();

    for name in &member_names {
        if layer_of(name).is_none() {
            v.push(format!(
                "{name}: not registered in the layer table (xtask/src/layering.rs)"
            ));
        }
    }
    for (name, _, _) in LAYERS {
        if !member_names.contains(name) {
            v.push(format!(
                "{name}: in the layer table but not a workspace member"
            ));
        }
    }

    for pkg in &members {
        let name = pkg["name"].as_str().unwrap_or_default();
        let Some(from) = layer_of(name) else { continue };
        for dep in pkg["dependencies"].as_array().unwrap_or(&empty) {
            let dname = dep["name"].as_str().unwrap_or_default();
            let kind = dep["kind"].as_str().unwrap_or("normal");
            if member_names.contains(dname) {
                let to = layer_of(dname).unwrap_or(Layer::Tool);
                if !edge_allowed(from, to) {
                    v.push(format!(
                        "{name} ({}) must not depend on {dname} ({}) [{kind}]",
                        label(name),
                        label(dname)
                    ));
                }
                for (f, t, why) in FORBIDDEN_EDGES {
                    if *f == name && *t == dname {
                        v.push(format!("{name} must not depend on {dname}: {why}"));
                    }
                }
            } else {
                if DEV_ONLY.contains(&dname) && kind != "dev" {
                    v.push(format!("{name}: `{dname}` may only be a dev-dependency"));
                }
                if READ_ONLY_PARSERS.contains(&dname) && kind != "dev" {
                    v.push(format!(
                        "{name}: `{dname}` may only be a dev-dependency (AD-1: self-written parser)"
                    ));
                }
                if WEB_TECH.contains(&dname) {
                    v.push(format!("{name}: `{dname}` is web technology (SPEC 1.5)"));
                }
                if !deps::is_allowed(name, dname) {
                    v.push(format!(
                        "{name}: dependency `{dname}` is not in the reviewed allowlist (xtask/src/deps.rs)"
                    ));
                }
            }
        }
        // Lints and layer docs are not in cargo metadata; read the files.
        let manifest = pkg["manifest_path"].as_str().unwrap_or_default();
        let toml = std::fs::read_to_string(manifest).unwrap_or_default();
        let inherits = toml.contains("[lints]") && toml.contains("workspace = true");
        match UNSAFE_EXCEPTIONS.iter().find(|(n, _)| *n == name) {
            Some(_) => {
                if !toml.contains("unsafe_code = \"deny\"") {
                    v.push(format!(
                        "{name}: unsafe exception must still set unsafe_code = \"deny\""
                    ));
                }
            }
            None => {
                if !inherits {
                    v.push(format!(
                        "{name}: must use `[lints] workspace = true` (forbid unsafe_code)"
                    ));
                }
            }
        }
        if let Some(dir) = Path::new(manifest).parent() {
            let lib = dir.join("src/lib.rs");
            if lib.exists() {
                let text = std::fs::read_to_string(&lib).unwrap_or_default();
                let expected = format!("**Layer {}**", label(name));
                if !text.lines().take(10).any(|l| l.contains(&expected)) {
                    v.push(format!(
                        "{name}: src/lib.rs header must document `{expected}`"
                    ));
                }
            }
        }
    }

    // Full resolved graph per native target: no web runtime anywhere, no read-only
    // Markdown parser in the non-dev graph of the shipped binaries.
    for target in TARGETS {
        let meta = metadata(root, Some(target))?;
        let names = resolved_names(&meta);
        for bad in WEB_TECH {
            if names.contains(*bad) {
                v.push(format!(
                    "{target}: dependency graph contains web technology `{bad}`"
                ));
            }
        }
        let shipped = normal_closure(&meta, &["inkra", "inkra-cli"]);
        for bad in READ_ONLY_PARSERS {
            if shipped.contains(*bad) {
                v.push(format!(
                    "{target}: shipped binaries depend on read-only parser `{bad}`"
                ));
            }
        }
    }
    Ok(v)
}

fn label(name: &str) -> &'static str {
    LAYERS
        .iter()
        .find(|(n, _, _)| *n == name)
        .map(|(_, _, l)| *l)
        .unwrap_or("?")
}

fn resolved_names(meta: &Value) -> BTreeSet<String> {
    let empty = Vec::new();
    let ids: BTreeSet<&str> = meta["resolve"]["nodes"]
        .as_array()
        .unwrap_or(&empty)
        .iter()
        .filter_map(|n| n["id"].as_str())
        .collect();
    meta["packages"]
        .as_array()
        .unwrap_or(&empty)
        .iter()
        .filter(|p| p["id"].as_str().is_some_and(|id| ids.contains(id)))
        .filter_map(|p| p["name"].as_str().map(str::to_owned))
        .collect()
}

/// Names of all packages reachable from `roots` through normal/build edges.
fn normal_closure(meta: &Value, roots: &[&str]) -> BTreeSet<String> {
    let empty = Vec::new();
    let mut name_of = BTreeMap::new();
    for p in meta["packages"].as_array().unwrap_or(&empty) {
        if let (Some(id), Some(n)) = (p["id"].as_str(), p["name"].as_str()) {
            name_of.insert(id.to_owned(), n.to_owned());
        }
    }
    let mut edges: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for n in meta["resolve"]["nodes"].as_array().unwrap_or(&empty) {
        let id = n["id"].as_str().unwrap_or_default().to_owned();
        let deps = n["deps"]
            .as_array()
            .unwrap_or(&empty)
            .iter()
            .filter(|d| {
                d["dep_kinds"]
                    .as_array()
                    .unwrap_or(&empty)
                    .iter()
                    .any(|k| !k["kind"].is_string() || k["kind"] == "build")
            })
            .filter_map(|d| d["pkg"].as_str().map(str::to_owned))
            .collect();
        edges.insert(id, deps);
    }
    let mut stack: Vec<String> = name_of
        .iter()
        .filter(|(_, n)| roots.contains(&n.as_str()))
        .map(|(id, _)| id.clone())
        .collect();
    let mut seen = BTreeSet::new();
    while let Some(id) = stack.pop() {
        if !seen.insert(id.clone()) {
            continue;
        }
        stack.extend(edges.get(&id).cloned().unwrap_or_default());
    }
    seen.iter()
        .filter_map(|id| name_of.get(id).cloned())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layer_rule() {
        let l = |n| layer_of(n).unwrap();
        assert!(edge_allowed(l("inkra-doc"), l("inkra-markdown")));
        assert!(!edge_allowed(l("inkra-markdown"), l("inkra-doc")));
        assert!(edge_allowed(l("inkra-render"), l("inkra-layout")));
        assert!(
            !edge_allowed(l("inkra-math"), l("inkra-html")),
            "same layer"
        );
        assert!(
            !edge_allowed(l("inkra-export"), l("inkra-workspace")),
            "same layer"
        );
        assert!(!edge_allowed(l("inkra-text"), l("inkra-ui")), "upwards");
        assert!(edge_allowed(l("inkra-text"), l("inkra-i18n")));
        assert!(!edge_allowed(l("inkra-i18n"), l("inkra-text")));
        assert!(edge_allowed(l("xtask"), l("inkra-engine")));
        assert!(!edge_allowed(l("inkra"), l("xtask")));
        assert!(
            !edge_allowed(l("inkra"), l("inkra-cli")),
            "apps are one layer"
        );
    }
}
