//! Inkra build tooling. Run with `cargo xtask <task>`.

mod deps;
mod dist;
mod docgen;
mod layering;

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const USAGE: &str = "\
Usage: cargo xtask <TASK>

Tasks:
  ci                 fmt --check, clippy -D warnings, test, layering, generated docs check
  layering           Check crate layering and dependency rules (SPEC 2.2)
  commands [--check] Generate docs/commands.md from the command registry
  prefs [--check]    Generate docs/preferences.md from the preference table
  keybindings [--check]  Generate docs/keybindings.md from the default key map
  docs [--check]     All of the above generators
  bundle-macos [--release]  Assemble target/<profile>/Inkra.app
  dist               Release packages for this platform in target/dist (NF-006)
  corpus             Fetch the real-world Markdown corpus into target/corpus (TS-002)
";

/// Real-world Markdown corpus (TS-002): `(name, git url, pinned commit)`. Not
/// vendored; fetched on demand. All are MIT/Apache or CC licensed documentation.
const CORPUS: &[(&str, &str, &str)] = &[
    (
        "book",
        "https://github.com/rust-lang/book.git",
        "1500248d8f230566e4ec9f27fcbb8fe9e2898ab1",
    ),
    (
        "rust-by-example",
        "https://github.com/rust-lang/rust-by-example.git",
        "dcc14f912bf8790792a2df082eddc82ab2539229",
    ),
    (
        "reference",
        "https://github.com/rust-lang/reference.git",
        "5baa209fc1998b8425be2ea1e8a57b613fed52d2",
    ),
    (
        "nomicon",
        "https://github.com/rust-lang/nomicon.git",
        "46fc44214afcd159be175c8ee62887cad04f615d",
    ),
    (
        "cargo",
        "https://github.com/rust-lang/cargo.git",
        "9d5a15d175a3073a185cb344fa476ab81cf82554",
    ),
    (
        "rust-course",
        "https://github.com/sunface/rust-course.git",
        "ebe2d82437f621085b0cb6895edbd1283ed63cb1",
    ),
    (
        "trpl-zh-cn",
        "https://github.com/KaiserY/trpl-zh-cn.git",
        "108a23ad7dc20f9267933ea109c248ec9d5602bb",
    ),
];

fn corpus() -> Result<(), String> {
    let dir = root().join("target/corpus");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    for (name, url, rev) in CORPUS {
        let repo = dir.join(name);
        if !repo.exists() {
            let mut init = Command::new("git");
            init.args(["init", "-q"]).arg(&repo);
            run(&format!("git init {name}"), init)?;
            let mut fetch = Command::new("git");
            fetch
                .current_dir(&repo)
                .args(["fetch", "-q", "--depth", "1", url, rev]);
            run(&format!("git fetch {name}"), fetch)?;
            let mut co = Command::new("git");
            co.current_dir(&repo).args(["checkout", "-q", "FETCH_HEAD"]);
            run(&format!("git checkout {name}"), co)?;
        }
    }
    eprintln!("corpus ready in {}", dir.display());
    Ok(())
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default()
}

fn cargo() -> Command {
    let mut c = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()));
    c.current_dir(root());
    c
}

fn run(step: &str, mut cmd: Command) -> Result<(), String> {
    eprintln!("==> {step}");
    let status = cmd.status().map_err(|e| format!("{step}: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{step} failed ({status})"))
    }
}

fn layering() -> Result<(), String> {
    eprintln!("==> layering");
    let violations = layering::check(&root())?;
    if violations.is_empty() {
        eprintln!("layering: OK ({} crates)", layering::LAYERS.len());
        Ok(())
    } else {
        for v in &violations {
            eprintln!("layering: {v}");
        }
        Err(format!("{} layering violation(s)", violations.len()))
    }
}

/// Writes or checks one generated file.
fn generated(rel: &str, content: String, check: bool) -> Result<(), String> {
    let path = root().join(rel);
    if check {
        let current = std::fs::read_to_string(&path).unwrap_or_default();
        if current.replace("\r\n", "\n") != content {
            return Err(format!("{rel} is out of date; run `cargo xtask docs`"));
        }
        eprintln!("{rel}: up to date");
    } else {
        std::fs::write(&path, content).map_err(|e| format!("{rel}: {e}"))?;
        eprintln!("wrote {rel}");
    }
    Ok(())
}

fn docs(which: &str, check: bool) -> Result<(), String> {
    if matches!(which, "commands" | "docs") {
        generated("docs/commands.md", docgen::commands(), check)?;
    }
    if matches!(which, "prefs" | "docs") {
        generated("docs/preferences.md", docgen::preferences(), check)?;
    }
    if matches!(which, "keybindings" | "docs") {
        generated("docs/keybindings.md", docgen::keybindings(), check)?;
    }
    Ok(())
}

fn ci() -> Result<(), String> {
    let mut fmt = cargo();
    fmt.args(["fmt", "--all", "--", "--check"]);
    run("fmt", fmt)?;
    let mut clippy = cargo();
    clippy.args([
        "clippy",
        "--workspace",
        "--all-targets",
        "--locked",
        "--",
        "-D",
        "warnings",
    ]);
    run("clippy", clippy)?;
    let mut test = cargo();
    test.args(["test", "--workspace", "--locked"]);
    run("test", test)?;
    layering()?;
    eprintln!("==> generated docs");
    docs("docs", true)?;
    eprintln!("ci: all checks passed");
    Ok(())
}

fn bundle_macos(release: bool) -> Result<(), String> {
    let profile = if release { "release" } else { "debug" };
    let mut build = cargo();
    build.args(["build", "-p", "inkra"]);
    if release {
        build.arg("--release");
    }
    run("build", build)?;
    let root = root();
    let target = root.join("target").join(profile);
    let app = target.join("Inkra.app/Contents");
    let io = |e: std::io::Error| e.to_string();
    std::fs::create_dir_all(app.join("MacOS")).map_err(io)?;
    std::fs::create_dir_all(app.join("Resources")).map_err(io)?;
    let plist = std::fs::read_to_string(root.join("apps/inkra/macos/Info.plist"))
        .map_err(io)?
        .replace("$VERSION", env!("CARGO_PKG_VERSION"));
    std::fs::write(app.join("Info.plist"), plist).map_err(io)?;
    std::fs::copy(target.join("inkra"), app.join("MacOS/inkra")).map_err(io)?;
    std::fs::copy(
        root.join("assets/brand/inkra.icns"),
        app.join("Resources/inkra.icns"),
    )
    .map_err(io)?;
    eprintln!("bundled {}", target.join("Inkra.app").display());
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let check = args.iter().any(|a| a == "--check");
    let result = match args.first().map(String::as_str) {
        Some("ci") => ci(),
        Some("layering") => layering(),
        Some(t @ ("commands" | "prefs" | "keybindings" | "docs")) => docs(t, check),
        Some("bundle-macos") => bundle_macos(args.iter().any(|a| a == "--release")),
        Some("corpus") => corpus(),
        Some("dist") => dist::dist().map(|files| {
            for f in files {
                eprintln!("dist: {}", f.display());
            }
        }),
        _ => {
            eprint!("{USAGE}");
            return ExitCode::from(1);
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("xtask: {e}");
            ExitCode::FAILURE
        }
    }
}
