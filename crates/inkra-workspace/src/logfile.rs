//! Size-limited rotating log file (NF-008).
//!
//! `<data_dir>/logs/inkra.log` is the current file; on startup and whenever it
//! exceeds the size limit it is rotated to `inkra.1.log`, `inkra.2.log`, keeping
//! three files in total.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Default per-file size limit: 16 MiB.
pub const DEFAULT_LIMIT: u64 = 16 * 1024 * 1024;
/// Number of files kept, including the current one.
pub const KEEP: usize = 3;

/// A writer that rotates the log file when it grows past `limit`.
#[derive(Debug)]
pub struct RotatingFile {
    dir: PathBuf,
    file: File,
    written: u64,
    limit: u64,
}

fn name(i: usize) -> String {
    if i == 0 {
        "inkra.log".to_owned()
    } else {
        format!("inkra.{i}.log")
    }
}

fn rotate(dir: &Path) -> io::Result<()> {
    let _ = fs::remove_file(dir.join(name(KEEP - 1)));
    for i in (0..KEEP - 1).rev() {
        let from = dir.join(name(i));
        if from.exists() {
            fs::rename(&from, dir.join(name(i + 1)))?;
        }
    }
    Ok(())
}

impl RotatingFile {
    /// Opens a fresh `inkra.log` in `dir`, rotating any previous one.
    pub fn open(dir: &Path, limit: u64) -> io::Result<Self> {
        fs::create_dir_all(dir)?;
        rotate(dir)?;
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(dir.join(name(0)))?;
        Ok(Self {
            dir: dir.to_owned(),
            file,
            written: 0,
            limit,
        })
    }

    /// Path of the current log file.
    pub fn current_path(&self) -> PathBuf {
        self.dir.join(name(0))
    }
}

impl Write for RotatingFile {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.written + buf.len() as u64 > self.limit && self.written > 0 {
            self.file.flush()?;
            rotate(&self.dir)?;
            self.file = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(self.dir.join(name(0)))?;
            self.written = 0;
        }
        let n = self.file.write(buf)?;
        self.written += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotates_and_keeps_three() {
        let dir = tempfile::tempdir().unwrap();
        for round in 0..5 {
            let mut f = RotatingFile::open(dir.path(), 64).unwrap();
            writeln!(f, "round {round}").unwrap();
        }
        let mut names: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        assert_eq!(names, ["inkra.1.log", "inkra.2.log", "inkra.log"]);
        assert_eq!(
            fs::read_to_string(dir.path().join("inkra.log")).unwrap(),
            "round 4\n"
        );
        assert_eq!(
            fs::read_to_string(dir.path().join("inkra.2.log")).unwrap(),
            "round 2\n"
        );
    }

    #[test]
    fn rotates_on_size() {
        let dir = tempfile::tempdir().unwrap();
        let mut f = RotatingFile::open(dir.path(), 10).unwrap();
        f.write_all(b"0123456789").unwrap();
        f.write_all(b"abc").unwrap();
        assert_eq!(
            fs::read_to_string(dir.path().join("inkra.log")).unwrap(),
            "abc"
        );
        assert_eq!(
            fs::read_to_string(dir.path().join("inkra.1.log")).unwrap(),
            "0123456789"
        );
    }
}
