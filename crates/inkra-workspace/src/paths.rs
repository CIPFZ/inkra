//! Platform directories (PR-001, SPEC appendix B).

use std::path::{Path, PathBuf};

/// Directories used by the application.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    /// Configuration: `conf.user.json`, `keybindings.json`, `themes/`, `dictionary.txt`.
    pub config: PathBuf,
    /// Data: `recovery/`, `logs/`, session.
    pub data: PathBuf,
    /// Disposable cache (downloaded images, thumbnails).
    pub cache: PathBuf,
}

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

fn home() -> PathBuf {
    env_path("HOME")
        .or_else(|| env_path("USERPROFILE"))
        .unwrap_or_else(|| PathBuf::from("."))
}

impl Paths {
    /// The platform default directories.
    ///
    /// | | Windows | macOS | Linux |
    /// |---|---|---|---|
    /// | config | `%APPDATA%\Inkra` | `~/Library/Application Support/Inkra` | `$XDG_CONFIG_HOME/inkra` |
    /// | data | `%LOCALAPPDATA%\Inkra` | `~/Library/Application Support/Inkra` | `$XDG_DATA_HOME/inkra` |
    /// | cache | `%LOCALAPPDATA%\Inkra\cache` | `~/Library/Caches/Inkra` | `$XDG_CACHE_HOME/inkra` |
    pub fn platform() -> Self {
        if cfg!(windows) {
            let roaming = env_path("APPDATA").unwrap_or_else(|| home().join("AppData/Roaming"));
            let local = env_path("LOCALAPPDATA").unwrap_or_else(|| home().join("AppData/Local"));
            Self {
                config: roaming.join("Inkra"),
                data: local.join("Inkra"),
                cache: local.join("Inkra").join("cache"),
            }
        } else if cfg!(target_os = "macos") {
            let support = home().join("Library/Application Support/Inkra");
            Self {
                config: support.clone(),
                data: support,
                cache: home().join("Library/Caches/Inkra"),
            }
        } else {
            let h = home();
            Self {
                config: env_path("XDG_CONFIG_HOME")
                    .unwrap_or_else(|| h.join(".config"))
                    .join("inkra"),
                data: env_path("XDG_DATA_HOME")
                    .unwrap_or_else(|| h.join(".local/share"))
                    .join("inkra"),
                cache: env_path("XDG_CACHE_HOME")
                    .unwrap_or_else(|| h.join(".cache"))
                    .join("inkra"),
            }
        }
    }

    /// All directories under one root (used by `--config <dir>` and tests).
    pub fn rooted(root: impl AsRef<Path>) -> Self {
        let root = root.as_ref();
        Self {
            config: root.join("config"),
            data: root.join("data"),
            cache: root.join("cache"),
        }
    }

    /// `conf.user.json`.
    pub fn preferences_file(&self) -> PathBuf {
        self.config.join("conf.user.json")
    }

    /// `keybindings.json` (UI-020).
    pub fn keybindings_file(&self) -> PathBuf {
        self.config.join("keybindings.json")
    }

    /// User theme directory (TH-003).
    pub fn themes_dir(&self) -> PathBuf {
        self.config.join("themes")
    }

    /// Log directory (NF-008).
    pub fn logs_dir(&self) -> PathBuf {
        self.data.join("logs")
    }

    /// Recovery directory (FS-004).
    pub fn recovery_dir(&self) -> PathBuf {
        self.data.join("recovery")
    }

    /// Session file (FS-006).
    pub fn session_file(&self) -> PathBuf {
        self.data.join("session.json")
    }

    /// Custom spelling dictionary.
    pub fn dictionary_file(&self) -> PathBuf {
        self.config.join("dictionary.txt")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_paths_end_with_app_dir() {
        let p = Paths::platform();
        let name = p
            .config
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_lowercase();
        assert_eq!(name, "inkra");
        assert!(p.preferences_file().ends_with("conf.user.json"));
    }

    #[test]
    fn rooted() {
        let p = Paths::rooted("/tmp/x");
        assert_eq!(p.logs_dir(), PathBuf::from("/tmp/x/data/logs"));
    }
}

/// Canonical form of `p` for comparing and displaying open files: symlinks and
/// `..` resolved, and on Windows without the `\\?\` verbatim prefix when the
/// path is representable without it. Falls back to `p` if it does not exist.
pub fn canonical(p: &Path) -> PathBuf {
    let c = match std::fs::canonicalize(p) {
        Ok(c) => c,
        Err(_) => return p.to_owned(),
    };
    simplify_verbatim(c)
}

fn simplify_verbatim(c: PathBuf) -> PathBuf {
    let s = c.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\") {
        // `\\?\UNC\server\share` → `\\server\share`; `\\?\C:\x` → `C:\x`.
        if let Some(unc) = rest.strip_prefix(r"UNC\") {
            return PathBuf::from(format!(r"\\{unc}"));
        }
        let drive = rest.as_bytes();
        let plain = drive.len() >= 3
            && drive[0].is_ascii_alphabetic()
            && drive[1] == b':'
            && drive[2] == b'\\';
        // Long paths and reserved names need the prefix.
        if plain
            && rest.len() < 260
            && !rest
                .split('\\')
                .any(|c| c.ends_with('.') || c.ends_with(' '))
        {
            return PathBuf::from(rest);
        }
    }
    c
}

#[cfg(test)]
mod canonical_tests {
    use super::*;

    #[test]
    fn strips_verbatim_prefix() {
        assert_eq!(
            simplify_verbatim(PathBuf::from(r"\\?\C:\work\a.md")),
            PathBuf::from(r"C:\work\a.md")
        );
        assert_eq!(
            simplify_verbatim(PathBuf::from(r"\\?\UNC\srv\share\a.md")),
            PathBuf::from(r"\\srv\share\a.md")
        );
        assert_eq!(
            simplify_verbatim(PathBuf::from("/tmp/a.md")),
            PathBuf::from("/tmp/a.md")
        );
    }
}
