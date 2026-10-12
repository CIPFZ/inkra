//! File system helpers: atomic writes (NF-003, FS-003, PR-001).

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn temp_path(target: &Path) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let name = target
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    target.with_file_name(format!(".{name}.inkra-tmp-{}-{n}", std::process::id()))
}

/// Writes `bytes` to `path` atomically: the data goes to a temporary file in the
/// same directory, is flushed to disk, and is then renamed over the target. The
/// target's permissions are preserved when it already exists. Parent directories
/// are created as needed.
///
/// Readers therefore see either the old or the new content, never a mix, even if
/// the process dies mid-write.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    let tmp = temp_path(path);
    let result = (|| {
        let mut f = OpenOptions::new().write(true).create_new(true).open(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        if let Ok(meta) = fs::metadata(path) {
            fs::set_permissions(&tmp, meta.permissions())?;
        }
        rename_over(&tmp, path)?;
        sync_parent(path);
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

fn rename_over(from: &Path, to: &Path) -> io::Result<()> {
    // On Windows a rename onto a file that is momentarily opened by another process
    // (virus scanner, indexer) fails with a sharing violation; retry briefly.
    let mut attempts = 0;
    loop {
        match fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(e) if cfg!(windows) && attempts < 10 => {
                attempts += 1;
                log::debug!("rename retry {attempts}: {e}");
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            Err(e) => return Err(e),
        }
    }
}

fn sync_parent(path: &Path) {
    #[cfg(unix)]
    if let Some(dir) = path.parent().and_then(|p| File::open(p).ok()) {
        let _ = dir.sync_all();
    }
    #[cfg(not(unix))]
    let _ = (path, File::open::<&Path>);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_and_replaces() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a/b/c.txt");
        atomic_write(&p, b"one").unwrap();
        assert_eq!(fs::read(&p).unwrap(), b"one");
        atomic_write(&p, b"two").unwrap();
        assert_eq!(fs::read(&p).unwrap(), b"two");
        let leftovers: Vec<_> = fs::read_dir(p.parent().unwrap()).unwrap().collect();
        assert_eq!(leftovers.len(), 1, "temporary file left behind");
    }

    #[cfg(unix)]
    #[test]
    fn preserves_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("x");
        fs::write(&p, "a").unwrap();
        fs::set_permissions(&p, fs::Permissions::from_mode(0o640)).unwrap();
        atomic_write(&p, b"b").unwrap();
        assert_eq!(
            fs::metadata(&p).unwrap().permissions().mode() & 0o777,
            0o640
        );
    }
}
