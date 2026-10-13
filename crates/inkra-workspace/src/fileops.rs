//! File operations from the sidebar (FS-022): new file/folder, rename, move,
//! move to the recycle bin.

use std::path::{Path, PathBuf};

/// Errors from file operations.
#[derive(Debug, thiserror::Error)]
pub enum OpError {
    /// The target exists.
    #[error("{0} already exists")]
    Exists(PathBuf),
    /// Invalid name.
    #[error("invalid name: {0}")]
    BadName(String),
    /// I/O.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// Recycle bin.
    #[error("cannot move to the recycle bin: {0}")]
    Trash(String),
}

fn check_name(name: &str) -> Result<(), OpError> {
    let bad = name.trim().is_empty()
        || name.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|'])
        || name == "."
        || name == "..";
    if bad {
        Err(OpError::BadName(name.to_owned()))
    } else {
        Ok(())
    }
}

/// A free name in `dir` based on `name` (`Untitled.md`, `Untitled 2.md`, …).
pub fn unique_name(dir: &Path, name: &str) -> PathBuf {
    let p = dir.join(name);
    if !p.exists() {
        return p;
    }
    let path = Path::new(name);
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ext = path
        .extension()
        .map(|s| format!(".{}", s.to_string_lossy()))
        .unwrap_or_default();
    (2..)
        .map(|n| dir.join(format!("{stem} {n}{ext}")))
        .find(|p| !p.exists())
        .unwrap_or(p)
}

/// Creates an empty Markdown file with a free name in `dir`.
pub fn new_file(dir: &Path, name: &str) -> Result<PathBuf, OpError> {
    check_name(name)?;
    let p = unique_name(dir, name);
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&p)?;
    Ok(p)
}

/// Creates a folder with a free name in `dir`.
pub fn new_folder(dir: &Path, name: &str) -> Result<PathBuf, OpError> {
    check_name(name)?;
    let p = unique_name(dir, name);
    std::fs::create_dir(&p)?;
    Ok(p)
}

/// Renames `path` to `new_name` in the same folder (no overwrite).
pub fn rename(path: &Path, new_name: &str) -> Result<PathBuf, OpError> {
    check_name(new_name)?;
    let target = path.parent().unwrap_or(Path::new(".")).join(new_name);
    if target == path {
        return Ok(target);
    }
    // Case-only renames on case-insensitive file systems are fine.
    let same_file = target.exists()
        && target.to_string_lossy().to_lowercase() == path.to_string_lossy().to_lowercase();
    if target.exists() && !same_file {
        return Err(OpError::Exists(target));
    }
    std::fs::rename(path, &target)?;
    Ok(target)
}

/// Moves `path` into folder `dest` (no overwrite; not into itself).
pub fn move_into(path: &Path, dest: &Path) -> Result<PathBuf, OpError> {
    let name = path
        .file_name()
        .ok_or_else(|| OpError::BadName(path.display().to_string()))?;
    let target = dest.join(name);
    if target == path {
        return Ok(target);
    }
    if dest.starts_with(path) {
        return Err(OpError::BadName(format!("{} into itself", path.display())));
    }
    if target.exists() {
        return Err(OpError::Exists(target));
    }
    match std::fs::rename(path, &target) {
        Ok(()) => Ok(target),
        Err(_) if path.is_file() => {
            // Across volumes: copy then delete.
            std::fs::copy(path, &target)?;
            std::fs::remove_file(path)?;
            Ok(target)
        }
        Err(e) => Err(e.into()),
    }
}

/// Moves `path` to the system recycle bin.
pub fn to_trash(path: &Path) -> Result<(), OpError> {
    trash::delete(path).map_err(|e| OpError::Trash(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_rename_move() {
        let d = tempfile::tempdir().unwrap();
        let r = d.path();
        let a = new_file(r, "Untitled.md").unwrap();
        let b = new_file(r, "Untitled.md").unwrap();
        assert_eq!(b.file_name().unwrap(), "Untitled 2.md");
        assert!(matches!(rename(&b, "Untitled.md"), Err(OpError::Exists(_))));
        let c = rename(&b, "notes.md").unwrap();
        let sub = new_folder(r, "sub").unwrap();
        let moved = move_into(&c, &sub).unwrap();
        assert!(moved.exists() && !c.exists());
        assert!(move_into(&sub, &sub).is_err());
        assert!(rename(&a, "bad/name").is_err());
    }
}
