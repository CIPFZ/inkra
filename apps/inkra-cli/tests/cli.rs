//! End-to-end tests of `inkra-cli` (CM-002).

use std::process::Command;

fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_inkra-cli"))
}

#[test]
fn inspect_lint_fmt_run_convert() {
    let d = tempfile::tempdir().unwrap();
    let doc = d.path().join("doc.md");
    std::fs::write(&doc, "# Title  \n\n|a|b|\n|-|-|\n|1|2|\n\nsee [x](#nope)\n").unwrap();

    let o = cli()
        .args(["inspect", doc.to_str().unwrap(), "--json"])
        .output()
        .unwrap();
    assert!(o.status.success());
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(v["headings"][0]["text"], "Title");

    let o = cli()
        .args(["lint", doc.to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(3), "problems found");
    assert!(String::from_utf8_lossy(&o.stdout).contains("broken-anchor"));

    let o = cli().args(["fmt", doc.to_str().unwrap()]).output().unwrap();
    assert_eq!(
        String::from_utf8_lossy(&o.stdout),
        "# Title\n\n| a   | b   |\n| --- | --- |\n| 1   | 2   |\n\nsee [x](#nope)\n"
    );

    let out = d.path().join("out.md");
    let o = cli()
        .args([
            "run",
            doc.to_str().unwrap(),
            "--cmd",
            "edit.selectAll",
            "--cmd",
            "format.bold",
            "--save",
            out.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert!(std::fs::read_to_string(&out).unwrap().contains("**"));

    let o = cli()
        .args(["run", doc.to_str().unwrap(), "--cmd", "no.such"])
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(1));

    let html = d.path().join("doc.html");
    let o = cli()
        .args(["convert", doc.to_str().unwrap(), html.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(o.status.success());
    assert!(
        std::fs::read_to_string(&html)
            .unwrap()
            .contains("<h1 id=\"title\">")
    );

    let o = cli()
        .args(["inspect", d.path().join("missing.md").to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(2));
}
