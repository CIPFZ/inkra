//! Image insertion targets (ED-092): where pasted bitmaps and inserted files go.

use std::path::{Path, PathBuf};

use crate::fs::atomic_write;
use crate::time::Utc;

/// What to do with inserted images (ED-092 "插入图片时").
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InsertAction {
    /// Use the original path or URL.
    None,
    /// Copy to `./${filename}.assets/`.
    CopyToAssets,
    /// Copy to a folder pattern (`${filename}`, `${filepath}`; relative to the
    /// document directory or absolute).
    CopyToFolder(String),
}

impl InsertAction {
    /// From preference values.
    pub fn from_pref(action: &str, folder: &str) -> Self {
        match action {
            "none" => Self::None,
            "folder" if !folder.trim().is_empty() => Self::CopyToFolder(folder.to_owned()),
            _ => Self::CopyToAssets,
        }
    }
}

/// The folder images are copied to for document `doc` (or, for untitled
/// documents, `fallback`).
pub fn target_dir(doc: Option<&Path>, action: &InsertAction, fallback: &Path) -> Option<PathBuf> {
    let Some(doc) = doc else {
        return (*action != InsertAction::None).then(|| fallback.to_owned());
    };
    let dir = doc.parent().unwrap_or(Path::new("."));
    let stem = doc
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    match action {
        InsertAction::None => None,
        InsertAction::CopyToAssets => Some(dir.join(format!("{stem}.assets"))),
        InsertAction::CopyToFolder(pat) => {
            let p = pat
                .replace("${filename}", &stem)
                .replace("${filepath}", &dir.to_string_lossy());
            let p = PathBuf::from(p);
            Some(if p.is_absolute() { p } else { dir.join(p) })
        }
    }
}

/// `image-YYYYMMDDHHmmssSSS.png` (ED-092).
pub fn pasted_name() -> String {
    format!("image-{}.png", Utc::now().compact_millis())
}

/// Writes PNG bytes into `dir` under a unique name based on `name`.
pub fn write_unique(dir: &Path, name: &str, bytes: &[u8]) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let mut p = dir.join(name);
    let stem = Path::new(name)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ext = Path::new(name)
        .extension()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut n = 1;
    while p.exists() {
        p = dir.join(format!("{stem}-{n}.{ext}"));
        n += 1;
    }
    atomic_write(&p, bytes)?;
    Ok(p)
}

/// Copies an image file into `dir` (unless it is already there).
pub fn copy_in(dir: &Path, src: &Path) -> std::io::Result<PathBuf> {
    if src.parent() == Some(dir) {
        return Ok(src.to_owned());
    }
    let name = src
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(pasted_name);
    let bytes = std::fs::read(src)?;
    write_unique(dir, &name, &bytes)
}

/// The Markdown source path for `path` as seen from document `doc` (ED-092:
/// relative when possible, `/` separators, spaces escaped).
pub fn markdown_src(doc: Option<&Path>, path: &Path, prefer_relative: bool) -> String {
    let rel = if prefer_relative {
        doc.and_then(|d| d.parent())
            .and_then(|dir| relative(dir, path))
            .unwrap_or_else(|| path.to_owned())
    } else {
        path.to_owned()
    };
    let s = rel.to_string_lossy().replace('\\', "/");
    s.replace(' ', "%20")
}

fn relative(from_dir: &Path, to: &Path) -> Option<PathBuf> {
    let from: Vec<_> = from_dir.components().collect();
    let target: Vec<_> = to.components().collect();
    let common = from.iter().zip(&target).take_while(|(a, b)| a == b).count();
    if common == 0 {
        return None;
    }
    let mut out = PathBuf::new();
    for _ in common..from.len() {
        out.push("..");
    }
    for c in &target[common..] {
        out.push(c);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn targets_and_relative_paths() {
        let doc = Path::new("/notes/my doc.md");
        let fb = Path::new("/data/images");
        assert_eq!(
            target_dir(Some(doc), &InsertAction::CopyToAssets, fb),
            Some(PathBuf::from("/notes/my doc.assets"))
        );
        assert_eq!(
            target_dir(
                Some(doc),
                &InsertAction::CopyToFolder("img/${filename}".into()),
                fb
            ),
            Some(PathBuf::from("/notes/img/my doc"))
        );
        assert_eq!(
            target_dir(None, &InsertAction::CopyToAssets, fb),
            Some(fb.to_owned())
        );
        assert_eq!(target_dir(Some(doc), &InsertAction::None, fb), None);
        assert_eq!(
            markdown_src(Some(doc), Path::new("/notes/my doc.assets/a.png"), true),
            "my%20doc.assets/a.png"
        );
        assert_eq!(
            markdown_src(Some(doc), Path::new("/pics/b.png"), true),
            "../pics/b.png"
        );
        assert!(pasted_name().starts_with("image-2"));
        let dir = tempfile::tempdir().unwrap();
        let a = write_unique(dir.path(), "x.png", b"1").unwrap();
        let b = write_unique(dir.path(), "x.png", b"2").unwrap();
        assert_ne!(a, b);
    }
}
