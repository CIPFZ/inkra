//! Fallback to an external Mermaid CLI (`mmdc`) for diagram types the built-in
//! renderer does not support (SPEC 2.4): the user configures the executable;
//! diagrams are rendered to SVG files in a cache directory on background
//! threads.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock, RwLock};

/// State of an external rendering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    /// Running.
    Pending,
    /// The SVG file.
    Ready(PathBuf),
    /// `mmdc` failed.
    Failed(String),
}

type Waker = Box<dyn Fn() + Send + Sync>;

static CONFIG: RwLock<Option<(PathBuf, PathBuf)>> = RwLock::new(None);
static JOBS: OnceLock<Mutex<HashMap<u64, Status>>> = OnceLock::new();
static WAKE: OnceLock<Mutex<Option<Waker>>> = OnceLock::new();
static GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Changes whenever a rendering finishes.
pub fn generation() -> u64 {
    GENERATION.load(std::sync::atomic::Ordering::SeqCst)
}

/// Every finished SVG (at most 64): layouts hash their loaded sizes so they
/// refresh when an image finishes loading.
pub fn ready_paths() -> Vec<PathBuf> {
    let Some(j) = JOBS.get() else {
        return Vec::new();
    };
    let Ok(j) = j.lock() else { return Vec::new() };
    j.values()
        .filter_map(|s| {
            if let Status::Ready(p) = s {
                Some(p.clone())
            } else {
                None
            }
        })
        .take(64)
        .collect()
}

/// The SVG of `src` if it is already rendered (never starts a job).
pub fn peek(src: &str, dark: bool) -> Option<PathBuf> {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (src, dark).hash(&mut h);
    match JOBS.get()?.lock().ok()?.get(&h.finish())? {
        Status::Ready(p) => Some(p.clone()),
        _ => None,
    }
}

/// Sets the `mmdc` executable (`None`: disabled) and the cache directory.
pub fn configure(mmdc: Option<PathBuf>, cache: PathBuf) {
    if let Ok(mut c) = CONFIG.write() {
        *c = mmdc
            .filter(|p| !p.as_os_str().is_empty())
            .map(|m| (m, cache));
    }
}

/// Whether an external renderer is configured.
pub fn enabled() -> bool {
    CONFIG.read().is_ok_and(|c| c.is_some())
}

/// Called (from a worker thread) when a rendering finishes.
pub fn on_ready(f: impl Fn() + Send + Sync + 'static) {
    if let Ok(mut w) = WAKE.get_or_init(Default::default).lock() {
        *w = Some(Box::new(f));
    }
}

/// The status of rendering `src` (starting it if needed); `None` when no
/// external renderer is configured.
pub fn request(src: &str, dark: bool) -> Option<Status> {
    let (mmdc, cache) = CONFIG.read().ok()?.clone()?;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (src, dark).hash(&mut h);
    let key = h.finish();
    let jobs = JOBS.get_or_init(Default::default);
    if let Some(s) = jobs.lock().ok()?.get(&key) {
        return Some(s.clone());
    }
    let svg = cache.join(format!("{key:016x}.svg"));
    if svg.is_file() {
        let st = Status::Ready(svg);
        jobs.lock().ok()?.insert(key, st.clone());
        return Some(st);
    }
    jobs.lock().ok()?.insert(key, Status::Pending);
    let src = src.to_owned();
    std::thread::Builder::new()
        .name("inkra-mmdc".into())
        .spawn(move || {
            let result = run(&mmdc, &cache, key, &src, dark);
            if let Some(j) = JOBS.get()
                && let Ok(mut j) = j.lock()
            {
                j.insert(key, result);
            }
            GENERATION.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if let Some(w) = WAKE.get()
                && let Ok(w) = w.lock()
                && let Some(f) = w.as_ref()
            {
                f();
            }
        })
        .ok()?;
    Some(Status::Pending)
}

fn run(mmdc: &std::path::Path, cache: &std::path::Path, key: u64, src: &str, dark: bool) -> Status {
    if let Err(e) = std::fs::create_dir_all(cache) {
        return Status::Failed(e.to_string());
    }
    let input = cache.join(format!("{key:016x}.mmd"));
    let output = cache.join(format!("{key:016x}.svg"));
    let config = cache.join("mmdc-config.json");
    // SVG text instead of HTML labels, which SVG renderers cannot draw.
    let _ = std::fs::write(
        &config,
        r#"{"htmlLabels":false,"flowchart":{"htmlLabels":false}}"#,
    );
    if let Err(e) = std::fs::write(&input, src) {
        return Status::Failed(e.to_string());
    }
    let out = std::process::Command::new(mmdc)
        .arg("-i")
        .arg(&input)
        .arg("-o")
        .arg(&output)
        .args([
            "-b",
            "transparent",
            "-t",
            if dark { "dark" } else { "default" },
        ])
        .arg("-c")
        .arg(&config)
        .output();
    let _ = std::fs::remove_file(&input);
    match out {
        Ok(o) if o.status.success() && output.is_file() => Status::Ready(output),
        Ok(o) => Status::Failed(
            String::from_utf8_lossy(&o.stderr)
                .lines()
                .last()
                .unwrap_or("mmdc failed")
                .to_owned(),
        ),
        Err(e) => Status::Failed(format!("{}: {e}", mmdc.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_without_configuration_and_reports_missing_executable() {
        configure(None, std::env::temp_dir());
        assert!(request("gantt\n  title x", false).is_none());
        let dir = tempfile::tempdir().unwrap();
        configure(
            Some(PathBuf::from("/nonexistent/mmdc")),
            dir.path().to_owned(),
        );
        assert_eq!(request("gantt\n  title y", false), Some(Status::Pending));
        for _ in 0..100 {
            if let Some(Status::Failed(_)) = request("gantt\n  title y", false) {
                configure(None, std::env::temp_dir());
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("no failure reported");
    }
}
