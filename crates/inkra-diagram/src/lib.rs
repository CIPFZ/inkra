//! # inkra-diagram
//!
//! **Layer L2** (see SPEC 2.2).
//!
//! Mermaid subset (SPEC 2.4, MD-100): `flowchart`/`graph`, `sequenceDiagram` and
//! `pie` are parsed and laid out here into vector scenes (labels as glyph
//! outlines). Parse errors carry a line number; the host shows them below the
//! last good rendering.

pub mod draw;
pub mod external;
pub mod flowchart;
pub mod pie;
pub mod sequence;

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

pub use draw::{Fill, Rgba, Scene};

/// A diagram error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    /// 1-based line in the diagram source (0: unknown).
    pub line: usize,
    /// Message.
    pub message: String,
}

impl Error {
    fn new(line: usize, msg: &str) -> Self {
        Self {
            line,
            message: msg.to_owned(),
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.line > 0 {
            write!(f, "line {}: {}", self.line, self.message)
        } else {
            f.write_str(&self.message)
        }
    }
}

/// Colors (follow the app theme, MD-102).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Palette {
    /// Text.
    pub fg: Rgba,
    /// Background.
    pub bg: Rgba,
    /// Node fill.
    pub node_fill: Rgba,
    /// Node outline.
    pub node_stroke: Rgba,
    /// Edges and arrows.
    pub edge: Rgba,
    /// Sequence lifelines.
    pub lifeline: Rgba,
    /// Subgraph / block background.
    pub cluster_fill: Rgba,
    /// Subgraph / block outline.
    pub cluster_stroke: Rgba,
    /// Note fill.
    pub note_fill: Rgba,
    /// Note outline.
    pub note_stroke: Rgba,
    /// Edge label background.
    pub label_bg: Rgba,
    /// Pie slice colors.
    pub pie: Vec<Rgba>,
    /// Text on pie slices.
    pub pie_text: Rgba,
}

impl Palette {
    /// Light default.
    pub fn light() -> Self {
        Self {
            fg: [36, 41, 46, 255],
            bg: [255, 255, 255, 255],
            node_fill: [236, 236, 255, 255],
            node_stroke: [147, 112, 219, 255],
            edge: [51, 51, 51, 255],
            lifeline: [153, 153, 153, 255],
            cluster_fill: [255, 255, 222, 255],
            cluster_stroke: [170, 170, 51, 255],
            note_fill: [255, 245, 173, 255],
            note_stroke: [170, 170, 51, 255],
            label_bg: [232, 232, 232, 255],
            pie: vec![
                [236, 236, 255, 255],
                [255, 255, 222, 255],
                [185, 185, 255, 255],
                [255, 214, 165, 255],
                [176, 232, 196, 255],
                [255, 180, 190, 255],
                [200, 225, 255, 255],
                [230, 200, 255, 255],
            ],
            pie_text: [36, 41, 46, 255],
        }
    }

    /// Dark default.
    pub fn dark() -> Self {
        Self {
            fg: [230, 227, 240, 255],
            bg: [30, 27, 51, 255],
            node_fill: [55, 50, 92, 255],
            node_stroke: [155, 140, 230, 255],
            edge: [210, 206, 230, 255],
            lifeline: [120, 116, 150, 255],
            cluster_fill: [45, 42, 70, 255],
            cluster_stroke: [130, 125, 170, 255],
            note_fill: [80, 72, 40, 255],
            note_stroke: [160, 150, 90, 255],
            label_bg: [50, 46, 80, 255],
            pie: vec![
                [110, 100, 200, 255],
                [190, 150, 70, 255],
                [80, 150, 200, 255],
                [190, 90, 120, 255],
                [80, 160, 110, 255],
                [150, 100, 190, 255],
                [200, 120, 80, 255],
                [100, 170, 170, 255],
            ],
            pie_text: [255, 255, 255, 255],
        }
    }
}

/// Supported diagram kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// `flowchart` / `graph`.
    Flowchart,
    /// `sequenceDiagram`.
    Sequence,
    /// `pie`.
    Pie,
}

/// The kind named by the first non-comment line, or the keyword if unsupported.
pub fn kind(src: &str) -> Result<Kind, String> {
    let first = src
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with("%%"))
        .unwrap_or("");
    let word = first.split_whitespace().next().unwrap_or("");
    match word {
        "flowchart" | "graph" | "flowchart-elk" => Ok(Kind::Flowchart),
        "sequenceDiagram" => Ok(Kind::Sequence),
        "pie" => Ok(Kind::Pie),
        w => Err(w.to_owned()),
    }
}

/// Whether an error means the diagram type is not built in (so the external
/// renderer may help).
pub fn is_unsupported(e: &Error) -> bool {
    e.message.starts_with("diagram type")
}

/// Parses, lays out and draws a diagram (uncached).
pub fn render_uncached(src: &str, pal: &Palette) -> Result<Scene, Error> {
    // Front matter (`---\nconfig…\n---`) and `%%{init}%%` directives are skipped.
    let src = strip_header(src);
    match kind(&src) {
        Ok(Kind::Flowchart) => flowchart::parse(&src).map(|g| flowchart::render(&g, pal)),
        Ok(Kind::Sequence) => sequence::parse(&src).map(|s| sequence::render(&s, pal)),
        Ok(Kind::Pie) => pie::parse(&src).map(|p| pie::render(&p, pal)),
        Err(w) if w.is_empty() => Err(Error::new(0, "empty diagram")),
        Err(w) => Err(Error::new(
            1,
            &format!("diagram type `{w}` is not supported"),
        )),
    }
}

fn strip_header(src: &str) -> String {
    let mut lines: Vec<&str> = src.lines().collect();
    if lines.first().is_some_and(|l| l.trim() == "---")
        && let Some(end) = lines.iter().skip(1).position(|l| l.trim() == "---")
    {
        lines.drain(..=end + 1);
    }
    lines.retain(|l| !l.trim_start().starts_with("%%{"));
    lines.join("\n")
}

type Cache = Mutex<HashMap<(String, Palette), Arc<Result<Scene, Error>>>>;
static CACHE: OnceLock<Cache> = OnceLock::new();

/// Renders with caching by source and palette.
pub fn render(src: &str, pal: &Palette) -> Arc<Result<Scene, Error>> {
    let cache = CACHE.get_or_init(Default::default);
    let key = (src.to_owned(), pal.clone());
    if let Ok(c) = cache.lock()
        && let Some(r) = c.get(&key)
    {
        return r.clone();
    }
    // NF-002: a layout bug becomes an error under the source block instead of
    // taking the frame down; the result is cached like any other.
    let r = Arc::new(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| render_uncached(src, pal)))
            .unwrap_or_else(|_| Err(Error::new(0, "internal error while drawing the diagram"))),
    );
    if let Ok(mut c) = cache.lock() {
        if c.len() > 512 {
            c.clear();
        }
        c.insert(key, r.clone());
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    fn corpus_blocks() -> Vec<String> {
        let text = include_str!("../../../docs/acceptance/diagram-corpus.md");
        let mut out = Vec::new();
        let mut cur: Option<String> = None;
        for l in text.lines() {
            match (&mut cur, l.trim()) {
                (None, "```mermaid") => cur = Some(String::new()),
                (Some(_), "```") => out.push(cur.take().unwrap_or_default()),
                (Some(b), _) => {
                    b.push_str(l);
                    b.push('\n');
                }
                _ => {}
            }
        }
        out
    }

    /// M5 acceptance: 30 flowchart examples plus sequence and pie render.
    #[test]
    fn corpus() {
        let blocks = corpus_blocks();
        let flow = blocks
            .iter()
            .filter(|b| matches!(kind(b), Ok(Kind::Flowchart)))
            .count();
        assert!(flow >= 30, "{flow} flowcharts");
        let pal = Palette::light();
        let bad: Vec<String> = blocks
            .iter()
            .filter_map(|b| render_uncached(b, &pal).err().map(|e| format!("{e}\n{b}")))
            .collect();
        assert!(bad.is_empty(), "{}", bad.join("\n---\n"));
    }

    #[test]
    fn renders_each_kind_and_reports_errors() {
        let pal = Palette::light();
        for src in [
            "flowchart TD\n  A[Christmas] -->|Get money| B(Go shopping)\n  B --> C{Let me think}\n  C -->|One| D[Laptop]\n  C -->|Two| E[iPhone]\n  C -->|Three| F[fa:fa-car Car]",
            "sequenceDiagram\n  Alice->>John: Hello John, how are you?\n  John-->>Alice: Great!\n  Alice-)John: See you later!",
            "pie title Pets adopted by volunteers\n  \"Dogs\" : 386\n  \"Cats\" : 85\n  \"Rats\" : 15",
        ] {
            let s = render_uncached(src, &pal).unwrap_or_else(|e| panic!("{src}: {e}"));
            assert!(s.width > 50.0 && s.height > 50.0);
            assert!(!s.fills.is_empty());
        }
        let e = render_uncached("gantt\n  title x", &pal).unwrap_err();
        assert!(e.message.contains("gantt"));
        let e = render_uncached("flowchart TD\n  A --> B\n  C -->", &pal).unwrap_err();
        assert_eq!(e.line, 3);
    }
}

#[cfg(test)]
mod robustness {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// NF-002: arbitrary input never panics.
        #[test]
        fn never_panics(body in "[a-zA-Z0-9 \\-\\->|\\[\\](){}:;&\"\\n.=xo%İß中]{0,120}", k in 0usize..3) {
            let head = ["flowchart TD\n", "sequenceDiagram\n", "pie\n"][k];
            let _ = render_uncached(&format!("{head}{body}"), &Palette::light());
        }
    }
}
