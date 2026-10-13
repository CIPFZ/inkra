//! Image loading for live rendering (ED-090, ED-091): local files, `file://` and
//! `data:` URIs are decoded on background threads (AD-5) and uploaded as
//! textures; layout uses their sizes, failures show a placeholder.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};

use inkra_layout::live::leaf::ImageSizes;

enum Entry {
    Loading,
    Ready {
        tex: egui::TextureHandle,
        size: [f32; 2],
    },
    Failed(String),
}

/// Loaded images keyed by source as written in the document.
pub struct Images {
    base: Option<PathBuf>,
    entries: HashMap<String, Entry>,
    tx: Sender<(String, Result<Decoded, String>)>,
    rx: Receiver<(String, Result<Decoded, String>)>,
    /// Column widths of tables resized by dragging, by table source start
    /// (display only, ED-084).
    pub column_widths: HashMap<usize, Vec<f32>>,
    /// Rasterized formulas by (scene key, color, pixels per point) with the
    /// frame they were last used in.
    math: HashMap<(u64, egui::Color32, u32), (egui::TextureHandle, u64)>,
    frame: u64,
    /// Load `http(s)` images (NF-004: can be turned off).
    pub remote: bool,
    root: Option<PathBuf>,
    /// Corner radius images are drawn with (theme).
    pub image_radius: f32,
}

impl Default for Images {
    fn default() -> Self {
        let (tx, rx) = channel();
        Self {
            base: None,
            entries: HashMap::new(),
            tx,
            rx,
            column_widths: HashMap::new(),
            math: HashMap::new(),
            frame: 0,
            remote: true,
            root: None,
            image_radius: 0.0,
        }
    }
}

impl ImageSizes for Images {
    fn column_widths(&self, table_start: usize) -> Option<Vec<f32>> {
        self.column_widths.get(&table_start).cloned()
    }

    fn size(&self, src: &str) -> Option<[f32; 2]> {
        match self.entries.get(src) {
            Some(Entry::Ready { size, .. }) => Some(*size),
            _ => None,
        }
    }
}

fn decode_base64(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let mut buf = 0u32;
    let mut bits = 0;
    for c in s.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' | b'\n' | b'\r' | b' ' => continue,
            _ => return None,
        } as u32;
        buf = (buf << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
        }
    }
    Some(out)
}

thread_local! {
    static ROOT: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}

/// Resolves an image source to a local path (ED-091: relative to the document's
/// directory).
pub fn resolve(base: Option<&Path>, src: &str) -> Option<PathBuf> {
    let src = src.trim();
    if src.starts_with("http://") || src.starts_with("https://") || src.starts_with("data:") {
        return None;
    }
    let path = src.strip_prefix("file://").unwrap_or(src);
    let decoded = percent_decode(path);
    let p = Path::new(&decoded);
    // ED-091: `/img/a.png` is relative to the image root when one is set
    // (`inkra-root-url` in the front matter or the global preference).
    if let Some(root) = ROOT.with(|r| r.borrow().clone())
        && (decoded.starts_with('/') || decoded.starts_with('\\'))
        && !src.starts_with("file://")
    {
        let candidate = root.join(decoded.trim_start_matches(['/', '\\']));
        if candidate.exists() || !p.exists() {
            return Some(candidate);
        }
    }
    if p.is_absolute() {
        return Some(p.to_owned());
    }
    // `/C:/...` from file:///C:/...
    if decoded.len() > 2 && decoded.as_bytes()[0] == b'/' && decoded.as_bytes()[2] == b':' {
        return Some(PathBuf::from(&decoded[1..]));
    }
    base.map(|b| b.join(p))
}

pub(crate) fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && i + 2 < b.len()
            && let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16)
        {
            out.push(v);
            i += 3;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// A decoded image and its size in points (SVGs are rasterized at twice their
/// size for sharpness).
type Decoded = (egui::ColorImage, [f32; 2]);

fn is_svg(bytes: &[u8]) -> bool {
    let head = &bytes[..bytes.len().min(512)];
    let s = String::from_utf8_lossy(head);
    let t = s.trim_start_matches('\u{feff}').trim_start();
    t.starts_with("<svg")
        || (t.starts_with("<?xml") && s.contains("<svg"))
        || t.starts_with("<!DOCTYPE svg")
}

/// System fonts for text in SVGs, loaded once.
fn svg_fonts() -> std::sync::Arc<resvg::usvg::fontdb::Database> {
    static DB: std::sync::OnceLock<std::sync::Arc<resvg::usvg::fontdb::Database>> =
        std::sync::OnceLock::new();
    DB.get_or_init(|| {
        let mut db = resvg::usvg::fontdb::Database::new();
        db.load_system_fonts();
        // Bundled faces guarantee the generic families resolve everywhere.
        for (name, data) in inkra_layout::fonts::bundled() {
            db.load_font_data(data.to_vec());
            if *name == inkra_layout::fonts::BODY {
                db.set_sans_serif_family("Inter");
            } else if *name == inkra_layout::fonts::SERIF {
                db.set_serif_family("Source Serif 4");
            } else if *name == inkra_layout::fonts::MONO {
                db.set_monospace_family("JetBrains Mono");
            }
        }
        std::sync::Arc::new(db)
    })
    .clone()
}

fn decode_svg(bytes: &[u8]) -> Result<Decoded, String> {
    let opt = resvg::usvg::Options {
        fontdb: svg_fonts(),
        ..Default::default()
    };
    let tree = resvg::usvg::Tree::from_data(bytes, &opt).map_err(|e| e.to_string())?;
    let size = tree.size();
    let (w, h) = (size.width().max(1.0), size.height().max(1.0));
    let scale = (2.0f32).min(4096.0 / w.max(h));
    let (pw, ph) = ((w * scale).ceil() as u32, (h * scale).ceil() as u32);
    let mut pm = resvg::tiny_skia::Pixmap::new(pw, ph).ok_or("SVG too large")?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pm.as_mut(),
    );
    let px: Vec<egui::Color32> = pm
        .data()
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| egui::Color32::from_rgba_premultiplied(c[0], c[1], c[2], c[3]))
        .collect();
    Ok((
        egui::ColorImage::new([pw as usize, ph as usize], px),
        [w, h],
    ))
}

fn decode(bytes: &[u8]) -> Result<Decoded, String> {
    if is_svg(bytes) {
        return decode_svg(bytes);
    }
    let img = image::load_from_memory(bytes).map_err(|e| e.to_string())?;
    let rgba = img.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    Ok((
        egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw()),
        [size[0] as f32, size[1] as f32],
    ))
}

impl Images {
    /// Sets the directory relative image paths resolve against.
    pub fn set_base(&mut self, base: Option<PathBuf>) {
        if self.base != base {
            self.base = base;
            self.entries.clear();
        }
    }

    /// Sets the folder `/…` image paths resolve against (ED-091).
    pub fn set_root(&mut self, root: Option<PathBuf>) {
        if self.root != root {
            self.root = root;
            self.entries.clear();
        }
    }

    /// Requests `src`; returns its texture if ready.
    pub fn get(&mut self, ctx: &egui::Context, src: &str) -> Option<egui::TextureId> {
        match self.entries.get(src) {
            Some(Entry::Ready { tex, .. }) => return Some(tex.id()),
            Some(Entry::Loading | Entry::Failed(_)) => return None,
            None => {}
        }
        self.entries.insert(src.to_owned(), Entry::Loading);
        let tx = self.tx.clone();
        let key = src.to_owned();
        let base = self.base.clone();
        let root = self.root.clone();
        let remote = self.remote;
        let ctx = ctx.clone();
        let spawned = std::thread::Builder::new()
            .name("inkra-image".into())
            .spawn(move || {
                ROOT.with(|r| *r.borrow_mut() = root);
                let result = if let Some(rest) = key.strip_prefix("data:") {
                    match rest.split_once(";base64,") {
                        Some((_, data)) => decode_base64(data)
                            .ok_or_else(|| "bad base64".to_owned())
                            .and_then(|b| decode(&b)),
                        None => Err("unsupported data URI".to_owned()),
                    }
                } else if key.starts_with("http://") || key.starts_with("https://") {
                    if remote {
                        inkra_workspace::net::get(&key).and_then(|b| decode(&b))
                    } else {
                        Err("network images are turned off".to_owned())
                    }
                } else {
                    match resolve(base.as_deref(), &key) {
                        Some(p) => std::fs::read(&p)
                            .map_err(|e| format!("{}: {e}", p.display()))
                            .and_then(|b| decode(&b)),
                        None => Err("unsupported image source".to_owned()),
                    }
                };
                let _ = tx.send((key, result));
                ctx.request_repaint();
            });
        if let Err(e) = spawned {
            log::warn!("cannot spawn image loader: {e}");
        }
        None
    }

    /// Uploads finished decodes; returns whether anything changed.
    pub fn poll(&mut self, ctx: &egui::Context) -> bool {
        let mut changed = false;
        while let Ok((key, result)) = self.rx.try_recv() {
            let entry = match result {
                Ok((img, size)) => {
                    let tex =
                        ctx.load_texture(format!("img:{key}"), img, egui::TextureOptions::LINEAR);
                    Entry::Ready { tex, size }
                }
                Err(e) => {
                    log::info!("image {key}: {e}");
                    Entry::Failed(e)
                }
            };
            self.entries.insert(key, entry);
            changed = true;
        }
        changed
    }

    /// Rasterizes a formula for this frame if needed (SPEC 2.4 caching).
    pub fn ensure_math(&mut self, ctx: &egui::Context, m: &inkra_layout::MathScene) {
        let ppp = ctx.pixels_per_point();
        let key = (m.key, m.color, ppp.to_bits());
        let frame = self.frame;
        if let Some(e) = self.math.get_mut(&key) {
            e.1 = frame;
            return;
        }
        let img = inkra_render::rasterize(&m.scene, m.color, ppp);
        let tex = ctx.load_texture(
            format!("math:{:x}", m.key),
            img,
            egui::TextureOptions::LINEAR,
        );
        self.math.insert(key, (tex, frame));
    }

    /// Starts a frame: formulas unused for a while release their textures.
    pub fn begin_frame(&mut self) {
        self.frame += 1;
        if self.math.len() > 256 {
            let f = self.frame;
            self.math.retain(|_, (_, used)| f - *used < 120);
        }
    }

    /// The texture of a loaded image.
    pub fn texture(&self, src: &str) -> Option<egui::TextureId> {
        match self.entries.get(src) {
            Some(Entry::Ready { tex, .. }) => Some(tex.id()),
            _ => None,
        }
    }

    /// Error of a failed image.
    pub fn error(&self, src: &str) -> Option<&str> {
        match self.entries.get(src) {
            Some(Entry::Failed(e)) => Some(e),
            _ => None,
        }
    }

    /// Forgets `src` so it is loaded again.
    pub fn reload(&mut self, src: &str) {
        self.entries.remove(src);
    }
}

impl inkra_render::Textures for Images {
    fn image(&self, src: &str) -> Option<egui::TextureId> {
        self.texture(src)
    }

    fn math(&self, m: &inkra_layout::MathScene) -> Option<egui::TextureId> {
        // Any pixels-per-point variant will do while the right one is pending.
        self.math
            .iter()
            .find(|(k, _)| k.0 == m.key && k.1 == m.color)
            .map(|(_, (t, _))| t.id())
    }

    fn image_radius(&self) -> f32 {
        self.image_radius
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_paths() {
        let base = Path::new("/docs");
        assert_eq!(
            resolve(Some(base), "a b.png"),
            Some(PathBuf::from("/docs/a b.png"))
        );
        assert_eq!(
            resolve(Some(base), "img/a%20b.png"),
            Some(PathBuf::from("/docs/img/a b.png"))
        );
        assert_eq!(resolve(Some(base), "https://x/y.png"), None);
        assert_eq!(decode_base64("aGk="), Some(b"hi".to_vec()));
    }
}
