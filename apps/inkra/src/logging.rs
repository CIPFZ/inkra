//! Logging setup (NF-008): `<data_dir>/logs/inkra.log`, three rotated files of at
//! most 16 MiB, level overridable with `RUST_LOG`. Document contents are never logged.

use std::path::Path;

use inkra_workspace::logfile::{DEFAULT_LIMIT, RotatingFile};
use log::LevelFilter;

/// Parsed `RUST_LOG`: a default level plus per-target overrides.
#[derive(Debug, Clone, PartialEq)]
pub struct Filter {
    /// Level for targets without a directive.
    pub default: LevelFilter,
    /// `(target prefix, level)` directives.
    pub targets: Vec<(String, LevelFilter)>,
}

/// Crate targets logged at `info` by default.
const OWN_TARGETS: &[&str] = &[
    "inkra",
    "inkra_text",
    "inkra_markdown",
    "inkra_doc",
    "inkra_highlight",
    "inkra_math",
    "inkra_diagram",
    "inkra_html",
    "inkra_layout",
    "inkra_render",
    "inkra_export",
    "inkra_workspace",
    "inkra_engine",
    "inkra_ui",
    "inkra_i18n",
];

impl Default for Filter {
    fn default() -> Self {
        Self {
            default: LevelFilter::Warn,
            targets: OWN_TARGETS
                .iter()
                .map(|t| ((*t).to_owned(), LevelFilter::Info))
                .collect(),
        }
    }
}

fn level(s: &str) -> Option<LevelFilter> {
    s.trim().parse().ok()
}

/// Parses a `RUST_LOG`-style spec such as `debug` or `warn,inkra_engine=trace`.
pub fn parse_filter(spec: &str) -> Filter {
    let mut f = Filter::default();
    for part in spec.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        match part.split_once('=') {
            Some((target, lvl)) => {
                if let Some(l) = level(lvl) {
                    f.targets.retain(|(t, _)| t != target);
                    f.targets.push((target.trim().to_owned(), l));
                }
            }
            None => {
                if let Some(l) = level(part) {
                    f.default = l;
                    f.targets
                        .retain(|(t, _)| !OWN_TARGETS.contains(&t.as_str()));
                }
            }
        }
    }
    f
}

/// Initializes logging to stderr and the rotating log file in `logs_dir`.
pub fn init(logs_dir: &Path) {
    let filter = std::env::var("RUST_LOG")
        .map(|s| parse_filter(&s))
        .unwrap_or_default();
    let mut dispatch = fern::Dispatch::new()
        .format(|out, message, record| {
            let t = inkra_workspace::time::Utc::now();
            out.finish(format_args!(
                "{}.{:03}Z {:5} {}: {}",
                t.compact(),
                t.millis,
                record.level(),
                record.target(),
                message
            ))
        })
        .level(filter.default);
    for (target, lvl) in &filter.targets {
        dispatch = dispatch.level_for(target.clone(), *lvl);
    }
    dispatch = dispatch.chain(std::io::stderr());
    match RotatingFile::open(logs_dir, DEFAULT_LIMIT) {
        Ok(file) => {
            dispatch = dispatch.chain(Box::new(file) as Box<dyn std::io::Write + Send>);
        }
        Err(e) => eprintln!("inkra: cannot open log file in {}: {e}", logs_dir.display()),
    }
    if let Err(e) = dispatch.apply() {
        eprintln!("inkra: logger already initialized: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters() {
        assert_eq!(parse_filter("debug").default, LevelFilter::Debug);
        let f = parse_filter("warn,inkra_engine=trace,bogus=xx");
        assert_eq!(f.default, LevelFilter::Warn);
        assert!(
            f.targets
                .contains(&("inkra_engine".into(), LevelFilter::Trace))
        );
        assert!(!f.targets.iter().any(|(t, _)| t == "bogus"));
    }
}
