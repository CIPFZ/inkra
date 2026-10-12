//! Opening and saving documents (FS-002, FS-003, ED-106, NF-003).

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use inkra_doc::Doc;
use inkra_markdown::ParseOptions;
use inkra_text::Encoding;

use crate::fs::atomic_write;

/// What we know about a file on disk (for external change detection, FS-007).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiskState {
    /// Modification time.
    pub mtime: Option<SystemTime>,
    /// Size in bytes.
    pub len: u64,
    /// Hash of the content.
    pub hash: u64,
    /// The file is not writable (FS-011).
    pub readonly: bool,
}

/// FNV-1a over bytes.
pub fn hash_bytes(b: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &x in b {
        h ^= u64::from(x);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// Errors from file operations.
#[derive(Debug, thiserror::Error)]
pub enum FileError {
    /// I/O error.
    #[error("{path}: {source}")]
    Io {
        /// Path.
        path: PathBuf,
        /// Cause.
        source: std::io::Error,
    },
}

/// Reads a document. `fallback` is the preferred legacy encoding (ED-106).
pub fn load(
    path: &Path,
    options: ParseOptions,
    fallback: Option<Encoding>,
) -> Result<(Doc, DiskState), FileError> {
    let bytes = std::fs::read(path).map_err(|source| FileError::Io {
        path: path.to_owned(),
        source,
    })?;
    let meta = std::fs::metadata(path).ok();
    let state = DiskState {
        mtime: meta.as_ref().and_then(|m| m.modified().ok()),
        len: bytes.len() as u64,
        hash: hash_bytes(&bytes),
        readonly: meta.as_ref().is_some_and(|m| m.permissions().readonly()),
    };
    Ok((Doc::from_bytes(&bytes, fallback, options), state))
}

/// Writes `doc` to `path` atomically, preserving permissions, encoding, BOM and
/// line endings.
pub fn save(path: &Path, doc: &Doc) -> Result<DiskState, FileError> {
    let bytes = doc.to_bytes();
    atomic_write(path, &bytes).map_err(|source| FileError::Io {
        path: path.to_owned(),
        source,
    })?;
    let meta = std::fs::metadata(path).ok();
    Ok(DiskState {
        mtime: meta.as_ref().and_then(|m| m.modified().ok()),
        len: bytes.len() as u64,
        hash: hash_bytes(&bytes),
        readonly: false,
    })
}

/// Reads the current disk state of `path` (`None` if it does not exist).
pub fn disk_state(path: &Path) -> Option<DiskState> {
    let bytes = std::fs::read(path).ok()?;
    let meta = std::fs::metadata(path).ok();
    Some(DiskState {
        mtime: meta.as_ref().and_then(|m| m.modified().ok()),
        len: bytes.len() as u64,
        hash: hash_bytes(&bytes),
        readonly: meta.as_ref().is_some_and(|m| m.permissions().readonly()),
    })
}

/// Default file name for an untitled document: its first heading, else
/// `Untitled` (FS-003).
pub fn suggested_name(doc: &Doc, untitled: &str, ext: &str) -> String {
    let blocks = doc.ast().to_blocks();
    let headings = inkra_markdown::walk::headings(doc.text(), &blocks);
    let base = headings
        .first()
        .map(|h| h.text.trim().to_owned())
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| untitled.to_owned());
    let clean: String = base
        .chars()
        .map(|c| {
            if "\\/:*?\"<>|".contains(c) || c.is_control() {
                ' '
            } else {
                c
            }
        })
        .collect();
    let clean = clean.trim().chars().take(80).collect::<String>();
    format!("{}.{ext}", if clean.is_empty() { untitled } else { &clean })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_and_load_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.md");
        std::fs::write(&p, b"# T\r\n\r\nx\r\n").unwrap();
        let (doc, st) = load(&p, Default::default(), None).unwrap();
        assert_eq!(doc.newline(), "\r\n");
        let st2 = save(&p, &doc).unwrap();
        assert_eq!(st.hash, st2.hash);
        assert_eq!(std::fs::read(&p).unwrap(), b"# T\r\n\r\nx\r\n");
        assert_eq!(suggested_name(&doc, "Untitled", "md"), "T.md");
        assert_eq!(
            suggested_name(&Doc::default(), "Untitled", "md"),
            "Untitled.md"
        );
    }
}
