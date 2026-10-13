//! Pandoc bridge (EX-004) and import (EX-020).

use std::path::{Path, PathBuf};
use std::process::Command;

/// A Pandoc output format offered in the export menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PandocFormat {
    /// Pandoc writer name.
    pub writer: &'static str,
    /// File extension.
    pub ext: &'static str,
    /// Menu label.
    pub label: &'static str,
}

/// Export formats (EX-004).
pub const FORMATS: &[PandocFormat] = &[
    PandocFormat {
        writer: "docx",
        ext: "docx",
        label: "Word (.docx)",
    },
    PandocFormat {
        writer: "odt",
        ext: "odt",
        label: "OpenOffice (.odt)",
    },
    PandocFormat {
        writer: "rtf",
        ext: "rtf",
        label: "RTF",
    },
    PandocFormat {
        writer: "epub",
        ext: "epub",
        label: "EPUB",
    },
    PandocFormat {
        writer: "latex",
        ext: "tex",
        label: "LaTeX",
    },
    PandocFormat {
        writer: "mediawiki",
        ext: "wiki",
        label: "Media Wiki",
    },
    PandocFormat {
        writer: "rst",
        ext: "rst",
        label: "reStructuredText",
    },
    PandocFormat {
        writer: "textile",
        ext: "textile",
        label: "Textile",
    },
    PandocFormat {
        writer: "opml",
        ext: "opml",
        label: "OPML",
    },
    PandocFormat {
        writer: "org",
        ext: "org",
        label: "Org Mode",
    },
];

/// Import extensions (EX-020).
pub const IMPORT_EXTS: &[&str] = &[
    "docx", "odt", "rtf", "epub", "tex", "latex", "rst", "org", "wiki", "textile", "opml", "html",
    "htm",
];

/// The format for a writer name or extension.
pub fn format(name: &str) -> Option<PandocFormat> {
    FORMATS
        .iter()
        .copied()
        .find(|f| f.writer.eq_ignore_ascii_case(name) || f.ext.eq_ignore_ascii_case(name))
}

/// The Pandoc executable: the configured path, else `pandoc` on PATH.
pub fn find(configured: &str) -> Option<PathBuf> {
    let c = configured.trim();
    if !c.is_empty() {
        let p = PathBuf::from(c);
        return p.is_file().then_some(p);
    }
    let exe = if cfg!(windows) {
        "pandoc.exe"
    } else {
        "pandoc"
    };
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    // Default install locations not always on PATH for GUI apps.
    if cfg!(windows) {
        if let Some(l) = std::env::var_os("LOCALAPPDATA") {
            dirs.push(PathBuf::from(l).join("Pandoc"));
        }
        dirs.push(PathBuf::from(r"C:\Program Files\Pandoc"));
    } else {
        dirs.extend(["/usr/local/bin", "/opt/homebrew/bin", "/usr/bin"].map(PathBuf::from));
    }
    dirs.into_iter().map(|d| d.join(exe)).find(|p| p.is_file())
}

/// Rewrites the document for Pandoc: image paths become absolute and Mermaid
/// blocks become PNG images rendered into `work_dir`.
pub fn normalize(
    src: &str,
    doc_path: Option<&Path>,
    work_dir: &Path,
    style: &inkra_layout::Style,
) -> String {
    let parse = inkra_markdown::ParseOptions::default();
    let p = inkra_markdown::parse(src, &parse);
    let mut edits: Vec<(std::ops::Range<usize>, String)> = Vec::new();
    let mut n = 0;
    for b in &p.blocks {
        b.walk(&mut |blk| {
            if let inkra_markdown::BlockKind::CodeBlock { fence: Some(f) } = &blk.kind
                && f.info
                    .as_ref()
                    .is_some_and(|r| src[r.clone()].trim().eq_ignore_ascii_case("mermaid"))
            {
                let code: String = blk
                    .lines
                    .iter()
                    .map(|l| format!("{}\n", &src[l.span.clone()]))
                    .collect();
                let pal = crate::html::diagram_palette(style);
                if let Ok(scene) = inkra_diagram::render(&code, &pal).as_ref() {
                    n += 1;
                    let file = work_dir.join(format!("diagram-{n}.png"));
                    if write_scene_png(scene, &file).is_ok() {
                        edits.push((
                            blk.span.clone(),
                            format!("![]({})\n", file.to_string_lossy().replace('\\', "/")),
                        ));
                    }
                }
            }
            for i in &blk.inlines {
                i.walk(&mut |x| {
                    if let inkra_markdown::InlineKind::Image {
                        dest,
                        syntax: inkra_markdown::ast::LinkSyntax::Inline { dest: ds },
                        ..
                    } = &x.kind
                        && !ds.is_empty()
                        && let Some(abs) = crate::html::resolve_image(doc_path, dest)
                        && abs.is_file()
                    {
                        edits.push((
                            ds.clone(),
                            format!("<{}>", abs.to_string_lossy().replace('\\', "/")),
                        ));
                    }
                });
            }
        });
    }
    edits.sort_by_key(|e| std::cmp::Reverse(e.0.start));
    let mut out = src.to_owned();
    for (r, t) in edits {
        if r.end <= out.len() {
            out.replace_range(r, &t);
        }
    }
    out
}

fn write_scene_png(scene: &inkra_diagram::Scene, file: &Path) -> Result<(), String> {
    let png = crate::raster::diagram_png(scene, 2.0, Some(inkra_layout::epaint::Color32::WHITE))?;
    std::fs::write(file, png).map_err(|e| e.to_string())
}

/// Runs Pandoc: `input` (Markdown text) to `out` in `writer`; `extra` are
/// additional arguments split on whitespace. Returns stderr on failure.
pub fn export(
    pandoc: &Path,
    markdown: &str,
    writer: &str,
    out: &Path,
    resource_dir: Option<&Path>,
    extra: &str,
) -> Result<(), String> {
    let tmp = std::env::temp_dir().join(format!("inkra-pandoc-{}.md", std::process::id()));
    std::fs::write(&tmp, markdown).map_err(|e| e.to_string())?;
    let mut cmd = Command::new(pandoc);
    cmd.arg(&tmp)
        .args(["-f", "markdown+tex_math_dollars+pipe_tables+task_lists+strikeout+yaml_metadata_block+footnotes", "-t", writer, "--standalone", "-o"])
        .arg(out);
    if let Some(d) = resource_dir {
        cmd.arg(format!("--resource-path={}", d.display()));
    }
    cmd.args(extra.split_whitespace());
    let r = cmd.output();
    let _ = std::fs::remove_file(&tmp);
    match r {
        Ok(o) if o.status.success() => Ok(()),
        Ok(o) => Err(String::from_utf8_lossy(&o.stderr).trim().to_owned()),
        Err(e) => Err(format!("{}: {e}", pandoc.display())),
    }
}

/// Runs a custom export template (EX-004): `$input`, `$output` and
/// `$outputDir` are replaced; the template is split on whitespace (quotes
/// group words).
pub fn run_template(template: &str, input: &Path, output: &Path) -> Result<(), String> {
    let words = split_words(template);
    let subst = |w: &str| {
        w.replace("$input", &input.to_string_lossy())
            .replace(
                "$outputDir",
                &output.parent().unwrap_or(Path::new(".")).to_string_lossy(),
            )
            .replace("$output", &output.to_string_lossy())
    };
    let mut it = words.iter().map(|w| subst(w));
    let prog = it.next().ok_or("empty command")?;
    let o = Command::new(&prog)
        .args(it)
        .output()
        .map_err(|e| format!("{prog}: {e}"))?;
    if o.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&o.stderr).trim().to_owned())
    }
}

fn split_words(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    let mut any = false;
    for c in s.chars() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), c) => cur.push(c),
            (None, '"' | '\'') => {
                quote = Some(c);
                any = true;
            }
            (None, c) if c.is_whitespace() => {
                if !cur.is_empty() || any {
                    out.push(std::mem::take(&mut cur));
                }
                any = false;
            }
            (None, c) => cur.push(c),
        }
    }
    if !cur.is_empty() || any {
        out.push(cur);
    }
    out
}

/// Imports `input` as Markdown (EX-020): HTML without Pandoc uses the built-in
/// converter; media is extracted into `assets_dir`.
pub fn import(input: &Path, pandoc: Option<&Path>, assets_dir: &Path) -> Result<String, String> {
    let ext = input
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !IMPORT_EXTS.contains(&ext.as_str()) {
        return Err(format!("cannot import .{ext} files"));
    }
    let html = matches!(ext.as_str(), "html" | "htm");
    match pandoc {
        None if html => {
            let bytes = std::fs::read(input).map_err(|e| e.to_string())?;
            Ok(inkra_html::html_to_markdown(&String::from_utf8_lossy(
                &bytes,
            )))
        }
        None => Err(
            "Pandoc is required to import this format (https://pandoc.org/installing.html)".into(),
        ),
        Some(p) => {
            let o = Command::new(p)
                .arg(input)
                .args(["-t", "gfm+tex_math_dollars+footnotes", "--wrap=none"])
                .arg(format!("--extract-media={}", assets_dir.display()))
                .output()
                .map_err(|e| format!("{}: {e}", p.display()))?;
            if o.status.success() {
                Ok(String::from_utf8_lossy(&o.stdout).into_owned())
            } else {
                Err(String::from_utf8_lossy(&o.stderr).trim().to_owned())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_resolves_images_and_renders_diagrams() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("a.png"), b"x").unwrap();
        let doc = d.path().join("doc.md");
        let src = "# T\n\n![a](a.png)\n\n```mermaid\npie\n\"x\" : 1\n```\n\nend\n";
        let out = normalize(src, Some(&doc), d.path(), &inkra_layout::Style::default());
        assert!(
            out.contains(&format!(
                "![a](<{}>)",
                d.path().join("a.png").to_string_lossy().replace('\\', "/")
            )),
            "{out}"
        );
        assert!(
            out.contains("diagram-1.png") && !out.contains("```mermaid"),
            "{out}"
        );
        assert!(d.path().join("diagram-1.png").is_file());
        assert!(out.ends_with("end\n"));
    }

    #[test]
    fn html_import_without_pandoc_and_word_splitting() {
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join("x.html");
        std::fs::write(&f, "<h1>Title</h1><p>Hello <b>world</b></p>").unwrap();
        assert_eq!(
            import(&f, None, d.path()).unwrap(),
            "# Title\n\nHello **world**"
        );
        assert!(import(&d.path().join("x.docx"), None, d.path()).is_err());
        assert_eq!(
            split_words(r#"pandoc "$input" -o 'a b'"#),
            ["pandoc", "$input", "-o", "a b"]
        );
        assert_eq!(format("tex").map(|f| f.writer), Some("latex"));
    }
}

#[cfg(test)]
mod pandoc_tests {
    use super::*;

    /// Runs only where Pandoc is installed (set `INKRA_PANDOC` or put it on PATH).
    #[test]
    fn docx_round_trip_with_images() {
        let Some(pandoc) = find(&std::env::var("INKRA_PANDOC").unwrap_or_default()) else {
            eprintln!("pandoc not found; skipped");
            return;
        };
        let d = tempfile::tempdir().unwrap();
        let png =
            crate::raster::fills_png(4.0, 4.0, &[], 1.0, Some(inkra_layout::epaint::Color32::RED))
                .unwrap();
        std::fs::write(d.path().join("pic.png"), png).unwrap();
        let doc = d.path().join("doc.md");
        let src =
            "# Report\n\nHello **world**.\n\n![pic](pic.png)\n\n```mermaid\npie\n\"a\" : 1\n```\n";
        let md = normalize(src, Some(&doc), d.path(), &inkra_layout::Style::default());
        let docx = d.path().join("out.docx");
        export(&pandoc, &md, "docx", &docx, Some(d.path()), "").unwrap();
        assert!(std::fs::metadata(&docx).unwrap().len() > 1000);
        let back = d.path().join("back");
        std::fs::create_dir_all(&back).unwrap();
        let assets = back.join("assets");
        let imported = import(&docx, Some(&pandoc), &assets).unwrap();
        assert!(
            imported.contains("# Report") && imported.contains("**world**"),
            "{imported}"
        );
        assert!(imported.contains("assets"), "{imported}");
        assert!(
            std::fs::read_dir(assets.join("media")).unwrap().count() >= 2,
            "both images extracted"
        );
    }
}
