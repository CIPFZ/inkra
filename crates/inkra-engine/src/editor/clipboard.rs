//! Clipboard payloads (ED-016).

use inkra_doc::{Doc, Selection, Transaction};

use super::context;

/// What the system clipboard holds.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClipboardContent {
    /// `text/plain`.
    pub text: Option<String>,
    /// `text/html`.
    pub html: Option<String>,
    /// File paths (copied files).
    pub files: Vec<std::path::PathBuf>,
    /// Whether the content came from Inkra itself (no HTML conversion then).
    pub from_inkra: bool,
}

/// Text written to the clipboard on copy: `(text/plain, text/html)`.
pub fn copy_payload(doc: &Doc, sel: Selection) -> Option<(String, String)> {
    if sel.is_empty() {
        return None;
    }
    let src = doc.text().slice(sel.range()).into_owned();
    let html = inkra_markdown::html::to_html(&src, doc.options());
    Some((src, html))
}

/// A web address of a picture (by its extension).
pub fn is_image_url(s: &str) -> bool {
    let t = s.trim();
    (t.starts_with("http://") || t.starts_with("https://")) && !t.contains(char::is_whitespace) && {
        let path = t.split(['?', '#']).next().unwrap_or(t).to_ascii_lowercase();
        [".png", ".jpg", ".jpeg", ".gif", ".webp", ".svg", ".bmp"]
            .iter()
            .any(|e| path.ends_with(e))
    }
}

fn looks_like_url(s: &str) -> bool {
    let t = s.trim();
    !t.contains(char::is_whitespace)
        && (t.starts_with("http://")
            || t.starts_with("https://")
            || t.starts_with("mailto:")
            || t.starts_with("www."))
}

/// Converts AI-tool math delimiters `\(..\)` / `\[..\]` to `$..$` / `$$..$$`
/// (ED-016 rule 6).
pub fn normalize_math_delimiters(s: &str) -> String {
    if !s.contains("\\(") && !s.contains("\\[") {
        return s.to_owned();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find(['\\']) {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        if let Some(body) = tail.strip_prefix("\\(") {
            if let Some(end) = body.find("\\)") {
                out.push('$');
                out.push_str(body[..end].trim());
                out.push('$');
                rest = &body[end + 2..];
                continue;
            }
        } else if let Some(body) = tail.strip_prefix("\\[")
            && let Some(end) = body.find("\\]")
        {
            out.push_str("$$");
            out.push_str(body[..end].trim_matches(|c: char| c == ' '));
            out.push_str("$$");
            rest = &body[end + 2..];
            continue;
        }
        out.push('\\');
        rest = &tail[1..];
    }
    out.push_str(rest);
    out
}

/// Transaction for pasting `c` (ED-016 rules 2–6). `plain` forces plain text
/// (`Ctrl+Shift+V`). Image data is handled by the image insertion flow.
pub fn paste(doc: &Doc, sel: Selection, c: &ClipboardContent, plain: bool) -> Option<Transaction> {
    let pos = sel.start();
    let in_code = context::in_code_content(doc, pos);
    let r = sel.range();
    let text = if !c.files.is_empty() && !plain {
        let links: Vec<String> = c
            .files
            .iter()
            .map(|p| {
                let name = p
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let url = p.to_string_lossy().replace('\\', "/").replace(' ', "%20");
                let image = matches!(
                    p.extension()
                        .and_then(|e| e.to_str())
                        .map(str::to_ascii_lowercase)
                        .as_deref(),
                    Some("png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "svg" | "tif" | "tiff")
                );
                if image {
                    format!("![{name}]({url})")
                } else {
                    format!("[{name}]({url})")
                }
            })
            .collect();
        links.join(doc.newline())
    } else if let (Some(html), false, false, false) = (&c.html, plain, in_code, c.from_inkra) {
        let md = inkra_html::html_to_markdown(html);
        if md.trim().is_empty() {
            c.text.clone()?
        } else {
            md
        }
    } else {
        c.text.clone()?
    };
    let text = if in_code || plain {
        text
    } else {
        normalize_math_delimiters(&text)
    };
    // URL pasted onto a selection makes a link.
    if !plain && !in_code && !sel.is_empty() && looks_like_url(&text) {
        let label = doc.text().slice(r.clone()).into_owned();
        if !label.contains('\n') {
            let link = format!("[{label}]({})", text.trim());
            let end = r.start + link.len();
            return Some(
                Transaction::single(r, &link)
                    .with_selection_before(sel)
                    .with_selection_after(Selection::caret(end)),
            );
        }
    }
    // ED-092: a pasted image URL becomes an image.
    let text = if !plain && !in_code && sel.is_empty() && is_image_url(&text) {
        format!("![]({})", text.trim())
    } else {
        text
    };
    let text = if doc.newline() == "\n" {
        text.replace("\r\n", "\n")
    } else {
        text
    };
    let end = r.start + text.len();
    Some(
        Transaction::single(r, &text)
            .with_selection_before(sel)
            .with_selection_after(Selection::caret(end)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn math_delimiters() {
        assert_eq!(
            normalize_math_delimiters(r"a \(x^2\) b \[ y \]"),
            "a $x^2$ b $$y$$"
        );
        assert_eq!(normalize_math_delimiters(r"c:\path"), r"c:\path");
    }

    #[test]
    fn url_on_selection_makes_link() {
        let doc = Doc::from_str("see docs here", Default::default());
        let c = ClipboardContent {
            text: Some("https://x.io".into()),
            ..Default::default()
        };
        let tx = paste(&doc, Selection::new(4, 8), &c, false).unwrap();
        assert_eq!(tx.edits[0].text, "[docs](https://x.io)");
    }
}
