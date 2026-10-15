//! Control channel protocol (CM-003, `docs/design/control-protocol.md`).
//!
//! The parts shared by the GUI listener (`inkra-ui::control`) and
//! `inkra-cli mcp`: newline-delimited JSON-RPC 2.0 messages, error codes, the
//! connection file with its token, and the methods that only need the
//! [`Engine`] (`hello`, `commands`, `execute`, `batch`, `inspect_document`,
//! `open`, `save`). Every change goes through the command registry, exactly
//! like a menu click; the host applies the effects it can and rejects calls
//! that would need a dialog.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::command::{CommandError, Outcome};
use crate::{Effect, Engine};

/// Protocol version (connection file and `hello`).
pub const VERSION: u32 = 1;
/// Longest accepted request line (16 MiB).
pub const MAX_LINE: usize = 16 << 20;
/// Simultaneous connections.
pub const MAX_CONNECTIONS: usize = 4;
/// How long a request may wait for the UI thread.
pub const TIMEOUT: Duration = Duration::from_secs(30);

/// JSON-RPC and application error codes (design section 3).
pub mod code {
    /// The line is not JSON.
    pub const PARSE: i64 = -32700;
    /// Not a JSON-RPC request object.
    pub const INVALID_REQUEST: i64 = -32600;
    /// No such method.
    pub const METHOD_NOT_FOUND: i64 = -32601;
    /// Bad parameters.
    pub const INVALID_PARAMS: i64 = -32602;
    /// `hello` missing or wrong token.
    pub const UNAUTHENTICATED: i64 = 1;
    /// No such command.
    pub const UNKNOWN_COMMAND: i64 = 2;
    /// The command is disabled in the current state.
    pub const DISABLED: i64 = 3;
    /// The command failed, needs user interaction, or is not allowed.
    pub const FAILED: i64 = 4;
    /// No such window.
    pub const NO_WINDOW: i64 = 5;
    /// The UI did not answer in time.
    pub const TIMEOUT: i64 = 6;
}

/// An error result.
#[derive(Debug, Clone, PartialEq)]
pub struct RpcError {
    /// One of [`code`].
    pub code: i64,
    /// Human-readable reason.
    pub message: String,
}

impl RpcError {
    /// An error with `code`.
    pub fn new(code: i64, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    /// Bad parameters.
    pub fn params(message: impl Into<String>) -> Self {
        Self::new(code::INVALID_PARAMS, message)
    }
}

impl std::fmt::Display for RpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} (code {})", self.message, self.code)
    }
}

impl From<CommandError> for RpcError {
    fn from(e: CommandError) -> Self {
        let c = match &e {
            CommandError::Unknown(_) => code::UNKNOWN_COMMAND,
            CommandError::Disabled(_) => code::DISABLED,
            CommandError::InvalidParams { .. } => code::INVALID_PARAMS,
            CommandError::Failed { .. } | CommandError::Panicked { .. } => code::FAILED,
        };
        Self::new(c, e.to_string())
    }
}

/// A request.
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    /// Echoed in the response (`null` for notifications).
    pub id: Value,
    /// Method name.
    pub method: String,
    /// Parameters (`{}` when absent).
    pub params: Value,
}

/// Parses one request line. On failure returns the id (if readable) and the
/// error to send back.
pub fn parse_request(line: &str) -> Result<Request, (Value, RpcError)> {
    let v: Value = serde_json::from_str(line)
        .map_err(|e| (Value::Null, RpcError::new(code::PARSE, e.to_string())))?;
    let id = v.get("id").cloned().unwrap_or(Value::Null);
    let Some(method) = v.get("method").and_then(Value::as_str) else {
        return Err((id, RpcError::new(code::INVALID_REQUEST, "missing `method`")));
    };
    let params = match v.get("params") {
        None | Some(Value::Null) => json!({}),
        Some(p @ Value::Object(_)) => p.clone(),
        Some(_) => return Err((id, RpcError::params("`params` must be an object"))),
    };
    Ok(Request {
        id,
        method: method.to_owned(),
        params,
    })
}

/// A response line (without the trailing newline).
pub fn response(id: &Value, result: &Result<Value, RpcError>) -> String {
    let v = match result {
        Ok(r) => json!({ "jsonrpc": "2.0", "id": id, "result": r }),
        Err(e) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": e.code, "message": e.message },
        }),
    };
    v.to_string()
}

/// A request line (without the trailing newline).
pub fn request(id: u64, method: &str, params: &Value) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }).to_string()
}

/// The result of a response line.
pub fn parse_response(line: &str) -> Result<Value, RpcError> {
    let v: Value =
        serde_json::from_str(line).map_err(|e| RpcError::new(code::PARSE, e.to_string()))?;
    if let Some(e) = v.get("error") {
        return Err(RpcError::new(
            e.get("code")
                .and_then(Value::as_i64)
                .unwrap_or(code::FAILED),
            e.get("message")
                .and_then(Value::as_str)
                .unwrap_or("error")
                .to_owned(),
        ));
    }
    Ok(v.get("result").cloned().unwrap_or(Value::Null))
}

// ---- connection file ----

/// `<data_dir>/control/<pid>.json`: how to reach a running Inkra.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionInfo {
    /// Protocol version.
    pub version: u32,
    /// Process id.
    pub pid: u32,
    /// TCP port on the loopback interface.
    pub port: u16,
    /// Hex token required by `hello`.
    pub token: String,
    /// Application version.
    pub app: String,
}

/// The folder of the connection files.
pub fn control_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("control")
}

impl ConnectionInfo {
    /// A new connection with a fresh token for this process.
    pub fn new(port: u16) -> Result<Self, String> {
        Ok(Self {
            version: VERSION,
            pid: std::process::id(),
            port,
            token: new_token()?,
            app: env!("CARGO_PKG_VERSION").to_owned(),
        })
    }

    /// Where this connection's file lives.
    pub fn path(&self, dir: &Path) -> PathBuf {
        dir.join(format!("{}.json", self.pid))
    }

    /// Writes the file readable by the user only (Unix `0600` in a `0700`
    /// folder; on Windows the per-user data folder).
    pub fn write(&self, dir: &Path) -> std::io::Result<PathBuf> {
        std::fs::create_dir_all(dir)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
        }
        let path = self.path(dir);
        let _ = std::fs::remove_file(&path);
        let mut o = std::fs::OpenOptions::new();
        o.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            o.mode(0o600);
        }
        let mut f = o.open(&path)?;
        std::io::Write::write_all(
            &mut f,
            serde_json::to_string(self)
                .map_err(std::io::Error::other)?
                .as_bytes(),
        )?;
        Ok(path)
    }

    /// Every connection file in `dir` (newest first), skipping unreadable ones.
    pub fn list(dir: &Path) -> Vec<(PathBuf, Self)> {
        let mut out: Vec<(std::time::SystemTime, PathBuf, Self)> = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
            .filter_map(|e| {
                let p = e.path();
                let info: Self = serde_json::from_str(&std::fs::read_to_string(&p).ok()?).ok()?;
                let t = e.metadata().and_then(|m| m.modified()).ok()?;
                Some((t, p, info))
            })
            .collect();
        out.sort_by_key(|e| std::cmp::Reverse(e.0));
        out.into_iter().map(|(_, p, i)| (p, i)).collect()
    }
}

impl ConnectionInfo {
    /// Whether something still listens on the port (a crashed or killed Inkra
    /// leaves its file behind).
    pub fn alive(&self) -> bool {
        let addr = std::net::SocketAddr::from((std::net::Ipv4Addr::LOCALHOST, self.port));
        std::net::TcpStream::connect_timeout(&addr, Duration::from_millis(300)).is_ok()
    }

    /// Removes the files of connections that no longer answer; returns the
    /// live ones (newest first).
    pub fn prune(dir: &Path) -> Vec<Self> {
        Self::list(dir)
            .into_iter()
            .filter_map(|(path, info)| {
                if info.pid != std::process::id() && !info.alive() {
                    let _ = std::fs::remove_file(path);
                    None
                } else {
                    Some(info)
                }
            })
            .collect()
    }
}

/// 256 random bits from the operating system, as hex.
pub fn new_token() -> Result<String, String> {
    let mut b = [0u8; 32];
    getrandom::fill(&mut b).map_err(|e| format!("no system randomness: {e}"))?;
    Ok(b.iter().map(|x| format!("{x:02x}")).collect())
}

/// Compares tokens in time independent of where they differ.
pub fn token_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Standard base64 (PNG bytes in `screenshot` results and MCP image content).
pub fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for c in bytes.chunks(3) {
        let n = (u32::from(c[0]) << 16)
            | (u32::from(*c.get(1).unwrap_or(&0)) << 8)
            | u32::from(*c.get(2).unwrap_or(&0));
        for k in 0..4 {
            if k <= c.len() {
                out.push(T[(n >> (18 - 6 * k) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

// ---- engine-level methods ----

/// What a front end provides to run control requests.
pub trait Host {
    /// The engine.
    fn engine(&mut self) -> &mut Engine;
    /// Applies effects of a command (never called with ones that
    /// [`needs_interaction`]).
    fn apply(&mut self, effects: Vec<Effect>) -> Result<(), RpcError>;
    /// Whether `command` with `params` may run (headless file roots, denied
    /// commands), and the parameters to run it with (paths made absolute).
    /// The GUI allows everything a user could do.
    fn allow(&mut self, command: &str, params: &Value) -> Result<Value, RpcError> {
        let _ = command;
        Ok(params.clone())
    }
}

/// Effects that would open a dialog or ask the user: a control request gets
/// an error instead (design section 4).
pub fn needs_interaction(e: &Effect) -> bool {
    matches!(
        e,
        Effect::OpenDialog
            | Effect::OpenFolderDialog
            | Effect::SaveAsDialog { .. }
            | Effect::ExportDialog { .. }
            | Effect::PickImage
            | Effect::ShowTableDialog
            | Effect::ConfirmClose { .. }
            | Effect::ConfirmDelete(_)
            | Effect::RenameDialog
            | Effect::MoveDialog(_)
            | Effect::LinkDialog { .. }
            | Effect::ImageDialog(_)
            | Effect::SettingsDialog { .. }
            | Effect::ImportDialog
    )
}

fn outcome(o: Outcome) -> Value {
    match o {
        Outcome::Done => json!("done"),
        Outcome::Noop => json!("noop"),
        Outcome::Value(v) => json!({ "value": v }),
    }
}

fn str_param<'a>(p: &'a Value, key: &str) -> Result<&'a str, RpcError> {
    p.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| RpcError::params(format!("`{key}` (string) is required")))
}

fn window_param(p: &Value) -> Result<Option<u64>, RpcError> {
    match p.get("window") {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_u64()
            .map(Some)
            .ok_or_else(|| RpcError::params("`window` must be a window id")),
    }
}

/// Runs `f` with `window` active (when given), then makes the previously
/// active window active again.
fn in_window<H: Host + ?Sized, T>(
    host: &mut H,
    window: Option<u64>,
    f: impl FnOnce(&mut H) -> Result<T, RpcError>,
) -> Result<T, RpcError> {
    let prev = host.engine().state.active();
    if let Some(w) = window
        && !host.engine().state.activate(w)
    {
        return Err(RpcError::new(code::NO_WINDOW, format!("no window {w}")));
    }
    let r = f(host);
    if host.engine().state.window_ids().contains(&prev) {
        host.engine().state.activate(prev);
    }
    r
}

/// Runs one command in the active window like a menu click.
fn execute_here<H: Host + ?Sized>(
    host: &mut H,
    command: &str,
    params: &Value,
) -> Result<Value, RpcError> {
    let params = host.allow(command, params)?;
    let r = host.engine().execute(command, &params);
    let effects = host.engine().take_effects();
    let (asks, rest): (Vec<Effect>, Vec<Effect>) = effects.into_iter().partition(needs_interaction);
    host.apply(rest)?;
    let o = r?;
    if !asks.is_empty() {
        return Err(RpcError::new(
            code::FAILED,
            format!(
                "`{command}` needs user interaction (a dialog); pass the missing parameters, e.g. `path`"
            ),
        ));
    }
    Ok(outcome(o))
}

/// The windows: id, file, active.
pub fn windows(engine: &Engine) -> Value {
    let s = &engine.state;
    let active = s.active();
    let mut out = vec![json!({
        "id": active,
        "path": s.editor.path,
        "dirty": s.editor.doc.is_dirty(),
        "active": true,
    })];
    for p in &s.others {
        out.push(json!({
            "id": p.meta.id,
            "path": p.editor.path,
            "dirty": p.editor.doc.is_dirty(),
            "active": false,
        }));
    }
    Value::Array(out)
}

/// Every command with its state in the active window.
pub fn commands(engine: &mut Engine) -> Value {
    let en = inkra_i18n::I18n::new("en-US");
    let os = crate::keymap::Os::current();
    let specs: Vec<crate::command::CommandSpec> =
        engine.registry.iter().map(|c| c.spec().clone()).collect();
    let mut out = Vec::new();
    for s in specs {
        let shortcut = engine.keymap.chord_for(s.id).map(|c| c.label(os));
        out.push(json!({
            "id": s.id,
            "title": en.tr(&s.title_key()),
            "category": s.category.name(),
            "params": s.params.and_then(|p| serde_json::from_str::<Value>(p).ok()),
            "description": s.doc,
            "shortcut": shortcut,
            "enabled": engine.is_enabled(s.id),
            "checked": engine.is_checked(s.id),
        }));
    }
    Value::Array(out)
}

/// `inspect_document` for the active window.
fn inspect(engine: &Engine, include: &[String]) -> Value {
    let e = &engine.state.editor;
    let doc = &e.doc;
    let wants = |k: &str| include.iter().any(|i| i == k);
    let mut v = json!({
        "window": engine.state.active(),
        "path": e.path,
        "dirty": doc.is_dirty(),
        "mode": match e.mode {
            crate::editor::ViewMode::Live => "live",
            crate::editor::ViewMode::Source => "source",
        },
        "encoding": doc.encoding.label(),
        "bom": doc.bom,
        "newline": match doc.line_ending {
            inkra_text::eol::LineEnding::Lf => "lf",
            inkra_text::eol::LineEnding::Crlf => "crlf",
            inkra_text::eol::LineEnding::Cr => "cr",
        },
        "length": doc.text().len(),
    });
    if wants("text") {
        v["text"] = json!(doc.text().to_string());
    }
    if wants("selection") {
        v["selection"] = json!({ "anchor": e.sel.anchor, "head": e.sel.head });
    }
    if wants("outline") || wants("stats") {
        let r = crate::analysis::inspect(doc).to_json();
        if wants("outline") {
            v["outline"] = r["headings"].clone();
        }
        if wants("stats") {
            v["stats"] = r["counts"].clone();
        }
    }
    if wants("blocks") {
        v["blocks"] = doc
            .ast()
            .to_blocks()
            .iter()
            .map(|b| json!({ "kind": b.kind.name(), "start": b.span.start, "end": b.span.end }))
            .collect();
    }
    v
}

/// Handles the engine-level methods; `None` for others (`screenshot`, `click`,
/// `key` are the GUI's).
pub fn handle<H: Host + ?Sized>(
    host: &mut H,
    method: &str,
    params: &Value,
) -> Option<Result<Value, RpcError>> {
    let p = params;
    Some(match method {
        "hello" => Ok(json!({
            "version": VERSION,
            "app": env!("CARGO_PKG_VERSION"),
            "windows": windows(host.engine()),
        })),
        "commands" => {
            window_param(p).and_then(|w| in_window(host, w, |h| Ok(commands(h.engine()))))
        }
        "execute" => (|| {
            let command = str_param(p, "command")?.to_owned();
            let args = p.get("params").cloned().unwrap_or(Value::Null);
            in_window(host, window_param(p)?, |h| execute_here(h, &command, &args))
        })(),
        "batch" => (|| {
            let calls = p
                .get("calls")
                .and_then(Value::as_array)
                .ok_or_else(|| RpcError::params("`calls` (array) is required"))?
                .clone();
            let stop = p
                .get("stopOnError")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            in_window(host, window_param(p)?, |h| {
                let mut out = Vec::new();
                for c in &calls {
                    let r = str_param(c, "command").and_then(|command| {
                        let args = c.get("params").cloned().unwrap_or(Value::Null);
                        execute_here(h, command, &args)
                    });
                    let failed = r.is_err();
                    out.push(match r {
                        Ok(v) => json!({ "result": v }),
                        Err(e) => json!({ "error": { "code": e.code, "message": e.message } }),
                    });
                    if failed && stop {
                        break;
                    }
                }
                Ok(Value::Array(out))
            })
        })(),
        "inspect_document" => (|| {
            let include: Vec<String> = p
                .get("include")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default();
            in_window(host, window_param(p)?, |h| {
                Ok(inspect(h.engine(), &include))
            })
        })(),
        "open" => (|| {
            let path = str_param(p, "path")?;
            let mut args = json!({ "path": path });
            if let Some(w) = p.get("window").and_then(Value::as_str) {
                args["window"] = json!(w);
            }
            execute_here(host, "file.open", &args)?;
            Ok(json!({ "window": host.engine().state.active() }))
        })(),
        "save" => (|| {
            in_window(host, window_param(p)?, |h| {
                match p.get("path").and_then(Value::as_str) {
                    Some(path) => execute_here(h, "file.saveAs", &json!({ "path": path }))?,
                    None => execute_here(h, "file.save", &json!({}))?,
                };
                let e = &h.engine().state.editor;
                if e.doc.is_dirty() {
                    return Err(RpcError::new(code::FAILED, "the document was not saved"));
                }
                Ok(json!({ "path": e.path }))
            })
        })(),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Headless {
        engine: Engine,
        applied: Vec<String>,
    }

    impl Host for Headless {
        fn engine(&mut self) -> &mut Engine {
            &mut self.engine
        }
        fn apply(&mut self, effects: Vec<Effect>) -> Result<(), RpcError> {
            self.applied.extend(
                effects
                    .iter()
                    .map(|e| format!("{e:?}").chars().take(20).collect()),
            );
            Ok(())
        }
    }

    fn host() -> Headless {
        Headless {
            engine: Engine::new(crate::Preferences::default()),
            applied: Vec::new(),
        }
    }

    #[test]
    fn messages_round_trip() {
        let r = parse_request(
            r#"{"jsonrpc":"2.0","id":7,"method":"execute","params":{"command":"x"}}"#,
        )
        .unwrap();
        assert_eq!(r.id, json!(7));
        assert_eq!(r.method, "execute");
        assert_eq!(parse_request("{").unwrap_err().1.code, code::PARSE);
        assert_eq!(
            parse_request(r#"{"id":1}"#).unwrap_err().1.code,
            code::INVALID_REQUEST
        );
        assert_eq!(
            parse_request(r#"{"id":1,"method":"x","params":[1]}"#)
                .unwrap_err()
                .1
                .code,
            code::INVALID_PARAMS
        );
        let ok = response(&json!(1), &Ok(json!({"a": 1})));
        assert_eq!(parse_response(&ok), Ok(json!({"a": 1})));
        let err = response(&json!(1), &Err(RpcError::new(code::NO_WINDOW, "no")));
        assert_eq!(parse_response(&err).unwrap_err().code, code::NO_WINDOW);
        assert!(!ok.contains('\n'));
    }

    #[test]
    fn tokens_and_base64() {
        let a = new_token().unwrap();
        assert_eq!(a.len(), 64);
        assert_ne!(a, new_token().unwrap());
        assert!(token_eq(&a, &a.clone()));
        assert!(!token_eq(&a, &a[..63]));
        assert!(!token_eq(&a, &format!("{}0", &a[..63])) || a.ends_with('0'));
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn connection_files() {
        let d = tempfile::tempdir().unwrap();
        let dir = control_dir(d.path());
        let info = ConnectionInfo::new(4242).unwrap();
        let p = info.write(&dir).unwrap();
        let list = ConnectionInfo::list(&dir);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].1, info);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&p).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        // Rewriting (same pid) replaces the file.
        ConnectionInfo::new(1).unwrap().write(&dir).unwrap();
        assert_eq!(ConnectionInfo::list(&dir)[0].1.port, 1);
        // A file left by another process that no longer listens is removed.
        let mut stale = ConnectionInfo::new(1).unwrap();
        stale.pid = u32::MAX;
        let sp = stale.write(&dir).unwrap();
        let live = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let mut other = ConnectionInfo::new(live.local_addr().unwrap().port()).unwrap();
        other.pid = u32::MAX - 1;
        let op = other.write(&dir).unwrap();
        ConnectionInfo::prune(&dir);
        assert!(!sp.exists() && op.exists());
    }

    #[test]
    fn execute_batch_and_inspect() {
        let mut h = host();
        let r = handle(
            &mut h,
            "execute",
            &json!({ "command": "edit.insertText", "params": { "text": "hello" } }),
        );
        assert!(r.unwrap().is_ok(), "insertText");
        let v = handle(
            &mut h,
            "inspect_document",
            &json!({ "include": ["text", "selection", "stats", "blocks"] }),
        )
        .unwrap()
        .unwrap();
        assert_eq!(v["text"], "hello");
        assert_eq!(v["dirty"], true);
        assert_eq!(v["selection"]["head"], 5);
        assert_eq!(v["stats"]["words"], 1);
        assert_eq!(v["blocks"][0]["kind"], "paragraph");
        assert!(v.get("outline").is_none());
        let b = handle(
            &mut h,
            "batch",
            &json!({ "calls": [
                { "command": "edit.selectAll" },
                { "command": "no.such" },
                { "command": "format.bold" },
            ]}),
        )
        .unwrap()
        .unwrap();
        assert_eq!(b.as_array().unwrap().len(), 2, "stops at the error");
        assert_eq!(b[1]["error"]["code"], code::UNKNOWN_COMMAND);
        let e = handle(&mut h, "execute", &json!({ "command": "no.such" })).unwrap();
        assert_eq!(e.unwrap_err().code, code::UNKNOWN_COMMAND);
        assert_eq!(
            handle(&mut h, "execute", &json!({}))
                .unwrap()
                .unwrap_err()
                .code,
            code::INVALID_PARAMS
        );
        assert_eq!(
            handle(&mut h, "inspect_document", &json!({ "window": 999 }))
                .unwrap()
                .unwrap_err()
                .code,
            code::NO_WINDOW
        );
        assert!(handle(&mut h, "screenshot", &json!({})).is_none());
        let cmds = handle(&mut h, "commands", &json!({})).unwrap().unwrap();
        assert!(
            cmds.as_array()
                .unwrap()
                .iter()
                .any(|c| c["id"] == "format.bold" && c["title"] == "Bold")
        );
    }

    #[test]
    fn dialogs_become_errors_and_files_save() {
        let mut h = host();
        // Untitled: "save" would ask for a name.
        let e = handle(&mut h, "save", &json!({})).unwrap().unwrap_err();
        assert_eq!(e.code, code::FAILED);
        assert!(e.message.contains("user interaction"), "{e}");
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join("a.md");
        std::fs::write(&f, "# A\n").unwrap();
        let w = handle(&mut h, "open", &json!({ "path": f }))
            .unwrap()
            .unwrap();
        assert!(w["window"].is_u64());
        handle(
            &mut h,
            "execute",
            &json!({ "command": "edit.insertText", "params": { "text": "x" } }),
        )
        .unwrap()
        .unwrap();
        let out = d.path().join("b.md");
        let s = handle(&mut h, "save", &json!({ "path": out }))
            .unwrap()
            .unwrap();
        assert!(s["path"].as_str().unwrap().ends_with("b.md"));
        assert!(std::fs::read_to_string(&out).unwrap().contains('x'));
        let hello = handle(&mut h, "hello", &json!({})).unwrap().unwrap();
        assert_eq!(hello["version"], VERSION);
        assert!(
            hello["windows"]
                .as_array()
                .unwrap()
                .iter()
                .any(|w| w["active"] == true)
        );
    }
}
