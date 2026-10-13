//! `inkra-cli`: command line interface (CM-002).

use std::process::ExitCode;

const USAGE: &str = "\
Usage: inkra-cli <COMMAND> [ARGS]

Commands:
  convert <in.md> <out.pdf|out.html|out.png> [--theme <name>] [--plain]
               Export a document
  commands     List all commands
  version      Print version

Options:
  -V, --version  Print version
  -h, --help     Print help
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("-V" | "--version" | "version") => {
            println!("inkra-cli {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Some("-h" | "--help") | None => {
            print!("{USAGE}");
            ExitCode::SUCCESS
        }
        Some("convert") => convert(&args[1..]),
        Some("commands") => {
            for c in inkra_engine::builtin_registry().iter() {
                println!("{}", c.spec().id);
            }
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("inkra-cli: unknown command `{other}`\n\n{USAGE}");
            ExitCode::from(1)
        }
    }
}

/// Exit codes (CM-002): 1 bad arguments, 2 file error, 3 conversion error.
fn convert(args: &[String]) -> ExitCode {
    let mut pos = Vec::new();
    let mut theme = "Paper".to_owned();
    let mut plain = false;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--theme" => match it.next() {
                Some(t) => theme = t.clone(),
                None => return usage_error("--theme needs a name"),
            },
            "--plain" => plain = true,
            _ => pos.push(a.clone()),
        }
    }
    let [input, output] = pos.as_slice() else {
        return usage_error("convert needs <in> and <out>");
    };
    let src = match std::fs::read(input) {
        Ok(b) => String::from_utf8_lossy(&b).into_owned(),
        Err(e) => {
            eprintln!("inkra-cli: {input}: {e}");
            return ExitCode::from(2);
        }
    };
    let style = match inkra_engine::theme::load(&theme, None) {
        Ok(s) => s,
        Err(e) => return usage_error(&e),
    };
    let doc_path = std::path::Path::new(input).canonicalize().ok();
    let parse = inkra_markdown_options();
    let ext = std::path::Path::new(output)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let bytes: Result<Vec<u8>, String> = match ext.as_str() {
        "html" | "htm" => {
            let o = inkra_export::html::HtmlExport {
                styled: !plain,
                style,
                doc_path,
                ..Default::default()
            };
            Ok(inkra_export::html::export_html(&src, &parse, &o).into_bytes())
        }
        "pdf" => {
            let o = inkra_export::pdf::PdfExport {
                style,
                doc_path,
                parse,
                ..Default::default()
            };
            inkra_export::pdf::export_pdf(&src, &o)
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
            return match inkra_export::raster::export_image(&src, &o, std::path::Path::new(output))
            {
                Ok(_) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("inkra-cli: conversion failed: {e}");
                    ExitCode::from(3)
                }
            };
        }
        _ => return usage_error(&format!("unsupported output format `{ext}`")),
    };
    match bytes {
        Ok(b) => match std::fs::write(output, b) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("inkra-cli: {output}: {e}");
                ExitCode::from(2)
            }
        },
        Err(e) => {
            eprintln!("inkra-cli: conversion failed: {e}");
            ExitCode::from(3)
        }
    }
}

fn inkra_markdown_options() -> inkra_markdown::ParseOptions {
    inkra_engine::markdown_options(&inkra_engine::Preferences::default())
}

fn usage_error(msg: &str) -> ExitCode {
    eprintln!("inkra-cli: {msg}\n\n{USAGE}");
    ExitCode::from(1)
}
