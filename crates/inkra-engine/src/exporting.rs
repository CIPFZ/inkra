//! Export, print and import commands (EX-001 … EX-006, EX-020): commands build
//! an [`ExportJob`] from the document and the export preferences; the host runs
//! it off the UI thread with [`run`].

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use inkra_export::html::{HtmlExport, ImagePaths, MathOutput};
use inkra_export::pdf::{PageSetup, PdfExport};
use inkra_export::raster::ImageExport;
use serde_json::Value;

use crate::command::{
    Category, CommandError, CommandRegistry, CommandResult, CommandSpec, FnCommand, Outcome, spec,
};
use crate::state::{AppState, CommandContext, Effect};

/// An export target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportFormat {
    /// HTML with the theme's styles.
    Html,
    /// HTML fragment without styles.
    HtmlPlain,
    /// PDF.
    Pdf,
    /// PNG image(s).
    Image,
    /// A Pandoc writer.
    Pandoc(String),
    /// The formula or diagram at the caret (PNG, or SVG by extension).
    Block,
    /// A custom export command from preferences (EX-004).
    Custom(usize),
}

impl ExportFormat {
    /// File extension of the output.
    pub fn ext(&self) -> &str {
        match self {
            Self::Html | Self::HtmlPlain => "html",
            Self::Pdf => "pdf",
            Self::Image => "png",
            Self::Pandoc(w) => inkra_export::pandoc::format(w).map_or("out", |f| f.ext),
            Self::Block => "png",
            Self::Custom(_) => "out",
        }
    }

    /// The command that exports to this format.
    pub fn command(&self) -> &'static str {
        match self {
            Self::Html => "file.exportHtml",
            Self::HtmlPlain => "file.exportHtmlPlain",
            Self::Pdf => "file.exportPdf",
            Self::Image => "file.exportImage",
            Self::Pandoc(_) => "file.exportPandoc",
            Self::Block => "file.exportBlockImage",
            Self::Custom(_) => "file.exportCustom",
        }
    }

    /// Parameters for [`Self::command`] besides the path.
    pub fn params(&self) -> Value {
        match self {
            Self::Pandoc(w) => serde_json::json!({ "format": w }),
            Self::Custom(k) => serde_json::json!({ "index": k }),
            _ => serde_json::json!({}),
        }
    }
}

/// Work for a background thread.
#[derive(Debug, Clone, PartialEq)]
pub enum JobKind {
    /// HTML export.
    Html(Box<HtmlExport>),
    /// PDF export.
    Pdf(Box<PdfExport>),
    /// Image export.
    Image(Box<ImageExport>),
    /// Pandoc export.
    Pandoc {
        /// Executable.
        pandoc: PathBuf,
        /// Writer.
        writer: String,
        /// Extra arguments.
        args: String,
        /// Theme (Mermaid colors).
        style: Box<inkra_layout::Style>,
        /// The document (relative paths).
        doc_path: Option<PathBuf>,
    },
    /// Print: a PDF in the temporary folder handed to the system.
    Print(Box<PdfExport>),
    /// A custom command template run on the saved document (or a temporary
    /// copy next to it when there are unsaved changes).
    Custom {
        /// Command template.
        template: String,
        /// The saved document, if it is up to date.
        saved: Option<PathBuf>,
        /// Folder for the temporary copy.
        dir: PathBuf,
    },
    /// Import a file as Markdown into `out`.
    Import {
        /// Source file.
        input: PathBuf,
        /// Pandoc, if available.
        pandoc: Option<PathBuf>,
    },
}

/// A job: what to do with the document source and where the result goes.
#[derive(Debug, Clone, PartialEq)]
pub struct ExportJob {
    /// Kind and options.
    pub kind: JobKind,
    /// Document source.
    pub src: String,
    /// Output file.
    pub out: PathBuf,
    /// Open the result when done.
    pub open_after: bool,
    /// Markdown options of the document.
    pub parse: inkra_markdown::ParseOptions,
}

/// What a finished job produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobResult {
    /// Files written.
    Exported(Vec<PathBuf>),
    /// A PDF to print.
    Print(PathBuf),
    /// A Markdown file to open.
    Imported(PathBuf),
}

/// Runs a job (blocking; call from a worker thread).
pub fn run(job: &ExportJob) -> Result<JobResult, String> {
    let write = |bytes: Vec<u8>| -> Result<JobResult, String> {
        std::fs::write(&job.out, bytes).map_err(|e| format!("{}: {e}", job.out.display()))?;
        Ok(JobResult::Exported(vec![job.out.clone()]))
    };
    match &job.kind {
        JobKind::Html(o) => {
            write(inkra_export::html::export_html(&job.src, &job.parse, o).into_bytes())
        }
        JobKind::Pdf(o) => write(inkra_export::pdf::export_pdf(&job.src, o)?),
        JobKind::Print(o) => {
            let bytes = inkra_export::pdf::export_pdf(&job.src, o)?;
            std::fs::write(&job.out, bytes).map_err(|e| e.to_string())?;
            Ok(JobResult::Print(job.out.clone()))
        }
        JobKind::Image(o) => {
            inkra_export::raster::export_image(&job.src, o, &job.out).map(JobResult::Exported)
        }
        JobKind::Pandoc {
            pandoc,
            writer,
            args,
            style,
            doc_path,
        } => {
            let work = std::env::temp_dir().join(format!("inkra-export-{}", std::process::id()));
            std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;
            let md = inkra_export::pandoc::normalize(&job.src, doc_path.as_deref(), &work, style);
            let dir = doc_path.as_deref().and_then(Path::parent);
            let r = inkra_export::pandoc::export(pandoc, &md, writer, &job.out, dir, args);
            let _ = std::fs::remove_dir_all(&work);
            r.map(|()| JobResult::Exported(vec![job.out.clone()]))
        }
        JobKind::Custom {
            template,
            saved,
            dir,
        } => {
            let (input, temp) = match saved {
                Some(p) => (p.clone(), None),
                None => {
                    let t = dir.join(format!(".inkra-export-{}.md", std::process::id()));
                    std::fs::write(&t, &job.src).map_err(|e| e.to_string())?;
                    (t.clone(), Some(t))
                }
            };
            let r = inkra_export::pandoc::run_template(template, &input, &job.out);
            if let Some(t) = temp {
                let _ = std::fs::remove_file(t);
            }
            r.map(|()| JobResult::Exported(vec![job.out.clone()]))
        }
        JobKind::Import { input, pandoc } => {
            let dir = job.out.parent().unwrap_or(Path::new("."));
            let assets = dir.join("assets");
            let md = inkra_export::pandoc::import(input, pandoc.as_deref(), &assets)?;
            // Media paths written by Pandoc are made relative to the new file.
            let prefix = format!("{}/", dir.to_string_lossy());
            let md = md
                .replace(&prefix, "")
                .replace(&prefix.replace('\\', "/"), "");
            std::fs::write(&job.out, md).map_err(|e| format!("{}: {e}", job.out.display()))?;
            Ok(JobResult::Imported(job.out.clone()))
        }
    }
}

/// The Pandoc executable from preferences or PATH (cached for a few seconds:
/// menus ask every frame).
pub fn pandoc(app: &AppState) -> Option<PathBuf> {
    static CACHE: Mutex<Option<(String, Instant, Option<PathBuf>)>> = Mutex::new(None);
    let want = app.prefs.string("export.pandocPath");
    if let Ok(c) = CACHE.lock()
        && let Some((w, at, p)) = c.as_ref()
        && *w == want
        && at.elapsed() < Duration::from_secs(5)
    {
        return p.clone();
    }
    let p = inkra_export::pandoc::find(&want);
    if let Ok(mut c) = CACHE.lock() {
        *c = Some((want, Instant::now(), p.clone()));
    }
    p
}

/// The theme used for exports (EX-005): the preference, else the current one.
/// Without a page background a dark theme falls back to the light default.
fn export_style(app: &AppState, background: bool) -> inkra_layout::Style {
    let name = app.prefs.string("export.theme");
    let dir = app.paths.as_ref().map(inkra_workspace::Paths::themes_dir);
    let mut style = if name.trim().is_empty() {
        app.style.clone()
    } else {
        crate::theme::load(name.trim(), dir.as_deref()).unwrap_or_else(|_| app.style.clone())
    };
    if style.dark && !background {
        style = crate::theme::load("Paper", None).unwrap_or_default();
    }
    style.math_numbering = app.style.math_numbering;
    style.diagrams = app.style.diagrams;
    style.toc_depth = app.style.toc_depth;
    style
}

/// Page setup from preferences (EX-002).
pub fn page_setup(app: &AppState) -> PageSetup {
    let p = &app.prefs;
    let (mut w, mut h) =
        inkra_export::pdf::paper(&p.string("export.pdf.paper")).unwrap_or((595.28, 841.89));
    if p.string("export.pdf.paper").eq_ignore_ascii_case("custom") {
        let mm = |k: &str| (p.f64(k) as f32 * 72.0 / 25.4).max(72.0);
        (w, h) = (mm("export.pdf.customWidth"), mm("export.pdf.customHeight"));
    }
    if p.string("export.pdf.orientation") == "landscape" {
        std::mem::swap(&mut w, &mut h);
    }
    let m = |k: &str| (p.f64(k) as f32 * 72.0 / 25.4).max(0.0);
    PageSetup {
        width: w,
        height: h,
        margins: [
            m("export.pdf.marginTop"),
            m("export.pdf.marginRight"),
            m("export.pdf.marginBottom"),
            m("export.pdf.marginLeft"),
        ],
        header: p.string("export.pdf.header"),
        footer: p.string("export.pdf.footer"),
        background: p.bool("export.pdf.background"),
        scale: 0.75,
    }
}

fn pdf_options(app: &AppState) -> PdfExport {
    let page = page_setup(app);
    PdfExport {
        style: export_style(app, page.background),
        page,
        doc_path: app.editor.path.clone(),
        parse: app.editor.doc.options().clone(),
        date: None,
    }
}

fn html_options(app: &AppState, styled: bool) -> HtmlExport {
    let p = &app.prefs;
    HtmlExport {
        styled,
        style: export_style(app, true),
        images: match p.string("export.html.images").as_str() {
            "absolute" => ImagePaths::Absolute,
            "embed" => ImagePaths::Embed,
            _ => ImagePaths::Keep,
        },
        math: if p.string("export.html.math") == "tex" {
            MathOutput::Tex
        } else {
            MathOutput::Svg
        },
        highlight: p.bool("export.html.highlight"),
        diagrams: app.style.diagrams,
        doc_path: app.editor.path.clone(),
        head_html: p.string("export.html.head"),
        foot_html: p.string("export.html.foot"),
        custom_css: p.string("export.html.css"),
        soft_break_as_br: p.bool("export.html.softBreakAsBr"),
    }
}

/// The HTML for "copy as HTML" (EX-006), sharing the export generator.
pub fn clipboard_html(app: &AppState, src: &str) -> String {
    inkra_export::html::clipboard_html(
        src,
        app.editor.doc.options(),
        &app.style,
        app.editor.path.as_deref(),
    )
}

/// Builds the job for `format` writing to `out`.
pub fn job(app: &AppState, format: &ExportFormat, out: PathBuf) -> Result<ExportJob, String> {
    let src = app.editor.doc.text().to_string();
    let open_after = app.prefs.bool("export.openAfter");
    let kind = match format {
        ExportFormat::Html => JobKind::Html(Box::new(html_options(app, true))),
        ExportFormat::HtmlPlain => JobKind::Html(Box::new(html_options(app, false))),
        ExportFormat::Pdf => JobKind::Pdf(Box::new(pdf_options(app))),
        ExportFormat::Image => {
            let scale = app.prefs.f64("export.image.scale").clamp(1.0, 3.0) as f32;
            JobKind::Image(Box::new(ImageExport {
                pdf: pdf_options(app),
                scale,
                background: None,
                width: app.style.max_width.clamp(480.0, 1200.0),
                paged: app.prefs.bool("export.image.paged"),
                jpeg: out.extension().is_some_and(|e| {
                    e.eq_ignore_ascii_case("jpg") || e.eq_ignore_ascii_case("jpeg")
                }),
            }))
        }
        ExportFormat::Block => return Err("not a document export".into()),
        ExportFormat::Custom(k) => {
            let c = custom_exports(app)
                .into_iter()
                .nth(*k)
                .ok_or("no such export command")?;
            let e = &app.editor;
            JobKind::Custom {
                template: c.template,
                saved: e.path.clone().filter(|_| !e.doc.is_dirty()),
                dir: e
                    .path
                    .as_ref()
                    .and_then(|p| p.parent().map(Path::to_owned))
                    .unwrap_or_else(std::env::temp_dir),
            }
        }
        ExportFormat::Pandoc(writer) => {
            let pandoc = pandoc(app)
                .ok_or("Pandoc is not installed (https://pandoc.org/installing.html)")?;
            JobKind::Pandoc {
                pandoc,
                writer: writer.clone(),
                args: app.prefs.string("export.pandocArgs"),
                style: Box::new(export_style(app, true)),
                doc_path: app.editor.path.clone(),
            }
        }
    };
    Ok(ExportJob {
        kind,
        src,
        out,
        open_after,
        parse: app.editor.doc.options().clone(),
    })
}

/// The suggested output name: the document's name with the format's extension.
pub fn suggested_name(app: &AppState, ext: &str) -> String {
    let stem = app
        .editor
        .path
        .as_ref()
        .and_then(|p| p.file_stem())
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| {
            inkra_workspace::files::suggested_name(&app.editor.doc, "Untitled", "md")
                .trim_end_matches(".md")
                .to_owned()
        });
    format!("{stem}.{ext}")
}

fn path_param(p: &Value) -> Option<PathBuf> {
    p.get("path")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
}

fn params(mut s: CommandSpec, p: &'static str) -> CommandSpec {
    s.params = Some(p);
    s
}

fn fail(id: &str, msg: impl std::fmt::Display) -> CommandError {
    CommandError::Failed {
        id: id.to_owned(),
        msg: msg.to_string(),
    }
}

/// Runs an export command: with `path` the job is queued, else the host asks
/// where to save (EX-005: `sameFolder` writes next to the document).
fn export(cx: &mut CommandContext<'_>, p: &Value, format: ExportFormat) -> CommandResult {
    let out = match path_param(p) {
        Some(o) => Some(o),
        None if p
            .get("sameFolder")
            .and_then(Value::as_bool)
            .unwrap_or(false) =>
        {
            let doc = cx
                .app
                .editor
                .path
                .clone()
                .ok_or_else(|| fail(format.command(), "the document has not been saved"))?;
            Some(doc.with_extension(format.ext()))
        }
        None => None,
    };
    match out {
        Some(out) => {
            let job = job(cx.app, &format, out).map_err(|e| fail(format.command(), e))?;
            cx.effects.push(Effect::RunJob(Box::new(job)));
        }
        None => {
            let name = suggested_name(cx.app, format.ext());
            let dir = cx
                .app
                .editor
                .path
                .as_ref()
                .and_then(|p| p.parent().map(Path::to_owned));
            cx.effects.push(Effect::ExportDialog { format, name, dir });
        }
    }
    Ok(Outcome::Done)
}

/// A custom export entry (`export.custom`: `Label|ext|command template`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomExport {
    /// Menu label.
    pub label: String,
    /// Output extension.
    pub ext: String,
    /// Command template with `$input`, `$output`, `$outputDir`.
    pub template: String,
}

/// The custom export entries from preferences.
pub fn custom_exports(app: &AppState) -> Vec<CustomExport> {
    app.prefs
        .strings("export.custom")
        .iter()
        .filter_map(|s| {
            let mut it = s.splitn(3, '|');
            let (label, ext, template) = (it.next()?.trim(), it.next()?.trim(), it.next()?.trim());
            (!label.is_empty() && !template.is_empty()).then(|| CustomExport {
                label: label.to_owned(),
                ext: ext.trim_start_matches('.').to_owned(),
                template: template.to_owned(),
            })
        })
        .collect()
}

/// The formula or diagram at the caret.
pub enum BlockVector {
    /// A display formula.
    Math(std::sync::Arc<inkra_math::Rendered>),
    /// A Mermaid diagram.
    Diagram(std::sync::Arc<inkra_diagram::Scene>),
}

/// The display formula or Mermaid diagram containing the caret (MD-100: copy as
/// SVG/PNG, export image).
pub fn block_vector(app: &AppState) -> Option<BlockVector> {
    let e = &app.editor;
    let path = crate::editor::context::path(&e.doc, e.sel.head);
    let text = e.doc.text();
    path.iter().rev().find_map(|n| {
        let content = || -> Option<String> {
            let (a, z) = (n.block.lines.first()?, n.block.lines.last()?);
            Some(
                text.slice(n.abs(a.span.start)..n.abs(z.span.end))
                    .into_owned(),
            )
        };
        match &n.block.kind {
            inkra_markdown::BlockKind::MathBlock { .. } => {
                let tex = content()?;
                Some(BlockVector::Math(inkra_math::render(
                    &inkra_math::Request::new(&tex, true, app.style.size * 1.12),
                )))
            }
            inkra_markdown::BlockKind::CodeBlock { fence: Some(f) } => {
                let info = f
                    .info
                    .clone()
                    .map(|r| text.slice(n.abs(r.start)..n.abs(r.end)).into_owned())?;
                if !info.trim().eq_ignore_ascii_case("mermaid") {
                    return None;
                }
                let src = format!("{}\n", content()?);
                let pal = inkra_export::html::diagram_palette(&app.style);
                inkra_diagram::render(&src, &pal)
                    .as_ref()
                    .as_ref()
                    .ok()
                    .cloned()
                    .map(|s| BlockVector::Diagram(std::sync::Arc::new(s)))
            }
            _ => None,
        }
    })
}

/// SVG markup of a block.
pub fn block_svg(b: &BlockVector) -> String {
    match b {
        BlockVector::Math(r) => inkra_export::svg::math_svg(&r.scene, ""),
        BlockVector::Diagram(s) => inkra_export::svg::diagram_svg(s),
    }
}

/// PNG of a block at 2x.
pub fn block_png(app: &AppState, b: &BlockVector) -> Result<Vec<u8>, String> {
    match b {
        BlockVector::Math(r) => inkra_export::raster::math_png(&r.scene, app.style.fg, 2.0),
        BlockVector::Diagram(s) => inkra_export::raster::diagram_png(s, 2.0, None),
    }
}

fn register_blocks(r: &mut CommandRegistry) {
    r.register(
        FnCommand::new(
            spec(
                "edit.copyBlockAsSvg",
                Category::Edit,
                "MD-100",
                "Copy the formula or diagram at the caret as SVG markup.",
            ),
            |cx, _| {
                let b = block_vector(cx.app).ok_or_else(|| {
                    fail("edit.copyBlockAsSvg", "no formula or diagram at the caret")
                })?;
                cx.effects.push(Effect::SetClipboard {
                    text: block_svg(&b),
                    html: None,
                });
                Ok(Outcome::Done)
            },
        )
        .enabled(|cx| block_vector(cx.app).is_some()),
    );
    r.register(
        FnCommand::new(
            spec(
                "edit.copyBlockAsPng",
                Category::Edit,
                "MD-100",
                "Copy the formula or diagram at the caret as an image.",
            ),
            |cx, _| {
                let b = block_vector(cx.app).ok_or_else(|| {
                    fail("edit.copyBlockAsPng", "no formula or diagram at the caret")
                })?;
                let png = block_png(cx.app, &b).map_err(|e| fail("edit.copyBlockAsPng", e))?;
                cx.effects.push(Effect::SetClipboardImage(png));
                Ok(Outcome::Done)
            },
        )
        .enabled(|cx| block_vector(cx.app).is_some()),
    );
    r.register(
        FnCommand::new(
            params(
                spec(
                    "file.exportBlockImage",
                    Category::File,
                    "EX-003",
                    "Export the formula or diagram at the caret as PNG or SVG (by extension).",
                ),
                PATH_PARAMS,
            ),
            |cx, p| {
                let b = block_vector(cx.app).ok_or_else(|| {
                    fail(
                        "file.exportBlockImage",
                        "no formula or diagram at the caret",
                    )
                })?;
                let Some(out) = path_param(p) else {
                    let name = suggested_name(cx.app, "png");
                    let dir = cx
                        .app
                        .editor
                        .path
                        .as_ref()
                        .and_then(|p| p.parent().map(Path::to_owned));
                    cx.effects.push(Effect::ExportDialog {
                        format: ExportFormat::Block,
                        name,
                        dir,
                    });
                    return Ok(Outcome::Done);
                };
                let svg = out
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("svg"));
                let bytes = if svg {
                    block_svg(&b).into_bytes()
                } else {
                    block_png(cx.app, &b).map_err(|e| fail("file.exportBlockImage", e))?
                };
                std::fs::write(&out, bytes).map_err(|e| fail("file.exportBlockImage", e))?;
                Ok(Outcome::Done)
            },
        )
        .enabled(|cx| block_vector(cx.app).is_some()),
    );
}

const PATH_PARAMS: &str =
    r#"{"type":"object","properties":{"path":{"type":"string"},"sameFolder":{"type":"boolean"}}}"#;

/// Registers export, print and import commands.
pub fn register(r: &mut CommandRegistry) {
    use Category::File;
    register_blocks(r);
    r.register(FnCommand::new(
        params(spec("file.exportHtml", File, "EX-001", "Export as HTML with the theme's styles (asks for the file unless `path` is given)."), PATH_PARAMS),
        |cx, p| export(cx, p, ExportFormat::Html),
    ));
    r.register(FnCommand::new(
        params(
            spec(
                "file.exportHtmlPlain",
                File,
                "EX-001",
                "Export as an HTML fragment without styles.",
            ),
            PATH_PARAMS,
        ),
        |cx, p| export(cx, p, ExportFormat::HtmlPlain),
    ));
    r.register(FnCommand::new(
        params(
            spec(
                "file.exportPdf",
                File,
                "EX-002",
                "Export as PDF using the page setup.",
            ),
            PATH_PARAMS,
        ),
        |cx, p| export(cx, p, ExportFormat::Pdf),
    ));
    r.register(FnCommand::new(
        params(
            spec(
                "file.exportImage",
                File,
                "EX-003",
                "Export as a PNG image (one long image, or one per page).",
            ),
            PATH_PARAMS,
        ),
        |cx, p| export(cx, p, ExportFormat::Image),
    ));
    r.register(FnCommand::new(
        spec(
            "help.installPandoc",
            Category::Help,
            "EX-004",
            "Open the Pandoc installation page.",
        ),
        |cx, _| {
            cx.effects
                .push(Effect::OpenUrl("https://pandoc.org/installing.html".into()));
            Ok(Outcome::Done)
        },
    ));
    r.register(
        FnCommand::new(
            params(
                spec("file.exportCustom", File, "EX-004", "Run a custom export command from the `export.custom` preference."),
                r#"{"type":"object","required":["index"],"properties":{"index":{"type":"integer","minimum":0},"path":{"type":"string"},"sameFolder":{"type":"boolean"}}}"#,
            ),
            |cx, p| {
                let k = p.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
                let c = custom_exports(cx.app).into_iter().nth(k).ok_or_else(|| fail("file.exportCustom", "no such export command"))?;
                let mut p = p.clone();
                if path_param(&p).is_none() && !p.get("sameFolder").and_then(Value::as_bool).unwrap_or(false) {
                    let name = suggested_name(cx.app, &c.ext);
                    let dir = cx.app.editor.path.as_ref().and_then(|p| p.parent().map(Path::to_owned));
                    cx.effects.push(Effect::ExportDialog { format: ExportFormat::Custom(k), name, dir });
                    return Ok(Outcome::Done);
                }
                if p.get("sameFolder").and_then(Value::as_bool).unwrap_or(false)
                    && let Some(doc) = cx.app.editor.path.clone()
                {
                    p["path"] = Value::String(doc.with_extension(&c.ext).to_string_lossy().into_owned());
                }
                export(cx, &p, ExportFormat::Custom(k))
            },
        )
        .enabled(|cx| !custom_exports(cx.app).is_empty()),
    );
    r.register(
        FnCommand::new(
            params(
                spec("file.exportPandoc", File, "EX-004", "Export through Pandoc (docx, odt, rtf, epub, latex, mediawiki, rst, textile, opml, org)."),
                r#"{"type":"object","required":["format"],"properties":{"format":{"enum":["docx","odt","rtf","epub","latex","mediawiki","rst","textile","opml","org"]},"path":{"type":"string"},"sameFolder":{"type":"boolean"}}}"#,
            ),
            |cx, p| {
                let w = p.get("format").and_then(Value::as_str).unwrap_or("docx");
                let f = inkra_export::pandoc::format(w).ok_or_else(|| fail("file.exportPandoc", format!("unknown format `{w}`")))?;
                export(cx, p, ExportFormat::Pandoc(f.writer.to_owned()))
            },
        )
        .enabled(|cx| pandoc(cx.app).is_some()),
    );
    r.register(FnCommand::new(
        spec(
            "file.print",
            File,
            "EX-002",
            "Print: export a PDF and hand it to the system's print dialog.",
        ),
        |cx, _| {
            let out = std::env::temp_dir().join(format!("inkra-print-{}.pdf", cx.app.active()));
            let o = pdf_options(cx.app);
            let src = cx.app.editor.doc.text().to_string();
            cx.effects.push(Effect::RunJob(Box::new(ExportJob {
                kind: JobKind::Print(Box::new(o)),
                src,
                out,
                open_after: false,
                parse: cx.app.editor.doc.options().clone(),
            })));
            Ok(Outcome::Done)
        },
    ));
    r.register(FnCommand::new(
        spec(
            "file.pageSetup",
            File,
            "EX-002",
            "Paper size, orientation, margins, header and footer for PDF and printing.",
        ),
        |cx, _| {
            cx.effects.push(Effect::ShowPageSetup);
            Ok(Outcome::Done)
        },
    ));
    r.register(FnCommand::new(
        params(
            spec("file.import", File, "EX-020", "Import a document (docx, odt, rtf, epub, tex, rst, org, wiki, textile, opml, html) as Markdown next to it and open it."),
            r#"{"type":"object","properties":{"path":{"type":"string"}}}"#,
        ),
        |cx, p| {
            let Some(input) = path_param(p) else {
                cx.effects.push(Effect::ImportDialog);
                return Ok(Outcome::Done);
            };
            let ext = input.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
            let pandoc = pandoc(cx.app);
            if pandoc.is_none() && !matches!(ext.as_str(), "html" | "htm") {
                return Err(fail("file.import", "Pandoc is required to import this format (https://pandoc.org/installing.html)"));
            }
            let dir = input.parent().unwrap_or(Path::new(".")).to_owned();
            let stem = input.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "imported".into());
            let out = inkra_workspace::fileops::unique_name(&dir, &format!("{stem}.md"));
            cx.effects.push(Effect::RunJob(Box::new(ExportJob { kind: JobKind::Import { input, pandoc }, src: String::new(), out, open_after: false, parse: Default::default() })));
            Ok(Outcome::Done)
        },
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn export_commands_build_jobs_that_run() {
        let d = tempfile::tempdir().unwrap();
        let mut e = crate::Engine::new(crate::Preferences::default());
        e.state.editor = crate::editor::Editor::from_text("# Hi\n\nText $x^2$.\n");
        e.execute("file.exportHtml", &json!({})).unwrap();
        assert!(
            matches!(e.take_effects().as_slice(), [Effect::ExportDialog { format: ExportFormat::Html, name, .. }] if name.ends_with(".html"))
        );
        for (cmd, file) in [
            ("file.exportHtml", "a.html"),
            ("file.exportPdf", "a.pdf"),
            ("file.exportImage", "a.png"),
            ("file.exportHtmlPlain", "b.html"),
        ] {
            let out = d.path().join(file);
            e.execute(cmd, &json!({ "path": out })).unwrap();
            let effects = e.take_effects();
            let [Effect::RunJob(job)] = effects.as_slice() else {
                panic!("{effects:?}")
            };
            assert_eq!(run(job).unwrap(), JobResult::Exported(vec![out.clone()]));
            assert!(std::fs::metadata(&out).unwrap().len() > 30, "{cmd}");
        }
        assert!(
            std::fs::read_to_string(d.path().join("a.html"))
                .unwrap()
                .contains("<style>")
        );
        assert!(
            !std::fs::read_to_string(d.path().join("b.html"))
                .unwrap()
                .contains("<style>")
        );
        // Import of HTML works without Pandoc.
        let html = d.path().join("page.html");
        std::fs::write(&html, "<h2>Imported</h2>").unwrap();
        e.execute("file.import", &json!({ "path": html })).unwrap();
        let effects = e.take_effects();
        let [Effect::RunJob(job)] = effects.as_slice() else {
            panic!()
        };
        let JobResult::Imported(md) = run(job).unwrap() else {
            panic!()
        };
        assert_eq!(md, d.path().join("page.md"));
        assert_eq!(std::fs::read_to_string(md).unwrap(), "## Imported");
    }

    #[test]
    fn blocks_copy_and_export() {
        let d = tempfile::tempdir().unwrap();
        let mut e = crate::Engine::new(crate::Preferences::default());
        e.state.editor = crate::editor::Editor::from_text(
            "a\n\n$$\nx^2\n$$\n\n```mermaid\npie\n\"a\" : 1\n```\n",
        );
        e.state.editor.sel = inkra_doc::Selection::caret(0);
        assert!(!e.is_enabled("edit.copyBlockAsSvg"));
        e.state.editor.sel = inkra_doc::Selection::caret(7);
        e.execute("edit.copyBlockAsSvg", &json!({})).unwrap();
        assert!(
            matches!(e.take_effects().as_slice(), [Effect::SetClipboard { text, .. }] if text.starts_with("<svg"))
        );
        let pie = e.state.editor.text().find("pie").unwrap();
        e.state.editor.sel = inkra_doc::Selection::caret(pie + 1);
        e.execute("edit.copyBlockAsPng", &json!({})).unwrap();
        assert!(
            matches!(e.take_effects().as_slice(), [Effect::SetClipboardImage(png)] if png.starts_with(b"\x89PNG"))
        );
        let out = d.path().join("pie.svg");
        e.execute("file.exportBlockImage", &json!({ "path": out }))
            .unwrap();
        assert!(
            std::fs::read_to_string(out)
                .unwrap()
                .contains("inkra-diagram")
        );
    }

    #[test]
    fn custom_export_runs_the_template() {
        let d = tempfile::tempdir().unwrap();
        let mut e = crate::Engine::new(crate::Preferences::default());
        e.state.editor = crate::editor::Editor::from_text("# Custom\n");
        let cp = if cfg!(windows) {
            "cmd /C copy $input $output"
        } else {
            "cp $input $output"
        };
        assert!(
            e.state
                .prefs
                .set("export.custom", json!([format!("Copy|txt|{cp}"), "broken"]))
        );
        assert_eq!(custom_exports(&e.state).len(), 1);
        let out = d.path().join("c.txt");
        e.execute("file.exportCustom", &json!({ "index": 0, "path": out }))
            .unwrap();
        let effects = e.take_effects();
        let [Effect::RunJob(job)] = effects.as_slice() else {
            panic!("{effects:?}")
        };
        run(job).unwrap();
        assert_eq!(std::fs::read_to_string(&out).unwrap(), "# Custom\n");
    }

    #[test]
    fn page_setup_from_preferences() {
        let mut e = crate::Engine::new(crate::Preferences::default());
        assert!(e.state.prefs.set("export.pdf.paper", json!("Letter")));
        assert!(
            e.state
                .prefs
                .set("export.pdf.orientation", json!("landscape"))
        );
        let p = page_setup(&e.state);
        assert_eq!((p.width, p.height), (792.0, 612.0));
        assert!((p.margins[0] - 20.0 * 72.0 / 25.4).abs() < 0.01);
    }
}
