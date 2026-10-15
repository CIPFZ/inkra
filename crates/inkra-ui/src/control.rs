//! The control channel listener (CM-003, `docs/design/control-protocol.md`).
//!
//! A listener thread accepts loopback connections; one thread per connection
//! reads newline-delimited JSON-RPC, checks the `hello` token and hands each
//! request to the UI thread through a channel. The UI thread runs them at the
//! start of a frame ([`crate::Shell::poll_control`]): engine-level methods via
//! [`inkra_engine::control::handle`], `screenshot`, `click` and `key` here.
//! Nothing is woken while no request arrives (idle CPU stays 0 %).

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};

use inkra_engine::control::{
    self as proto, ConnectionInfo, MAX_CONNECTIONS, MAX_LINE, Request, RpcError, TIMEOUT, code,
};
use serde_json::{Value, json};

/// A reply channel for one request.
pub type Reply = Sender<Result<Value, RpcError>>;

/// A request waiting for the UI thread.
pub struct Job {
    /// The request.
    pub req: Request,
    /// Where the result goes.
    pub reply: Reply,
}

/// A running control channel.
pub struct Server {
    rx: Receiver<Job>,
    port: u16,
    file: PathBuf,
    clients: Arc<AtomicUsize>,
}

impl Server {
    /// Listens on `127.0.0.1:port` (and `[::1]` on the same port when
    /// available; `0` lets the system pick) and writes the connection file
    /// into `dir`. `wake` is called when a request is queued.
    pub fn start(
        port: u16,
        dir: &std::path::Path,
        wake: impl Fn() + Send + Sync + 'static,
    ) -> Result<Self, String> {
        let v4 = TcpListener::bind((Ipv4Addr::LOCALHOST, port)).map_err(|e| e.to_string())?;
        let port = v4.local_addr().map_err(|e| e.to_string())?.port();
        let v6 = TcpListener::bind((Ipv6Addr::LOCALHOST, port)).ok();
        // Files of instances that were killed (no clean exit) are removed.
        ConnectionInfo::prune(dir);
        let info = ConnectionInfo::new(port)?;
        let file = info.write(dir).map_err(|e| e.to_string())?;
        let token = Arc::new(info.token);
        let (tx, rx) = channel();
        let clients = Arc::new(AtomicUsize::new(0));
        let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(wake);
        for l in std::iter::once(v4).chain(v6) {
            let (tx, token, clients, wake) =
                (tx.clone(), token.clone(), clients.clone(), wake.clone());
            std::thread::Builder::new()
                .name("inkra-control".into())
                .spawn(move || accept(l, tx, token, clients, wake))
                .map_err(|e| e.to_string())?;
        }
        log::info!("control: listening on loopback port {port}");
        Ok(Self {
            rx,
            port,
            file,
            clients,
        })
    }

    /// The listening port.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Connected clients.
    pub fn clients(&self) -> usize {
        self.clients.load(Ordering::SeqCst)
    }

    /// Requests that arrived since the last call.
    pub fn take(&self) -> Vec<Job> {
        self.rx.try_iter().collect()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.file);
    }
}

fn accept(
    l: TcpListener,
    tx: Sender<Job>,
    token: Arc<String>,
    clients: Arc<AtomicUsize>,
    wake: Arc<dyn Fn() + Send + Sync>,
) {
    for stream in l.incoming().flatten() {
        let loopback = stream.peer_addr().is_ok_and(|a| a.ip().is_loopback());
        if !loopback {
            continue;
        }
        if clients.fetch_add(1, Ordering::SeqCst) >= MAX_CONNECTIONS {
            clients.fetch_sub(1, Ordering::SeqCst);
            let mut s = stream;
            let _ = writeln!(
                s,
                "{}",
                proto::response(
                    &Value::Null,
                    &Err(RpcError::new(code::FAILED, "too many connections"))
                )
            );
            continue;
        }
        let (tx, token, count, wake) = (tx.clone(), token.clone(), clients.clone(), wake.clone());
        let spawned = std::thread::Builder::new()
            .name("inkra-control-conn".into())
            .spawn(move || {
                let peer = stream.peer_addr().ok();
                serve(stream, &tx, &token, wake.as_ref());
                count.fetch_sub(1, Ordering::SeqCst);
                log::info!(
                    "control: client {} disconnected",
                    peer.map_or_else(String::new, |p: SocketAddr| p.to_string())
                );
            });
        if spawned.is_err() {
            clients.fetch_sub(1, Ordering::SeqCst);
        }
    }
}

/// Reads one line of at most [`MAX_LINE`] bytes; `None` at the end.
fn read_line(r: &mut impl BufRead) -> Option<Result<String, RpcError>> {
    let mut buf = Vec::new();
    match r
        .by_ref()
        .take(MAX_LINE as u64 + 1)
        .read_until(b'\n', &mut buf)
    {
        Ok(0) | Err(_) => None,
        Ok(_) if buf.len() > MAX_LINE => Some(Err(RpcError::new(
            code::INVALID_REQUEST,
            "request longer than 16 MiB",
        ))),
        Ok(_) => Some(String::from_utf8(buf).map_err(|_| RpcError::new(code::PARSE, "not UTF-8"))),
    }
}

fn serve(stream: TcpStream, tx: &Sender<Job>, token: &str, wake: &(dyn Fn() + Send + Sync)) {
    let Ok(mut out) = stream.try_clone() else {
        return;
    };
    let mut input = BufReader::new(stream);
    let mut authed = false;
    while let Some(line) = read_line(&mut input) {
        let (id, result) =
            match line.and_then(|l| proto::parse_request(l.trim()).map_err(|(_, e)| e)) {
                Err(e) => (Value::Null, Err(e)),
                Ok(req) => {
                    let id = req.id.clone();
                    if !authed {
                        let ok = req.method == "hello"
                            && req
                                .params
                                .get("token")
                                .and_then(Value::as_str)
                                .is_some_and(|t| proto::token_eq(t, token));
                        if !ok {
                            let _ = writeln!(
                                out,
                                "{}",
                                proto::response(
                                    &id,
                                    &Err(RpcError::new(
                                        code::UNAUTHENTICATED,
                                        "send `hello` with the token first"
                                    ))
                                )
                            );
                            return;
                        }
                        authed = true;
                        log::info!("control: client authenticated");
                    }
                    (id, run_on_ui(req, tx, wake))
                }
            };
        if writeln!(out, "{}", proto::response(&id, &result)).is_err() {
            return;
        }
    }
}

fn run_on_ui(
    req: Request,
    tx: &Sender<Job>,
    wake: &(dyn Fn() + Send + Sync),
) -> Result<Value, RpcError> {
    let (reply, rx) = channel();
    let method = req.method.clone();
    if tx.send(Job { req, reply }).is_err() {
        return Err(RpcError::new(code::FAILED, "the application is closing"));
    }
    wake();
    let t = std::time::Instant::now();
    let r = rx.recv_timeout(TIMEOUT).unwrap_or_else(|_| {
        Err(RpcError::new(
            code::TIMEOUT,
            "no answer from the UI within 30 s",
        ))
    });
    // NF-008: method, time and error code only, never parameters or content.
    log::info!(
        "control: {method} in {:.1} ms{}",
        t.elapsed().as_secs_f64() * 1e3,
        r.as_ref()
            .err()
            .map_or_else(String::new, |e| format!(" -> error {}", e.code))
    );
    r
}

/// Parses `click` parameters into egui events at the pointer position.
pub fn click_events(p: &Value) -> Result<(Vec<egui::Event>, String), RpcError> {
    let num = |k: &str| {
        p.get(k)
            .and_then(Value::as_f64)
            .ok_or_else(|| RpcError::params(format!("`{k}` (number) is required")))
    };
    let pos = egui::pos2(num("x")? as f32, num("y")? as f32);
    let button = match p.get("button").and_then(Value::as_str).unwrap_or("primary") {
        "primary" => egui::PointerButton::Primary,
        "secondary" => egui::PointerButton::Secondary,
        "middle" => egui::PointerButton::Middle,
        other => return Err(RpcError::params(format!("unknown button `{other}`"))),
    };
    let count = p
        .get("count")
        .and_then(Value::as_u64)
        .unwrap_or(1)
        .clamp(1, 3);
    let modifiers = modifiers_of(p.get("modifiers"))?;
    let mut ev = vec![egui::Event::PointerMoved(pos)];
    for _ in 0..count {
        for pressed in [true, false] {
            ev.push(egui::Event::PointerButton {
                pos,
                button,
                pressed,
                modifiers,
            });
        }
    }
    let what = format!(
        "click ({}, {}, {button:?}, {count}, {modifiers:?})",
        pos.x, pos.y
    );
    Ok((ev, what))
}

fn modifiers_of(v: Option<&Value>) -> Result<egui::Modifiers, RpcError> {
    let mut m = egui::Modifiers::default();
    for name in v.and_then(Value::as_array).into_iter().flatten() {
        match name
            .as_str()
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str()
        {
            "ctrl" | "control" => {
                m.ctrl = true;
                if !cfg!(target_os = "macos") {
                    m.command = true;
                }
            }
            "cmd" | "command" | "meta" => {
                m.command = true;
                if cfg!(target_os = "macos") {
                    m.mac_cmd = true;
                } else {
                    m.ctrl = true;
                }
            }
            "shift" => m.shift = true,
            "alt" | "option" => m.alt = true,
            other => return Err(RpcError::params(format!("unknown modifier `{other}`"))),
        }
    }
    Ok(m)
}

/// Parses `key` parameters: `key` in `keybindings.json` notation (`Ctrl+Shift+K`,
/// `Enter`) and/or `text` to type.
pub fn key_events(p: &Value) -> Result<(Vec<egui::Event>, String), RpcError> {
    let mut ev = Vec::new();
    let mut what = Vec::new();
    if let Some(k) = p.get("key").and_then(Value::as_str) {
        let chord: inkra_engine::keymap::KeyChord = k
            .parse()
            .map_err(|e| RpcError::params(format!("bad key `{k}`: {e}")))?;
        let key = egui::Key::ALL
            .iter()
            .copied()
            .find(|e| crate::keys::map_key(*e) == Some(chord.key))
            .ok_or_else(|| RpcError::params(format!("no key for `{k}`")))?;
        let mac = cfg!(target_os = "macos");
        let modifiers = egui::Modifiers {
            alt: chord.alt,
            ctrl: if mac { chord.mac_ctrl } else { chord.ctrl },
            shift: chord.shift,
            mac_cmd: mac && chord.ctrl,
            command: chord.ctrl,
        };
        for pressed in [true, false] {
            ev.push(egui::Event::Key {
                key,
                physical_key: Some(key),
                pressed,
                repeat: false,
                modifiers,
            });
        }
        what.push(format!("key {chord}"));
    }
    if let Some(t) = p.get("text").and_then(Value::as_str) {
        ev.push(egui::Event::Text(t.to_owned()));
        // NF-008: the length only, typed text can be document content.
        what.push(format!("text of {} characters", t.chars().count()));
    }
    if ev.is_empty() {
        return Err(RpcError::params("`key` or `text` is required"));
    }
    Ok((ev, what.join(", ")))
}

/// A screenshot as `{ png, width, height }` (bytes in the reply only: nothing
/// is written to disk).
pub fn screenshot_result(image: &egui::ColorImage) -> Result<Value, RpcError> {
    let [w, h] = image.size;
    let rgba: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_array()).collect();
    let img = image::RgbaImage::from_raw(w as u32, h as u32, rgba)
        .ok_or_else(|| RpcError::new(code::FAILED, "bad screenshot buffer"))?;
    let mut png = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .map_err(|e| RpcError::new(code::FAILED, e.to_string()))?;
    Ok(json!({ "png": proto::base64(&png), "width": w, "height": h }))
}

struct ShellHost<'a> {
    shell: &'a mut crate::Shell,
    ctx: &'a egui::Context,
}

impl proto::Host for ShellHost<'_> {
    fn engine(&mut self) -> &mut inkra_engine::Engine {
        &mut self.shell.engine
    }

    fn apply(&mut self, effects: Vec<inkra_engine::Effect>) -> Result<(), RpcError> {
        self.shell.apply_effects(self.ctx, Some(effects));
        Ok(())
    }
}

impl crate::Shell {
    /// Opens the control channel (`inkra --control <port>`); returns the port.
    /// The connection file goes to `<data_dir>/control/`.
    pub fn start_control(&mut self, ctx: &egui::Context, port: u16) -> Result<u16, String> {
        let dir = self
            .engine
            .state
            .paths
            .as_ref()
            .map(|p| proto::control_dir(&p.data))
            .ok_or("no data folder for the connection file")?;
        let c = ctx.clone();
        let server = Server::start(port, &dir, move || c.request_repaint())?;
        let port = server.port();
        self.control = Some(server);
        Ok(port)
    }

    /// Moves input injected by `click` / `key` into this frame's input (call
    /// from `eframe::App::raw_input_hook`). A frame with injected input counts
    /// as focused: egui ignores keys in an unfocused window, and automation
    /// usually runs while the user works elsewhere.
    pub fn control_raw_input(&mut self, raw: &mut egui::RawInput) {
        for (ev, reply) in std::mem::take(&mut self.control_input) {
            raw.events.extend(ev);
            raw.focused = true;
            self.control_injected.push(reply);
        }
    }

    /// Runs queued control requests (start of every frame).
    pub(crate) fn poll_control(&mut self, ctx: &egui::Context) {
        // Injected input was processed by the previous frame.
        for r in std::mem::take(&mut self.control_done) {
            let _ = r.send(Ok(json!({})));
        }
        self.control_done = std::mem::take(&mut self.control_injected);
        if !self.control_done.is_empty() || !self.control_input.is_empty() {
            ctx.request_repaint();
        }
        if !self.control_shots.is_empty() {
            let shots: Vec<(u64, std::sync::Arc<egui::ColorImage>)> = ctx.input(|i| {
                i.events
                    .iter()
                    .filter_map(|e| match e {
                        egui::Event::Screenshot {
                            image, user_data, ..
                        } => user_data
                            .data
                            .as_ref()
                            .and_then(|d| d.downcast_ref::<u64>())
                            .map(|n| (*n, image.clone())),
                        _ => None,
                    })
                    .collect()
            });
            for (n, image) in shots {
                if let Some(k) = self.control_shots.iter().position(|(m, _)| *m == n) {
                    let (_, reply) = self.control_shots.swap_remove(k);
                    let _ = reply.send(screenshot_result(&image));
                }
            }
        }
        let Some(jobs) = self.control.as_ref().map(Server::take) else {
            return;
        };
        for Job { req, reply } in jobs {
            match req.method.as_str() {
                "screenshot" => {
                    self.control_seq += 1;
                    let n = self.control_seq;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::new(
                        n,
                    )));
                    self.control_shots.push((n, reply));
                    ctx.request_repaint();
                }
                "click" | "key" => {
                    let parsed = if req.method == "click" {
                        click_events(&req.params)
                    } else {
                        key_events(&req.params)
                    };
                    match parsed {
                        Ok((ev, what)) => {
                            // Design 8 (b): every injection is logged.
                            log::info!("control: injected {what}");
                            self.control_input.push((ev, reply));
                            ctx.request_repaint();
                        }
                        Err(e) => {
                            let _ = reply.send(Err(e));
                        }
                    }
                }
                m => {
                    let mut host = ShellHost { shell: self, ctx };
                    let r = proto::handle(&mut host, m, &req.params).unwrap_or_else(|| {
                        Err(RpcError::new(
                            code::METHOD_NOT_FOUND,
                            format!("no method `{m}`"),
                        ))
                    });
                    let _ = reply.send(r);
                    ctx.request_repaint();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn click_and_key_parameters() {
        let (ev, what) = click_events(&json!({ "x": 10, "y": 20, "count": 2 })).unwrap();
        assert_eq!(ev.len(), 1 + 4);
        assert!(what.starts_with("click (10, 20"));
        assert!(click_events(&json!({ "x": 1 })).is_err());
        assert!(click_events(&json!({ "x": 1, "y": 1, "button": "fourth" })).is_err());
        let (ev, what) = key_events(&json!({ "key": "Ctrl+Shift+K", "text": "secret" })).unwrap();
        assert_eq!(ev.len(), 3);
        assert!(
            matches!(ev[0], egui::Event::Key { key: egui::Key::K, pressed: true, modifiers, .. } if modifiers.shift && modifiers.command)
        );
        assert!(
            !what.contains("secret") && what.contains("6 characters"),
            "{what}"
        );
        let (ev, _) = key_events(&json!({ "key": "Enter" })).unwrap();
        assert!(matches!(
            ev[0],
            egui::Event::Key {
                key: egui::Key::Enter,
                ..
            }
        ));
        assert!(key_events(&json!({})).is_err());
        assert!(key_events(&json!({ "key": "Hyper+Q" })).is_err());
    }

    #[test]
    fn screenshots_are_png() {
        let img = egui::ColorImage::new([3, 2], vec![egui::Color32::RED; 6]);
        let v = screenshot_result(&img).unwrap();
        assert_eq!(v["width"], 3);
        assert!(
            v["png"].as_str().unwrap().starts_with("iVBOR"),
            "PNG signature"
        );
    }

    #[test]
    fn listener_requires_the_token() {
        let d = tempfile::tempdir().unwrap();
        let s = Server::start(0, d.path(), || {}).unwrap();
        let info = &ConnectionInfo::list(d.path())[0].1;
        assert_eq!(info.port, s.port());
        // Wrong token: an error and the connection closes.
        let mut c = TcpStream::connect(("127.0.0.1", s.port())).unwrap();
        writeln!(
            c,
            "{}",
            proto::request(1, "hello", &json!({ "token": "x" }))
        )
        .unwrap();
        let mut line = String::new();
        BufReader::new(c.try_clone().unwrap())
            .read_line(&mut line)
            .unwrap();
        assert_eq!(
            proto::parse_response(&line).unwrap_err().code,
            code::UNAUTHENTICATED
        );
        // Right token: the request reaches the UI side.
        let mut c = TcpStream::connect(("127.0.0.1", s.port())).unwrap();
        writeln!(
            c,
            "{}",
            proto::request(1, "hello", &json!({ "token": info.token }))
        )
        .unwrap();
        let job = (0..200)
            .find_map(|_| {
                std::thread::sleep(std::time::Duration::from_millis(10));
                s.take().pop()
            })
            .expect("queued");
        assert_eq!(job.req.method, "hello");
        job.reply.send(Ok(json!({ "ok": true }))).unwrap();
        let mut line = String::new();
        BufReader::new(c).read_line(&mut line).unwrap();
        assert_eq!(proto::parse_response(&line).unwrap(), json!({ "ok": true }));
        let file = ConnectionInfo::list(d.path())[0].0.clone();
        drop(s);
        assert!(!file.exists(), "the connection file is removed");
    }
}
