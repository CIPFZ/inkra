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
  dist [--target T]  Release packages for this platform in target/dist (NF-006); T e.g.
                     aarch64-pc-windows-msvc, universal-apple-darwin
  bench [--save]     Criterion benchmarks; fail on > 20 % regression against the saved reference (NF-001)
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
    if matches!(which, "licenses" | "docs") {
        generated("docs/third-party.md", third_party()?, check)?;
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
        .replace("$VERSION", dist::numeric_version());
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
        Some("bench") => bench(args.iter().any(|a| a == "--save")),
        Some("dist") => dist::dist(
            args.iter()
                .position(|a| a == "--target")
                .and_then(|i| args.get(i + 1).cloned()),
        )
        .map(|files| {
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

/// p95 of the per-iteration times of a criterion `sample.json`.
fn p95(path: &Path) -> Option<f64> {
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
    let iters = v["iters"].as_array()?;
    let times = v["times"].as_array()?;
    let mut per: Vec<f64> = iters
        .iter()
        .zip(times)
        .filter_map(|(n, t)| Some(t.as_f64()? / n.as_f64()?.max(1.0)))
        .collect();
    if per.is_empty() {
        return None;
    }
    per.sort_by(f64::total_cmp);
    let k = ((per.len() as f64 * 0.95).ceil() as usize).clamp(1, per.len()) - 1;
    Some(per[k])
}

/// `cargo xtask bench [--save]` (NF-001, TS-006): runs the criterion benchmarks
/// (parser, typing latency, PDF and HTML export); with a saved `reference` baseline, fails when a
/// benchmark's p95 got more than 20 % slower. `--save` records the current
/// numbers as the reference.
fn bench(save: bool) -> Result<(), String> {
    let target =
        std::env::var_os("CARGO_TARGET_DIR").map_or_else(|| root().join("target"), PathBuf::from);
    let crit = target.join("criterion");
    let has_reference = std::fs::read_dir(&crit)
        .map(|rd| rd.flatten().any(|e| e.path().join("reference").is_dir()))
        .unwrap_or(false);
    for (pkg, name) in [
        ("inkra-markdown", "parse"),
        ("inkra-layout", "typing"),
        ("inkra-export", "export"),
    ] {
        let mut c = cargo();
        c.args(["bench", "-p", pkg, "--bench", name, "--"]);
        if save {
            c.args(["--save-baseline", "reference"]);
        } else if has_reference {
            c.args(["--baseline", "reference"]);
        }
        run("bench", c)?;
    }
    if save || !has_reference {
        if !save {
            eprintln!("bench: no reference baseline yet (run `cargo xtask bench --save`)");
        }
        return Ok(());
    }
    let mut slower = Vec::new();
    for e in std::fs::read_dir(&crit)
        .map_err(|e| e.to_string())?
        .flatten()
    {
        let (Some(new), Some(old)) = (
            p95(&e.path().join("new/sample.json")),
            p95(&e.path().join("reference/sample.json")),
        ) else {
            continue;
        };
        let change = new / old - 1.0;
        let name = e.file_name().to_string_lossy().into_owned();
        eprintln!(
            "bench: {name}: p95 {:.3} ms ({:+.1} % against the reference)",
            new / 1e6,
            change * 100.0
        );
        if change > 0.20 {
            slower.push(format!("{name} ({:+.0} %)", change * 100.0));
        }
    }
    if slower.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "benchmarks regressed by more than 20 % at p95: {}",
            slower.join(", ")
        ))
    }
}

/// The third-party notice (PR "关于" 致谢; NF-011): crates linked into the
/// applications on any platform, with their licenses, and the bundled assets.
fn third_party() -> Result<String, String> {
    let out = cargo()
        .args(["metadata", "--format-version", "1", "--locked"])
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).into_owned());
    }
    let meta: serde_json::Value = serde_json::from_slice(&out.stdout).map_err(|e| e.to_string())?;
    let packages = meta["packages"].as_array().cloned().unwrap_or_default();
    let by_id: std::collections::HashMap<String, &serde_json::Value> = packages
        .iter()
        .filter_map(|p| Some((p["id"].as_str()?.to_owned(), p)))
        .collect();
    let members: Vec<String> = meta["workspace_members"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| m.as_str().map(str::to_owned))
        .collect();
    // What is linked: `cargo tree` resolves features per target like a build
    // does (`cargo metadata` unifies them with dev-dependencies, listing e.g.
    // glow backends the apps never link).
    let tree = cargo()
        .args([
            "tree",
            "-p",
            "inkra",
            "-p",
            "inkra-cli",
            "-e",
            "normal,build",
            "--target",
            "all",
            "--prefix",
            "none",
            "--format",
            "{p}",
            "--locked",
        ])
        .output()
        .map_err(|e| e.to_string())?;
    if !tree.status.success() {
        return Err(String::from_utf8_lossy(&tree.stderr).into_owned());
    }
    let linked: std::collections::BTreeSet<(String, String)> =
        String::from_utf8_lossy(&tree.stdout)
            .lines()
            .filter_map(|l| {
                let mut w = l.split_whitespace();
                let name = w.next()?;
                let version = w.next()?.strip_prefix('v')?;
                Some((name.to_owned(), version.to_owned()))
            })
            .collect();
    let seen: Vec<String> = packages
        .iter()
        .filter(|p| {
            let (Some(n), Some(v)) = (p["name"].as_str(), p["version"].as_str()) else {
                return false;
            };
            linked.contains(&(n.to_owned(), v.to_owned()))
        })
        .filter_map(|p| p["id"].as_str().map(str::to_owned))
        .collect();
    let mut rows: Vec<(String, String, String, String)> = seen
        .iter()
        .filter(|id| !members.contains(id))
        .filter_map(|id| {
            let p = by_id.get(id)?;
            Some((
                p["name"].as_str()?.to_owned(),
                p["version"].as_str()?.to_owned(),
                p["license"].as_str().unwrap_or("see repository").to_owned(),
                p["repository"].as_str().unwrap_or("").to_owned(),
            ))
        })
        .collect();
    rows.sort();
    rows.dedup();
    let mut s = String::from(
        "<!-- Generated by `cargo xtask docs`. Do not edit by hand. -->\n\n# Third-party software\n\nInkra is MIT OR Apache-2.0. It contains the following works.\n\n## Bundled assets\n\n| Asset | License |\n|---|---|\n| Inter | SIL Open Font License 1.1 |\n| JetBrains Mono | SIL Open Font License 1.1 |\n| Source Serif 4 | SIL Open Font License 1.1 |\n| STIX Two Math | SIL Open Font License 1.1 |\n| gemoji emoji names | MIT |\n| SCOWL en_US Hunspell dictionary | SCOWL license (BSD-like, see `assets/dictionaries/`) |\n| Noto Emoji (monochrome) | SIL Open Font License 1.1 |\n| emoji-icon-font (UI symbols) | MIT |\n\n## Rust crates\n\n",
    );
    let _ = std::fmt::Write::write_fmt(
        &mut s,
        format_args!(
            "{} crates linked into `inkra` and `inkra-cli` on any platform.\n\n| Crate | Version | License | Source |\n|---|---|---|---|\n",
            rows.len()
        ),
    );
    for (n, v, l, r) in rows {
        let _ = std::fmt::Write::write_fmt(
            &mut s,
            format_args!("| {n} | {v} | {} | {r} |\n", l.replace('|', "\\|")),
        );
    }
    Ok(s)
}
