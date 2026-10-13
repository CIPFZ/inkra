//! Document analysis for `inkra-cli inspect` and `inkra-cli lint` (CM-002):
//! headings, links, images and counts; structural problems.

use std::path::Path;

use inkra_doc::Doc;
use inkra_markdown::ast::{Block, BlockKind, Inline, InlineKind};
use serde_json::{Value, json};

/// A link or image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    /// Destination as written (unescaped).
    pub dest: String,
    /// 1-based line.
    pub line: usize,
}

/// What `inspect` reports.
#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    /// `(level, text, anchor, line)`.
    pub headings: Vec<(u8, String, String, usize)>,
    /// Links.
    pub links: Vec<Target>,
    /// Images.
    pub images: Vec<Target>,
    /// Footnote labels defined.
    pub footnotes: Vec<String>,
    /// Word, character and line counts.
    pub counts: inkra_text::count::Counts,
    /// Top-level block kinds, in order.
    pub blocks: Vec<&'static str>,
}

fn walk_inlines(list: &[Inline], f: &mut dyn FnMut(&Inline)) {
    for i in list {
        i.walk(&mut |n| f(n));
    }
}

/// Analyzes a document.
pub fn inspect(doc: &Doc) -> Report {
    let text = doc.text();
    let blocks = doc.ast().to_blocks();
    let line_of = |pos: usize| text.byte_to_line(pos.min(text.len())) + 1;
    let headings = inkra_markdown::walk::headings(text, &blocks)
        .into_iter()
        .map(|h| (h.level, h.text, h.anchor, line_of(h.span.start)))
        .collect();
    let mut links = Vec::new();
    let mut images = Vec::new();
    let mut footnotes = Vec::new();
    for b in &blocks {
        b.walk(&mut |blk: &Block| {
            if let BlockKind::FootnoteDef { label, .. } = &blk.kind {
                footnotes.push(label.clone());
            }
            walk_inlines(&blk.inlines, &mut |n| match &n.kind {
                InlineKind::Link { dest, .. } => links.push(Target {
                    dest: dest.clone(),
                    line: line_of(n.span.start),
                }),
                InlineKind::Image { dest, .. } => images.push(Target {
                    dest: dest.clone(),
                    line: line_of(n.span.start),
                }),
                _ => {}
            });
        });
    }
    Report {
        headings,
        links,
        images,
        footnotes,
        counts: inkra_text::count::Counts::of(&text.to_string()),
        blocks: blocks.iter().map(|b| b.kind.name()).collect(),
    }
}

impl Report {
    /// The report as JSON.
    pub fn to_json(&self) -> Value {
        json!({
            "headings": self.headings.iter().map(|(l, t, a, line)| json!({ "level": l, "text": t, "anchor": a, "line": line })).collect::<Vec<_>>(),
            "links": self.links.iter().map(|t| json!({ "dest": t.dest, "line": t.line })).collect::<Vec<_>>(),
            "images": self.images.iter().map(|t| json!({ "src": t.dest, "line": t.line })).collect::<Vec<_>>(),
            "footnotes": self.footnotes,
            "counts": {
                "words": self.counts.words,
                "characters": self.counts.chars,
                "charactersNoSpaces": self.counts.chars_no_space,
                "lines": self.counts.lines,
                "paragraphs": self.counts.paragraphs,
            },
            "blocks": self.blocks,
        })
    }
}

/// A structural problem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    /// 1-based line.
    pub line: usize,
    /// Stable kind (`broken-anchor`, `missing-image`, `missing-file`,
    /// `unclosed-fence`, `undefined-footnote`, `duplicate-heading`).
    pub kind: &'static str,
    /// Human-readable message.
    pub message: String,
}

fn is_remote(d: &str) -> bool {
    d.contains("://") || d.starts_with("mailto:") || d.starts_with("data:")
}

/// Finds broken links, missing images, unclosed fences, undefined footnotes and
/// duplicate heading anchors. `doc_path` resolves relative files.
pub fn lint(doc: &Doc, doc_path: Option<&Path>) -> Vec<Problem> {
    let r = inspect(doc);
    let text = doc.text();
    let line_of = |pos: usize| text.byte_to_line(pos.min(text.len())) + 1;
    let dir = doc_path.and_then(Path::parent).unwrap_or(Path::new("."));
    let resolve = |d: &str| {
        let d = d.split('#').next().unwrap_or("");
        let p = Path::new(d);
        if p.is_absolute() {
            p.to_owned()
        } else {
            dir.join(p)
        }
    };
    let mut out = Vec::new();
    let anchors: Vec<&str> = r.headings.iter().map(|h| h.2.as_str()).collect();
    for l in &r.links {
        if let Some(a) = l.dest.strip_prefix('#') {
            if !anchors.contains(&a) && !a.starts_with("fn") {
                out.push(Problem {
                    line: l.line,
                    kind: "broken-anchor",
                    message: format!("no heading with anchor `#{a}`"),
                });
            }
        } else if !is_remote(&l.dest) && !l.dest.is_empty() && !resolve(&l.dest).exists() {
            out.push(Problem {
                line: l.line,
                kind: "missing-file",
                message: format!("linked file `{}` does not exist", l.dest),
            });
        }
    }
    for i in &r.images {
        if i.dest.is_empty() {
            out.push(Problem {
                line: i.line,
                kind: "missing-image",
                message: "image without a source".into(),
            });
        } else if !is_remote(&i.dest) && !resolve(&i.dest).is_file() {
            out.push(Problem {
                line: i.line,
                kind: "missing-image",
                message: format!("image `{}` does not exist", i.dest),
            });
        }
    }
    let blocks = doc.ast().to_blocks();
    for b in &blocks {
        b.walk(&mut |blk: &Block| {
            if let BlockKind::CodeBlock { fence: Some(f) } = &blk.kind
                && f.close.is_none()
            {
                out.push(Problem {
                    line: line_of(blk.span.start),
                    kind: "unclosed-fence",
                    message: "code fence is never closed".into(),
                });
            }
            // An undefined `[^label]` stays literal text.
            walk_inlines(&blk.inlines, &mut |n| {
                if !matches!(n.kind, InlineKind::Text) {
                    return;
                }
                let t = text.slice(n.span.clone());
                let mut rest: &str = &t;
                let mut base = n.span.start;
                while let Some(i) = rest.find("[^") {
                    let after = &rest[i + 2..];
                    if let Some(j) = after.find(']') {
                        let label = &after[..j];
                        if !label.is_empty()
                            && !label.contains(char::is_whitespace)
                            && !r.footnotes.iter().any(|f| f == label)
                        {
                            out.push(Problem {
                                line: line_of(base + i),
                                kind: "undefined-footnote",
                                message: format!("footnote `[^{label}]` has no definition"),
                            });
                        }
                    }
                    base += i + 2;
                    rest = after;
                }
            });
        });
    }
    // Repeated heading texts get suffixed anchors (`#a-1`) that links rarely use.
    let mut seen = std::collections::HashSet::new();
    for (_, t, a, line) in &r.headings {
        if !seen.insert(t.to_lowercase()) {
            out.push(Problem {
                line: *line,
                kind: "duplicate-heading",
                message: format!("heading `{t}` appears more than once (this one is `#{a}`)"),
            });
        }
    }
    out.sort_by_key(|p| p.line);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inspect_and_lint() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("there.png"), b"x").unwrap();
        let src = "# A\n\n[ok](#a) [bad](#nope) [web](https://x.org) [file](missing.md)\n\n![](there.png) ![x](gone.png)\n\nNote[^1] and[^2].\n\n[^1]: one\n\n# A\n\n```rust\nfn main() {}\n";
        let doc = Doc::from_str(src, Default::default());
        let r = inspect(&doc);
        assert_eq!(r.headings.len(), 2);
        assert_eq!(r.links.len(), 4);
        assert_eq!(r.images.len(), 2);
        assert_eq!(
            r.to_json()["counts"]["words"].as_u64().map(|w| w > 5),
            Some(true)
        );
        let kinds: Vec<&str> = lint(&doc, Some(&d.path().join("doc.md")))
            .iter()
            .map(|p| p.kind)
            .collect();
        assert_eq!(
            kinds,
            [
                "broken-anchor",
                "missing-file",
                "missing-image",
                "undefined-footnote",
                "duplicate-heading",
                "unclosed-fence"
            ]
        );
    }
}
