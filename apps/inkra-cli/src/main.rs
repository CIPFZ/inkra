//! `inkra-cli`: command line interface (CM-002).
//!
//! Exit codes: 0 success, 1 bad arguments, 2 file error, 3 conversion error
//! (and, for `lint`, problems found).

use std::path::{Path, PathBuf};
use std::process::ExitCode;

const USAGE: &str = "\
Usage: inkra-cli <COMMAND> [ARGS]

Commands:
  convert <in.md> <out> [--theme <name>] [--plain]
               Export to .pdf, .html, .png/.jpg, or (with Pandoc) .docx, .odt,
               .rtf, .epub, .tex, .wiki, .rst, .textile, .opml, .org
  inspect <in.md> [--json]
               Headings, links, images and counts
  lint <in.md> [--json]
               Structural problems: broken links, missing images, unclosed
               fences, undefined footnotes, repeated headings
  fmt <in.md> [--write]
               Normalize (tables, list numbers, whitespace, final newline)
  run <in.md> --cmd <id[=json]>... [--save <out.md>]
               Run editor commands on the document; prints it unless saved
  commands [--json]
               List all commands
  version      Print version

Options:
  -V, --version  Print version
  -h, --help     Print help
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let rest = args.get(1..).unwrap_or(&[]);
    match args.first().map(String::as_str) {
        Some("-V" | "--version" | "version") => {
            println!("inkra-cli {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Some("-h" | "--help") | None => {
            print!("{USAGE}");
            ExitCode::SUCCESS
        }
        Some("convert") => convert(rest),
        Some("inspect") => inspect(rest),
        Some("lint") => lint(rest),
        Some("fmt") => fmt(rest),
        Some("run") => run(rest),
        Some("commands") => {
            let json = rest.iter().any(|a| a == "--json");
            let reg = inkra_engine::builtin_registry();
            if json {
                let list: Vec<serde_json::Value> = reg
                    .iter()
                    .map(|c| {
                        let s = c.spec();
                        serde_json::json!({
                            "id": s.id,
                            "req": s.req,
                            "doc": s.doc,
                            "params": s.params.and_then(|p| serde_json::from_str::<serde_json::Value>(p).ok()),
                        })
                    })
                    .collect();
                println!(
                    "{}",
                    serde_json::to_string_pretty(&list).unwrap_or_default()
                );
            } else {
                for c in reg.iter() {
                    println!("{}", c.spec().id);
                }
            }
            ExitCode::SUCCESS
        }
        Some(other) => usage_error(&format!("unknown command `{other}`")),
    }
}

fn usage_error(msg: &str) -> ExitCode {
    eprintln!("inkra-cli: {msg}\n\n{USAGE}");
    ExitCode::from(1)
}

fn file_error(path: &str, e: impl std::fmt::Display) -> ExitCode {
    eprintln!("inkra-cli: {path}: {e}");
    ExitCode::from(2)
}

fn read(path: &str) -> Result<String, ExitCode> {
    std::fs::read(path)
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .map_err(|e| file_error(path, e))
}

fn parse_options() -> inkra_markdown::ParseOptions {
    inkra_engine::markdown_options(&inkra_engine::Preferences::default())
}

/// Splits arguments into positional ones and `--flag [value]` options.
fn split(args: &[String], with_value: &[&str]) -> (Vec<String>, Vec<(String, Option<String>)>) {
    let mut pos = Vec::new();
    let mut opts = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if let Some(name) = a.strip_prefix("--") {
            let v = if with_value.contains(&name) {
                it.next().cloned()
            } else {
                None
            };
            opts.push((name.to_owned(), v));
        } else {
            pos.push(a.clone());
        }
    }
    (pos, opts)
}

fn has(opts: &[(String, Option<String>)], name: &str) -> bool {
    opts.iter().any(|(n, _)| n == name)
}

fn convert(args: &[String]) -> ExitCode {
    let (pos, opts) = split(args, &["theme"]);
    let [input, output] = pos.as_slice() else {
        return usage_error("convert needs <in> and <out>");
    };
    let theme = opts
        .iter()
        .find(|(n, _)| n == "theme")
        .and_then(|(_, v)| v.clone())
        .unwrap_or_else(|| "Paper".into());
    let src = match read(input) {
        Ok(s) => s,
        Err(c) => return c,
    };
    let style = match inkra_engine::theme::load(&theme, None) {
        Ok(s) => s,
        Err(e) => return usage_error(&e),
    };
    let doc_path = Path::new(input).canonicalize().ok();
    let parse = parse_options();
    let ext = Path::new(output)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let out_path = Path::new(output);
    let result: Result<(), String> = match ext.as_str() {
        "html" | "htm" => {
            let o = inkra_export::html::HtmlExport {
                styled: !has(&opts, "plain"),
                style,
                doc_path,
                ..Default::default()
            };
            std::fs::write(out_path, inkra_export::html::export_html(&src, &parse, &o))
                .map_err(|e| e.to_string())
        }
        "pdf" => {
            let o = inkra_export::pdf::PdfExport {
                style,
                doc_path,
                parse,
                ..Default::default()
            };
            inkra_export::pdf::export_pdf(&src, &o)
                .and_then(|b| std::fs::write(out_path, b).map_err(|e| e.to_string()))
        }
        "png" | "jpg" | "jpeg" => {
            let o = inkra_export::raster::ImageExport {
                pdf: inkra_export::pdf::PdfExport {
                    style,
                    doc_path,
                    parse,
                    ..Default::default()
                },
                jpeg: ext != "png",
                ..Default::default()
            };
            inkra_export::raster::export_image(&src, &o, out_path).map(|_| ())
        }
        other => match inkra_export::pandoc::format(other) {
            Some(f) => {
                let Some(pandoc) = inkra_export::pandoc::find("") else {
                    eprintln!(
                        "inkra-cli: .{other} needs Pandoc (https://pandoc.org/installing.html)"
                    );
                    return ExitCode::from(3);
                };
                let work = std::env::temp_dir().join(format!("inkra-cli-{}", std::process::id()));
                let _ = std::fs::create_dir_all(&work);
                let md = inkra_export::pandoc::normalize(&src, doc_path.as_deref(), &work, &style);
                let dir = doc_path.as_deref().and_then(Path::parent);
                let r = inkra_export::pandoc::export(&pandoc, &md, f.writer, out_path, dir, "");
                let _ = std::fs::remove_dir_all(&work);
                r
            }
            None => return usage_error(&format!("unsupported output format `{ext}`")),
        },
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("inkra-cli: conversion failed: {e}");
            ExitCode::from(3)
        }
    }
}

fn load_doc(path: &str) -> Result<inkra_doc::Doc, ExitCode> {
    let bytes = std::fs::read(path).map_err(|e| file_error(path, e))?;
    Ok(inkra_doc::Doc::from_bytes(&bytes, None, parse_options()))
}

fn inspect(args: &[String]) -> ExitCode {
    let (pos, opts) = split(args, &[]);
    let [input] = pos.as_slice() else {
        return usage_error("inspect needs <in.md>");
    };
    let doc = match load_doc(input) {
        Ok(d) => d,
        Err(c) => return c,
    };
    let r = inkra_engine::analysis::inspect(&doc);
    if has(&opts, "json") {
        println!(
            "{}",
            serde_json::to_string_pretty(&r.to_json()).unwrap_or_default()
        );
        return ExitCode::SUCCESS;
    }
    println!("Headings:");
    for (l, t, a, line) in &r.headings {
        println!(
            "  {:>4}  {}{}  #{a}",
            line,
            "  ".repeat(usize::from(l.saturating_sub(1))),
            t
        );
    }
    println!("Links: {}", r.links.len());
    for l in &r.links {
        println!("  {:>4}  {}", l.line, l.dest);
    }
    println!("Images: {}", r.images.len());
    for i in &r.images {
        println!("  {:>4}  {}", i.line, i.dest);
    }
    let c = &r.counts;
    println!(
        "Words: {}  Characters: {}  Lines: {}  Paragraphs: {}",
        c.words, c.chars, c.lines, c.paragraphs
    );
    ExitCode::SUCCESS
}

fn lint(args: &[String]) -> ExitCode {
    let (pos, opts) = split(args, &[]);
    let [input] = pos.as_slice() else {
        return usage_error("lint needs <in.md>");
    };
    let doc = match load_doc(input) {
        Ok(d) => d,
        Err(c) => return c,
    };
    let path = Path::new(input).canonicalize().ok();
    let problems = inkra_engine::analysis::lint(&doc, path.as_deref());
    if has(&opts, "json") {
        let list: Vec<serde_json::Value> = problems
            .iter()
            .map(|p| serde_json::json!({ "line": p.line, "kind": p.kind, "message": p.message }))
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&list).unwrap_or_default()
        );
    } else {
        for p in &problems {
            println!("{input}:{}: {}: {}", p.line, p.kind, p.message);
        }
    }
    if problems.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(3)
    }
}

fn fmt(args: &[String]) -> ExitCode {
    let (pos, opts) = split(args, &[]);
    let [input] = pos.as_slice() else {
        return usage_error("fmt needs <in.md>");
    };
    let doc = match load_doc(input) {
        Ok(d) => d,
        Err(c) => return c,
    };
    let out = inkra_engine::editor::normalize::normalize(
        &doc,
        inkra_engine::editor::normalize::Normalize::ALL,
    );
    if has(&opts, "write") {
        let mut d = doc;
        let mut sel = inkra_doc::Selection::caret(0);
        if let Some(tx) = inkra_engine::editor::normalize::replace_with(&d, sel, &out) {
            sel = d.apply(tx, inkra_doc::UndoPolicy::None);
        }
        let _ = sel;
        return match inkra_workspace::files::save(Path::new(input), &d) {
            Ok(_) => ExitCode::SUCCESS,
            Err(e) => file_error(input, e),
        };
    }
    print!("{out}");
    ExitCode::SUCCESS
}

fn run(args: &[String]) -> ExitCode {
    let (pos, opts) = split(args, &["cmd", "save"]);
    let [input] = pos.as_slice() else {
        return usage_error("run needs <in.md>");
    };
    let path = PathBuf::from(input);
    let mut engine = inkra_engine::Engine::new(inkra_engine::Preferences::default());
    if let Err(e) = engine.execute("file.open", &serde_json::json!({ "path": path })) {
        return file_error(input, e);
    }
    engine.take_effects();
    for (name, v) in &opts {
        if name != "cmd" {
            continue;
        }
        let Some(spec) = v else {
            return usage_error("--cmd needs a command");
        };
        let (id, params) = match spec.split_once('=') {
            Some((id, json)) => match serde_json::from_str::<serde_json::Value>(json) {
                Ok(p) => (id.trim(), p),
                Err(e) => return usage_error(&format!("bad parameters for `{id}`: {e}")),
            },
            None => (spec.trim(), serde_json::Value::Null),
        };
        if engine.registry.get(id).is_none() {
            return usage_error(&format!("unknown command `{id}`"));
        }
        if let Err(e) = engine.execute(id, &params) {
            eprintln!("inkra-cli: {id}: {e}");
            return ExitCode::from(3);
        }
        engine.take_effects();
    }
    match opts
        .iter()
        .find(|(n, _)| n == "save")
        .and_then(|(_, v)| v.clone())
    {
        Some(out) => {
            match inkra_workspace::files::save(Path::new(&out), &engine.state.editor.doc) {
                Ok(_) => ExitCode::SUCCESS,
                Err(e) => file_error(&out, e),
            }
        }
        None => {
            print!("{}", engine.state.editor.text());
            ExitCode::SUCCESS
        }
    }
}
