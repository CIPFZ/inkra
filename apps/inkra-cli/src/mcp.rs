//! `inkra-cli mcp` (CM-003): the commands as MCP tools on stdio.
//!
//! Headless by default: the engine runs in this process, and files are limited
//! to `--automation-read-root` / `--automation-write-root` (the current folder
//! when not given; design section 8). `--attach` drives a running Inkra through
//! its control channel instead.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use inkra_engine::control::{self as proto, ConnectionInfo, Host, RpcError, code};
use inkra_engine::{Effect, Engine, Preferences};
use serde_json::{Value, json};

/// The MCP protocol revision implemented.
const MCP_VERSION: &str = "2025-06-18";

/// Where tool calls go.
trait Backend {
    fn call(&mut self, method: &str, params: &Value) -> Result<Value, RpcError>;
    fn mode(&self) -> &'static str;
}

// ---- headless ----

/// Commands that leave the document and its files: links, file managers,
/// printing, settings files, user-configured programs (design section 8).
const DENIED: &[&str] = &[
    "app.openUrl",
    "app.openFolder",
    "app.importSettings",
    "app.quit",
    "file.print",
    "file.revealInFolder",
    "file.exportCustom",
    "sidebar.reveal",
    "image.reveal",
    "image.open",
    "image.openWindow",
];

/// Preferences naming external programs.
const PROGRAM_PREFS: &[&str] = &["export.pandocPath", "export.custom", "markdown.mermaidCli"];

/// Commands whose `path` / `dest` parameters are read.
const READS: &[&str] = &[
    "file.open",
    "file.openRecent",
    "edit.insertFileLink",
    "image.relink",
    "sidebar.openInNewWindow",
    "sidebar.setRoot",
    "sidebar.copyPath",
    "format.insertImageFile",
];

/// Commands that write files next to the current document (or its files).
const WRITES_NEAR_DOC: &[&str] = &[
    "file.save",
    "file.saveAll",
    "file.quickExport",
    "file.delete",
    "file.rename",
    "file.moveTo",
    "file.exportBlockImage",
    "format.insertImageFile",
    "image.toLocal",
    "image.delete",
];

/// The folders the headless engine may read and write.
#[derive(Debug, Clone)]
pub struct Roots {
    read: Vec<PathBuf>,
    write: Vec<PathBuf>,
    cwd: PathBuf,
}

impl Roots {
    /// Roots from the options; the current folder for any that is missing.
    pub fn new(read: &[PathBuf], write: &[PathBuf], cwd: &Path) -> Result<Self, String> {
        let canon = |list: &[PathBuf]| -> Result<Vec<PathBuf>, String> {
            let list = if list.is_empty() {
                vec![cwd.to_owned()]
            } else {
                list.to_vec()
            };
            list.iter()
                .map(|p| {
                    std::fs::canonicalize(cwd.join(p)).map_err(|e| format!("{}: {e}", p.display()))
                })
                .collect()
        };
        let write = canon(write)?;
        // Whatever may be written may also be read back.
        let mut read = canon(read)?;
        read.extend(write.iter().cloned());
        Ok(Self {
            read,
            write,
            cwd: cwd.to_owned(),
        })
    }

    /// `p` as an absolute path with symlinks resolved (for a new file: its
    /// folder resolved).
    fn resolve(&self, p: &str) -> Result<PathBuf, RpcError> {
        let abs = self.cwd.join(p);
        if let Ok(c) = std::fs::canonicalize(&abs) {
            return Ok(c);
        }
        match (abs.parent(), abs.file_name()) {
            (Some(dir), Some(name)) => std::fs::canonicalize(dir)
                .map(|d| d.join(name))
                .map_err(|_| RpcError::new(code::FAILED, format!("{p}: no such folder"))),
            _ => Err(RpcError::new(code::FAILED, format!("{p}: not a file path"))),
        }
    }

    fn check(&self, p: &Path, write: bool) -> Result<(), RpcError> {
        let roots = if write { &self.write } else { &self.read };
        if roots.iter().any(|r| p.starts_with(r)) {
            Ok(())
        } else {
            Err(RpcError::new(
                code::FAILED,
                format!(
                    "{} is outside the automation {} root (--automation-{}-root)",
                    p.display(),
                    if write { "write" } else { "read" },
                    if write { "write" } else { "read" },
                ),
            ))
        }
    }

    /// Whether `command` with `params` stays inside the roots; returns the
    /// parameters with paths made absolute.
    pub fn allow(&self, engine: &Engine, command: &str, params: &Value) -> Result<Value, RpcError> {
        if DENIED.contains(&command) {
            return Err(RpcError::new(
                code::FAILED,
                format!("`{command}` is not available in headless automation"),
            ));
        }
        if command == "app.setPreference"
            && params
                .get("key")
                .and_then(Value::as_str)
                .is_some_and(|k| PROGRAM_PREFS.contains(&k))
        {
            return Err(RpcError::new(
                code::FAILED,
                "preferences naming programs cannot be set in headless automation",
            ));
        }
        for key in ["name", "file"] {
            if let Some(n) = params.get(key).and_then(Value::as_str)
                && (n.contains(['/', '\\']) || n == ".." || n == ".")
            {
                return Err(RpcError::params(format!(
                    "`{key}` must be a plain file name"
                )));
            }
        }
        let mut out = params.clone();
        let write = !READS.contains(&command);
        for key in ["path", "dest"] {
            if let Some(p) = params.get(key).and_then(Value::as_str) {
                let abs = self.resolve(p)?;
                // Importing reads the file and writes the Markdown beside it.
                self.check(&abs, write || command == "file.import")?;
                out[key] = json!(abs);
            }
        }
        let near_doc = WRITES_NEAR_DOC.contains(&command)
            || (command.starts_with("file.export")
                && params.get("sameFolder").and_then(Value::as_bool) == Some(true))
            || (command == "format.insertImageFile");
        if near_doc {
            let s = &engine.state;
            let docs: Vec<&PathBuf> = if command == "file.saveAll" {
                std::iter::once(&s.editor.path)
                    .chain(s.others.iter().map(|p| &p.editor.path))
                    .flatten()
                    .collect()
            } else {
                s.editor.path.iter().collect()
            };
            for d in docs {
                let dir = d.parent().unwrap_or(d);
                let dir = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_owned());
                self.check(&dir, true)?;
            }
        }
        Ok(out)
    }
}

/// The engine in this process.
struct Headless {
    engine: Engine,
    roots: Roots,
}

impl Host for Headless {
    fn engine(&mut self) -> &mut Engine {
        &mut self.engine
    }

    fn apply(&mut self, effects: Vec<Effect>) -> Result<(), RpcError> {
        for e in effects {
            match e {
                // Exports run here, synchronously.
                Effect::RunJob(job) => {
                    inkra_engine::exporting::run(&job)
                        .map_err(|m| RpcError::new(code::FAILED, m))?;
                }
                Effect::Error(m) => return Err(RpcError::new(code::FAILED, m)),
                _ => {}
            }
        }
        Ok(())
    }

    fn allow(&mut self, command: &str, params: &Value) -> Result<Value, RpcError> {
        self.roots.allow(&self.engine, command, params)
    }
}

impl Backend for Headless {
    fn call(&mut self, method: &str, params: &Value) -> Result<Value, RpcError> {
        let mut params = params.clone();
        // `open` / `save` paths go through the same root checks as commands.
        match method {
            "open" | "save" => {
                if let Some(p) = params.get("path").and_then(Value::as_str) {
                    let abs = self.roots.resolve(p)?;
                    self.roots.check(&abs, method == "save")?;
                    params["path"] = json!(abs);
                }
            }
            "screenshot" | "click" | "key" => {
                return Err(RpcError::new(
                    code::FAILED,
                    format!(
                        "`{method}` needs a window: run `inkra-cli mcp --attach` with a running Inkra"
                    ),
                ));
            }
            _ => {}
        }
        proto::handle(self, method, &params).unwrap_or_else(|| {
            Err(RpcError::new(
                code::METHOD_NOT_FOUND,
                format!("no method `{method}`"),
            ))
        })
    }

    fn mode(&self) -> &'static str {
        "headless"
    }
}

// ---- attached ----

struct Attached {
    out: TcpStream,
    input: BufReader<TcpStream>,
    next: u64,
}

impl Attached {
    fn connect(data_dir: &Path, pid: Option<u32>) -> Result<Self, String> {
        let dir = proto::control_dir(data_dir);
        // Stale files (killed instances) are removed on the way.
        let info = ConnectionInfo::prune(&dir)
            .into_iter()
            .find(|i| pid.is_none_or(|p| p == i.pid))
            .ok_or_else(|| {
                format!(
                    "no running Inkra with a control channel in {} (start it with `inkra --control 0`)",
                    dir.display()
                )
            })?;
        let s = TcpStream::connect(("127.0.0.1", info.port))
            .map_err(|e| format!("cannot reach Inkra (pid {}): {e}", info.pid))?;
        let input = BufReader::new(s.try_clone().map_err(|e| e.to_string())?);
        let mut a = Self {
            out: s,
            input,
            next: 0,
        };
        a.call(
            "hello",
            &json!({ "token": info.token, "client": "inkra-cli mcp" }),
        )
        .map_err(|e| e.to_string())?;
        Ok(a)
    }
}

impl Backend for Attached {
    fn call(&mut self, method: &str, params: &Value) -> Result<Value, RpcError> {
        self.next += 1;
        let lost = |e: std::io::Error| RpcError::new(code::FAILED, format!("connection lost: {e}"));
        writeln!(self.out, "{}", proto::request(self.next, method, params)).map_err(lost)?;
        let mut line = String::new();
        if self.input.read_line(&mut line).map_err(lost)? == 0 {
            return Err(RpcError::new(code::FAILED, "Inkra closed the connection"));
        }
        proto::parse_response(&line)
    }

    fn mode(&self) -> &'static str {
        "attached"
    }
}

// ---- MCP ----

fn tools() -> Value {
    let obj = |props: Value, required: &[&str]| json!({ "type": "object", "properties": props, "required": required });
    let window =
        json!({ "type": "integer", "description": "Window id (default: the active window)" });
    json!([
        { "name": "inkra_list_commands", "description": "List every Inkra command with its parameters (JSON Schema), shortcut and whether it is enabled now.",
          "inputSchema": obj(json!({ "window": window }), &[]) },
        { "name": "inkra_execute", "description": "Run one Inkra command (see inkra_list_commands, or the inkra://commands resource) like a menu click. Commands that would open a dialog fail; pass their parameters, e.g. `path`.",
          "inputSchema": obj(json!({ "command": { "type": "string" }, "params": { "type": "object" }, "window": window }), &["command"]) },
        { "name": "inkra_batch", "description": "Run several commands in order; stops at the first error unless stopOnError is false.",
          "inputSchema": obj(json!({ "calls": { "type": "array", "items": { "type": "object", "properties": { "command": { "type": "string" }, "params": { "type": "object" } }, "required": ["command"] } }, "stopOnError": { "type": "boolean" }, "window": window }), &["calls"]) },
        { "name": "inkra_inspect", "description": "Describe the document: path, dirty, mode, encoding; optionally its text, selection, outline, stats and top-level blocks.",
          "inputSchema": obj(json!({ "include": { "type": "array", "items": { "enum": ["text", "selection", "outline", "stats", "blocks"] } }, "window": window }), &[]) },
        { "name": "inkra_open", "description": "Open a Markdown file.",
          "inputSchema": obj(json!({ "path": { "type": "string" }, "window": { "enum": ["current", "new"] } }), &["path"]) },
        { "name": "inkra_save", "description": "Save the document, or save it as `path`.",
          "inputSchema": obj(json!({ "path": { "type": "string" }, "window": window }), &[]) },
        { "name": "inkra_screenshot", "description": "A PNG of the Inkra window (attached mode only).",
          "inputSchema": obj(json!({}), &[]) },
        { "name": "inkra_click", "description": "Click at window coordinates in logical pixels (attached mode only).",
          "inputSchema": obj(json!({ "x": { "type": "number" }, "y": { "type": "number" }, "button": { "enum": ["primary", "secondary", "middle"] }, "count": { "type": "integer", "minimum": 1, "maximum": 3 }, "modifiers": { "type": "array", "items": { "enum": ["ctrl", "cmd", "shift", "alt"] } } }), &["x", "y"]) },
        { "name": "inkra_key", "description": "Press a key chord such as `Ctrl+B` or `Enter`, and/or type text (attached mode only).",
          "inputSchema": obj(json!({ "key": { "type": "string" }, "text": { "type": "string" } }), &[]) },
    ])
}

fn tool_method(name: &str) -> Option<&'static str> {
    Some(match name {
        "inkra_list_commands" => "commands",
        "inkra_execute" => "execute",
        "inkra_batch" => "batch",
        "inkra_inspect" => "inspect_document",
        "inkra_open" => "open",
        "inkra_save" => "save",
        "inkra_screenshot" => "screenshot",
        "inkra_click" => "click",
        "inkra_key" => "key",
        _ => return None,
    })
}

const COMMANDS_MD: &str = include_str!("../../../docs/commands.md");

/// Handles one MCP message; `None` for notifications.
fn handle(b: &mut dyn Backend, msg: &Value) -> Option<Value> {
    let id = msg.get("id").cloned()?;
    let method = msg
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let p = msg.get("params").cloned().unwrap_or(json!({}));
    let result: Result<Value, RpcError> = match method {
        "initialize" => Ok(json!({
            "protocolVersion": MCP_VERSION,
            "capabilities": { "tools": {}, "resources": {} },
            "serverInfo": { "name": "inkra", "version": env!("CARGO_PKG_VERSION") },
            "instructions": format!("Inkra Markdown editor ({} mode). Every edit is an Inkra command: list them with inkra_list_commands, run them with inkra_execute.", b.mode()),
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tools() })),
        "tools/call" => {
            let name = p.get("name").and_then(Value::as_str).unwrap_or_default();
            let args = p.get("arguments").cloned().unwrap_or(json!({}));
            match tool_method(name) {
                None => Err(RpcError::params(format!("no tool `{name}`"))),
                // Tool errors are results with isError, so the model sees them.
                Some(m) => Ok(match b.call(m, &args) {
                    Ok(v) if m == "screenshot" => json!({ "content": [{
                        "type": "image",
                        "data": v["png"],
                        "mimeType": "image/png",
                    }] }),
                    Ok(v) => json!({ "content": [{ "type": "text", "text": v.to_string() }] }),
                    Err(e) => {
                        json!({ "content": [{ "type": "text", "text": e.to_string() }], "isError": true })
                    }
                }),
            }
        }
        "resources/list" => Ok(json!({ "resources": [
            { "uri": "inkra://commands", "name": "Inkra commands", "mimeType": "text/markdown",
              "description": "Every command with its parameters (docs/commands.md)" },
            { "uri": "inkra://document/active", "name": "Active document", "mimeType": "text/markdown" },
        ] })),
        "resources/read" => {
            let uri = p.get("uri").and_then(Value::as_str).unwrap_or_default();
            let text = if uri == "inkra://commands" {
                Ok(COMMANDS_MD.to_owned())
            } else if let Some(w) = uri.strip_prefix("inkra://document/") {
                let mut q = json!({ "include": ["text"] });
                if let Ok(n) = w.parse::<u64>() {
                    q["window"] = json!(n);
                }
                b.call("inspect_document", &q)
                    .map(|v| v["text"].as_str().unwrap_or_default().to_owned())
            } else {
                Err(RpcError::params(format!("no resource `{uri}`")))
            };
            text.map(
                |t| json!({ "contents": [{ "uri": uri, "mimeType": "text/markdown", "text": t }] }),
            )
        }
        m => Err(RpcError::new(
            code::METHOD_NOT_FOUND,
            format!("no method `{m}`"),
        )),
    };
    Some(serde_json::from_str(&proto::response(&id, &result)).unwrap_or(Value::Null))
}

fn serve(b: &mut dyn Backend, input: impl BufRead, mut out: impl Write) -> std::io::Result<()> {
    for line in input.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(&line) {
            Ok(msg) => handle(b, &msg),
            Err(e) => Some(
                json!({ "jsonrpc": "2.0", "id": null, "error": { "code": code::PARSE, "message": e.to_string() } }),
            ),
        };
        if let Some(r) = reply {
            writeln!(out, "{r}")?;
            out.flush()?;
        }
    }
    Ok(())
}

/// `inkra-cli mcp [--attach [--pid N] [--config DIR]] [--automation-read-root DIR]... [--automation-write-root DIR]...`
pub fn run(args: &[String]) -> ExitCode {
    let mut attach = false;
    let mut pid = None;
    let mut config: Option<PathBuf> = None;
    let (mut read, mut write) = (Vec::new(), Vec::new());
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--attach" => attach = true,
            "--config" => match it.next() {
                Some(d) => config = Some(PathBuf::from(d)),
                None => return usage("--config needs a folder"),
            },
            "--pid" => match it.next().and_then(|v| v.parse().ok()) {
                Some(p) => pid = Some(p),
                None => return usage("--pid needs a process id"),
            },
            "--automation-read-root" | "--automation-write-root" => match it.next() {
                Some(d) if a.ends_with("read-root") => read.push(PathBuf::from(d)),
                Some(d) => write.push(PathBuf::from(d)),
                None => return usage(&format!("{a} needs a folder")),
            },
            other => return usage(&format!("unknown option `{other}`")),
        }
    }
    let mut backend: Box<dyn Backend> = if attach {
        if !read.is_empty() || !write.is_empty() {
            return usage(
                "the automation roots apply to headless mode; --attach acts as the GUI user",
            );
        }
        // `--config` as given to `inkra --config <dir>`.
        let paths = config.as_ref().map_or_else(
            inkra_workspace::Paths::platform,
            inkra_workspace::Paths::rooted,
        );
        match Attached::connect(&paths.data, pid) {
            Ok(a) => Box::new(a),
            Err(e) => {
                eprintln!("inkra-cli mcp: {e}");
                return ExitCode::from(2);
            }
        }
    } else {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        match Roots::new(&read, &write, &cwd) {
            Ok(roots) => Box::new(Headless {
                engine: Engine::new(Preferences::default()),
                roots,
            }),
            Err(e) => {
                eprintln!("inkra-cli mcp: {e}");
                return ExitCode::from(2);
            }
        }
    };
    let stdin = std::io::stdin();
    match serve(backend.as_mut(), stdin.lock(), std::io::stdout().lock()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("inkra-cli mcp: {e}");
            ExitCode::from(2)
        }
    }
}

fn usage(msg: &str) -> ExitCode {
    eprintln!("inkra-cli mcp: {msg}");
    ExitCode::from(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headless(dir: &Path) -> Headless {
        Headless {
            engine: Engine::new(Preferences::default()),
            roots: Roots::new(&[], &[], dir).unwrap(),
        }
    }

    fn tool(b: &mut dyn Backend, name: &str, args: Value) -> Value {
        let msg = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": name, "arguments": args } });
        handle(b, &msg).unwrap()["result"].clone()
    }

    fn ok_text(r: &Value) -> Value {
        assert!(r.get("isError").is_none(), "{r}");
        serde_json::from_str(r["content"][0]["text"].as_str().unwrap()).unwrap()
    }

    #[test]
    fn mcp_session_over_stdio() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("a.md"), "# A\n").unwrap();
        let mut b = headless(d.path());
        let input = [
            json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": MCP_VERSION } }),
            json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
            json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }),
            json!({ "jsonrpc": "2.0", "id": 3, "method": "resources/read", "params": { "uri": "inkra://commands" } }),
            json!({ "jsonrpc": "2.0", "id": 4, "method": "nope" }),
        ]
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n");
        let mut out = Vec::new();
        serve(&mut b, input.as_bytes(), &mut out).unwrap();
        let lines: Vec<Value> = String::from_utf8(out)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(lines.len(), 4, "no reply to the notification");
        assert_eq!(lines[0]["result"]["serverInfo"]["name"], "inkra");
        assert_eq!(lines[1]["result"]["tools"].as_array().unwrap().len(), 9);
        assert!(
            lines[2]["result"]["contents"][0]["text"]
                .as_str()
                .unwrap()
                .contains("format.bold")
        );
        assert_eq!(lines[3]["error"]["code"], code::METHOD_NOT_FOUND);
    }

    #[test]
    fn headless_edits_and_saves_inside_the_root() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("a.md"), "# A\n").unwrap();
        let mut b = headless(d.path());
        ok_text(&tool(&mut b, "inkra_open", json!({ "path": "a.md" })));
        ok_text(&tool(
            &mut b,
            "inkra_batch",
            json!({ "calls": [
            { "command": "edit.selectAll" },
            { "command": "edit.insertText", "params": { "text": "# B\n\nbody\n" } },
        ] }),
        ));
        let v = ok_text(&tool(
            &mut b,
            "inkra_inspect",
            json!({ "include": ["text", "outline"] }),
        ));
        assert_eq!(v["outline"][0]["text"], "B");
        ok_text(&tool(&mut b, "inkra_save", json!({})));
        assert_eq!(
            std::fs::read_to_string(d.path().join("a.md")).unwrap(),
            "# B\n\nbody\n"
        );
        ok_text(&tool(&mut b, "inkra_save", json!({ "path": "copy.md" })));
        assert!(d.path().join("copy.md").is_file());
        ok_text(&tool(
            &mut b,
            "inkra_execute",
            json!({ "command": "file.exportHtml", "params": { "path": "out.html" } }),
        ));
        assert!(
            std::fs::read_to_string(d.path().join("out.html"))
                .unwrap()
                .contains("<h1")
        );
        let s = tool(&mut b, "inkra_screenshot", json!({}));
        assert_eq!(s["isError"], true);
    }

    #[test]
    fn headless_refuses_files_outside_the_roots() {
        let outer = tempfile::tempdir().unwrap();
        let root = outer.path().join("root");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(outer.path().join("secret.md"), "s").unwrap();
        std::fs::write(root.join("a.md"), "a").unwrap();
        let mut b = headless(&root);
        let err = |r: Value| {
            assert_eq!(r["isError"], true, "{r}");
            r["content"][0]["text"].as_str().unwrap().to_owned()
        };
        let e = err(tool(
            &mut b,
            "inkra_open",
            json!({ "path": "../secret.md" }),
        ));
        assert!(e.contains("outside the automation read root"), "{e}");
        err(tool(
            &mut b,
            "inkra_open",
            json!({ "path": outer.path().join("secret.md") }),
        ));
        ok_text(&tool(&mut b, "inkra_open", json!({ "path": "a.md" })));
        err(tool(&mut b, "inkra_save", json!({ "path": "../out.md" })));
        err(tool(
            &mut b,
            "inkra_execute",
            json!({ "command": "file.exportPdf", "params": { "path": "../x.pdf" } }),
        ));
        err(tool(
            &mut b,
            "inkra_execute",
            json!({ "command": "sidebar.move", "params": { "path": "a.md", "dest": ".." } }),
        ));
        err(tool(
            &mut b,
            "inkra_execute",
            json!({ "command": "file.rename", "params": { "name": "../evil.md" } }),
        ));
        err(tool(
            &mut b,
            "inkra_execute",
            json!({ "command": "app.openUrl", "params": { "url": "https://example.com" } }),
        ));
        err(tool(
            &mut b,
            "inkra_execute",
            json!({ "command": "app.setPreference", "params": { "key": "export.pandocPath", "value": "/bin/sh" } }),
        ));
        assert!(!outer.path().join("out.md").exists() && !outer.path().join("x.pdf").exists());
        // Symlinks out of the root count as outside.
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(outer.path().join("secret.md"), root.join("link.md"))
                .unwrap();
            err(tool(&mut b, "inkra_open", json!({ "path": "link.md" })));
        }
        // A document outside the write root cannot be saved in place, even
        // when the read root allows opening it.
        let mut r = Headless {
            engine: Engine::new(Preferences::default()),
            roots: Roots::new(
                &[outer.path().to_owned()],
                std::slice::from_ref(&root),
                &root,
            )
            .unwrap(),
        };
        ok_text(&tool(
            &mut r,
            "inkra_open",
            json!({ "path": outer.path().join("secret.md") }),
        ));
        ok_text(&tool(
            &mut r,
            "inkra_execute",
            json!({ "command": "edit.insertText", "params": { "text": "x" } }),
        ));
        let e = err(tool(&mut r, "inkra_save", json!({})));
        assert!(e.contains("write root"), "{e}");
        assert_eq!(
            std::fs::read_to_string(outer.path().join("secret.md")).unwrap(),
            "s"
        );
    }
}
