//! Command line parsing (CM-004).

use std::path::PathBuf;

/// Parsed command line.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Args {
    /// Files or a folder to open.
    pub paths: Vec<PathBuf>,
    /// `--new`: start with a new blank document.
    pub new: bool,
    /// `--source-mode`: start in source mode.
    pub source_mode: bool,
    /// `--theme <name>`.
    pub theme: Option<String>,
    /// `--config <dir>`: use `<dir>` as the root for config/data/cache.
    pub config: Option<PathBuf>,
    /// `--version`.
    pub version: bool,
    /// `--help`.
    pub help: bool,
    /// `--measure-startup`: print the time to the first presented frame and exit.
    pub measure_startup: bool,
    /// `--no-session`: do not restore or save the session.
    pub no_session: bool,
    /// `--control <port>`: open the control channel on a loopback port (CM-003;
    /// `0` picks a free one).
    pub control: Option<u16>,
}

/// Usage text.
pub const USAGE: &str = "\
Usage: inkra [OPTIONS] [FILES...|FOLDER]

Options:
      --new              Start with a new blank document
      --source-mode      Start in source mode
      --theme <NAME>     Use theme NAME
      --config <DIR>     Use DIR for configuration, data and cache
      --no-session       Do not restore or save the session
      --measure-startup  Print the time to the first frame and exit
      --control <PORT>   Accept automation on a loopback port (0: any free port)
  -V, --version          Print version
  -h, --help             Print help
";

/// Parses `args` (without the program name).
pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Args, String> {
    let mut out = Args::default();
    let mut it = args.into_iter();
    let mut only_paths = false;
    while let Some(a) = it.next() {
        if only_paths || !a.starts_with('-') || a == "-" {
            out.paths.push(PathBuf::from(a));
            continue;
        }
        let (flag, inline) = match a.split_once('=') {
            Some((f, v)) => (f.to_owned(), Some(v.to_owned())),
            None => (a.clone(), None),
        };
        let mut value = |name: &str| -> Result<String, String> {
            inline
                .clone()
                .or_else(|| it.next())
                .ok_or_else(|| format!("{name} requires a value"))
        };
        match flag.as_str() {
            "--" => only_paths = true,
            "--new" => out.new = true,
            "--source-mode" => out.source_mode = true,
            "--theme" => out.theme = Some(value("--theme")?),
            "--config" => out.config = Some(PathBuf::from(value("--config")?)),
            "--version" | "-V" => out.version = true,
            "--help" | "-h" => out.help = true,
            "--measure-startup" => out.measure_startup = true,
            "--no-session" => out.no_session = true,
            "--control" => {
                let v = value("--control")?;
                out.control = Some(
                    v.parse()
                        .map_err(|_| format!("--control needs a port number, not `{v}`"))?,
                );
            }
            // macOS passes a process serial number when launched from Finder.
            f if f.starts_with("-psn_") => {}
            other => return Err(format!("unknown option `{other}`")),
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(v: &[&str]) -> Result<Args, String> {
        parse(v.iter().map(|s| (*s).to_owned()))
    }

    #[test]
    fn parses_flags_and_paths() {
        let a = p(&[
            "--theme",
            "Ink",
            "a.md",
            "--config=/tmp/c",
            "--measure-startup",
            "--",
            "--x.md",
        ])
        .unwrap();
        assert_eq!(a.theme.as_deref(), Some("Ink"));
        assert_eq!(a.config, Some(PathBuf::from("/tmp/c")));
        assert!(a.measure_startup);
        assert_eq!(
            a.paths,
            vec![PathBuf::from("a.md"), PathBuf::from("--x.md")]
        );
        assert!(p(&["-V"]).unwrap().version);
        assert!(p(&["--bogus"]).is_err());
        assert!(p(&["--theme"]).is_err());
        assert_eq!(p(&["--control", "0"]).unwrap().control, Some(0));
        assert_eq!(p(&["--control=5123"]).unwrap().control, Some(5123));
        assert!(p(&["--control", "x"]).is_err());
    }
}
