//! Link labels, destinations and titles; shared by link reference definitions
//! and inline links.

use crate::entities;
use crate::scan::is_space_or_tab;

/// Normalizes a link label for matching: Unicode case fold (approximated by
/// lower-casing plus `ß` → `ss`) and collapsing internal whitespace.
pub fn normalize_label(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut pending_space = false;
    for c in s.trim_matches(|c: char| c.is_whitespace()).chars() {
        if c.is_whitespace() {
            pending_space = true;
            continue;
        }
        if pending_space {
            out.push(' ');
            pending_space = false;
        }
        for l in c.to_lowercase() {
            if l == 'ß' {
                out.push_str("ss");
            } else {
                out.push(l);
            }
        }
    }
    out
}

/// Scans a link label `[...]` at the start of `s` (max 999 chars, no unescaped
/// brackets). Returns the total length including brackets.
pub fn scan_label(s: &[u8]) -> Option<usize> {
    if s.first() != Some(&b'[') {
        return None;
    }
    let mut i = 1;
    let mut nonblank = false;
    while i < s.len() {
        match s[i] {
            b'\\' if i + 1 < s.len() => {
                nonblank = true;
                i += 2;
                continue;
            }
            b'[' => return None,
            b']' => {
                return (nonblank && i <= 1000).then_some(i + 1);
            }
            b if !b.is_ascii_whitespace() => nonblank = true,
            _ => {}
        }
        i += 1;
        if i > 1001 {
            return None;
        }
    }
    None
}

/// Scans a link destination at the start of `s`. Returns `(length, raw
/// destination without angle brackets)`.
pub fn scan_destination(s: &[u8]) -> Option<(usize, &[u8])> {
    if s.first() == Some(&b'<') {
        let mut i = 1;
        while i < s.len() {
            match s[i] {
                b'\\' if i + 1 < s.len() => i += 2,
                b'>' => return Some((i + 1, &s[1..i])),
                b'<' | b'\n' | b'\r' => return None,
                _ => i += 1,
            }
        }
        return None;
    }
    let mut depth = 0usize;
    let mut i = 0;
    while i < s.len() {
        match s[i] {
            b'\\' if i + 1 < s.len() && s[i + 1].is_ascii_punctuation() => i += 2,
            b'(' => {
                depth += 1;
                if depth > 32 {
                    return None;
                }
                i += 1;
            }
            b')' => {
                if depth == 0 {
                    break;
                }
                depth -= 1;
                i += 1;
            }
            b if b <= b' ' || b == 0x7f => break,
            _ => i += 1,
        }
    }
    if i == 0 || depth != 0 {
        return None;
    }
    Some((i, &s[..i]))
}

/// Scans a link title (`"…"`, `'…'` or `(…)`) at the start of `s`. Returns
/// `(length, raw title without delimiters)`.
pub fn scan_title(s: &[u8]) -> Option<(usize, &[u8])> {
    let open = *s.first()?;
    let close = match open {
        b'"' => b'"',
        b'\'' => b'\'',
        b'(' => b')',
        _ => return None,
    };
    let mut i = 1;
    while i < s.len() {
        match s[i] {
            b'\\' if i + 1 < s.len() => i += 2,
            b if b == close => return Some((i + 1, &s[1..i])),
            b'(' if open == b'(' => return None,
            _ => i += 1,
        }
    }
    None
}

/// Resolves backslash escapes and entity references in `s`.
pub fn unescape(s: &str) -> String {
    if !s.contains(['\\', '&']) {
        return s.to_owned();
    }
    let b = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    let mut last = 0;
    while i < b.len() {
        match b[i] {
            b'\\' if i + 1 < b.len() && b[i + 1].is_ascii_punctuation() => {
                out.push_str(&s[last..i]);
                out.push(b[i + 1] as char);
                i += 2;
                last = i;
            }
            b'&' => {
                if let Some((len, text)) = scan_entity(&b[i..]) {
                    out.push_str(&s[last..i]);
                    out.push_str(&text);
                    i += len;
                    last = i;
                } else {
                    i += 1;
                }
            }
            _ => i += 1,
        }
    }
    out.push_str(&s[last..]);
    out
}

/// Scans an entity or numeric character reference at the start of `s`
/// (`&name;`, `&#123;`, `&#x1F;`). Returns `(length, decoded text)`.
pub fn scan_entity(s: &[u8]) -> Option<(usize, String)> {
    if s.first() != Some(&b'&') {
        return None;
    }
    if s.get(1) == Some(&b'#') {
        let (hex, start) = match s.get(2) {
            Some(b'x' | b'X') => (true, 3),
            _ => (false, 2),
        };
        let digits = s[start..]
            .iter()
            .take_while(|b| {
                if hex {
                    b.is_ascii_hexdigit()
                } else {
                    b.is_ascii_digit()
                }
            })
            .count();
        let max = if hex { 6 } else { 7 };
        if digits == 0 || digits > max || s.get(start + digits) != Some(&b';') {
            return None;
        }
        let text = std::str::from_utf8(&s[start..start + digits]).ok()?;
        let n = u32::from_str_radix(text, if hex { 16 } else { 10 }).ok()?;
        let c = if n == 0 {
            '\u{FFFD}'
        } else {
            char::from_u32(n).unwrap_or('\u{FFFD}')
        };
        return Some((start + digits + 1, c.to_string()));
    }
    let n = s[1..]
        .iter()
        .take_while(|b| b.is_ascii_alphanumeric())
        .count();
    if n == 0 || s.get(1 + n) != Some(&b';') {
        return None;
    }
    let name = std::str::from_utf8(&s[1..1 + n]).ok()?;
    entities::lookup(name).map(|t| (n + 2, t.to_owned()))
}

/// A parsed link reference definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefDef {
    /// Bytes consumed, including the final line ending.
    pub len: usize,
    /// Normalized label.
    pub label: String,
    /// Unescaped destination.
    pub dest: String,
    /// Unescaped title.
    pub title: Option<String>,
}

fn skip_ws_one_newline(s: &[u8], mut i: usize) -> (usize, bool) {
    let mut newline = false;
    while i < s.len() {
        match s[i] {
            b' ' | b'\t' => i += 1,
            b'\n' if !newline => {
                newline = true;
                i += 1;
            }
            _ => break,
        }
    }
    (i, newline)
}

/// Parses a link reference definition at the start of `s` (paragraph content
/// joined with `\n`, ending with `\n`).
pub fn parse_ref_def(s: &str) -> Option<RefDef> {
    let b = s.as_bytes();
    let label_len = scan_label(b)?;
    if b.get(label_len) != Some(&b':') {
        return None;
    }
    let label = normalize_label(&s[1..label_len - 1]);
    if label.is_empty() {
        return None;
    }
    let (i, _) = skip_ws_one_newline(b, label_len + 1);
    let (dlen, raw_dest) = scan_destination(&b[i..])?;
    if raw_dest.is_empty() && b[i] != b'<' {
        return None;
    }
    let dest = unescape(std::str::from_utf8(raw_dest).ok()?);
    let after_dest = i + dlen;
    // Optional title: must be separated from the destination by whitespace.
    let (j, _) = skip_ws_one_newline(b, after_dest);
    let mut title = None;
    let mut end = after_dest;
    if j > after_dest
        && let Some((tlen, raw)) = scan_title(&b[j..])
    {
        let k = j + tlen;
        let rest_end = k + b[k..].iter().take_while(|&&c| is_space_or_tab(c)).count();
        if rest_end >= b.len() || b[rest_end] == b'\n' {
            title = Some(unescape(std::str::from_utf8(raw).ok()?));
            end = k;
        }
    }
    // The rest of the line must be blank.
    let rest_end = end + b[end..].iter().take_while(|&&c| is_space_or_tab(c)).count();
    if rest_end < b.len() && b[rest_end] != b'\n' {
        if title.is_some() {
            return None;
        }
        // Title failed; destination line itself must end here.
        return None;
    }
    let len = if rest_end < b.len() {
        rest_end + 1
    } else {
        rest_end
    };
    Some(RefDef {
        len,
        label,
        dest,
        title,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels() {
        assert_eq!(normalize_label("  Foo\n  BAR "), "foo bar");
        assert_eq!(normalize_label("ẞ"), normalize_label("SS"));
        assert_eq!(scan_label(b"[foo] x"), Some(5));
        assert_eq!(scan_label(b"[fo[o]"), None);
        assert_eq!(scan_label(b"[ ]"), None);
        assert_eq!(scan_label(b"[a\\]b]"), Some(6));
    }

    #[test]
    fn destinations_and_titles() {
        assert_eq!(scan_destination(b"<a b> x"), Some((5, &b"a b"[..])));
        assert_eq!(scan_destination(b"a(b)c) x"), Some((5, &b"a(b)c"[..])));
        assert_eq!(scan_title(b"\"t\\\"x\" y"), Some((6, &b"t\\\"x"[..])));
        assert_eq!(unescape("a\\*b&amp;&#35;&#x41;&bogus;"), "a*b&#A&bogus;");
    }

    #[test]
    fn ref_defs() {
        let d = parse_ref_def("[Foo]: /url \"title\"\nrest\n").unwrap();
        assert_eq!(
            (d.label.as_str(), d.dest.as_str(), d.title.as_deref()),
            ("foo", "/url", Some("title"))
        );
        assert_eq!(d.len, 20);
        let d = parse_ref_def("[foo]:\n/url\n").unwrap();
        assert_eq!(d.dest, "/url");
        assert!(parse_ref_def("[foo]: /url \"title\" ok\n").is_none());
        let d = parse_ref_def("[foo]: /url\n\"title\" ok\n").unwrap();
        assert_eq!((d.title, d.len), (None, 12));
        assert!(parse_ref_def("[foo]:\n").is_none());
        let d = parse_ref_def("[foo]: <>\n").unwrap();
        assert_eq!(d.dest, "");
    }
}
