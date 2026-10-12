//! Hand-written scanners for CommonMark constructs. All functions take a byte
//! slice starting at the candidate position and return the matched length.

/// Whether `b` is a space or tab.
#[inline]
pub fn is_space_or_tab(b: u8) -> bool {
    b == b' ' || b == b'\t'
}

/// Unicode whitespace per CommonMark (`Zs` plus tab, LF, FF, CR).
pub fn is_unicode_whitespace(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\u{c}' | '\r')
        || matches!(
            c,
            '\u{a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}' | '\u{202f}' | '\u{205f}' | '\u{3000}'
        )
}

/// Unicode punctuation per CommonMark 0.31 (`P*` and `S*` categories). ASCII is
/// exact; for non-ASCII we cover the common punctuation and symbol blocks.
pub fn is_punctuation(c: char) -> bool {
    if c.is_ascii() {
        return c.is_ascii_punctuation();
    }
    let u = c as u32;
    matches!(u,
        0x00A1..=0x00BF | 0x00D7 | 0x00F7
        | 0x02C2..=0x02C5 | 0x02D2..=0x02DF | 0x02E5..=0x02EB | 0x02ED | 0x02EF..=0x02FF
        | 0x037E | 0x0384 | 0x0385 | 0x0387 | 0x03F6
        | 0x055A..=0x055F | 0x0589 | 0x058A | 0x05BE | 0x05C0 | 0x05C3 | 0x05C6 | 0x05F3 | 0x05F4
        | 0x0606..=0x060F | 0x061B | 0x061D..=0x061F | 0x066A..=0x066D | 0x06D4
        | 0x0964 | 0x0965 | 0x0970 | 0x0E4F | 0x0E5A | 0x0E5B
        | 0x2010..=0x2027 | 0x2030..=0x205E | 0x207A..=0x207E | 0x208A..=0x208E
        | 0x20A0..=0x20C0 | 0x2100..=0x214F | 0x2190..=0x23FF | 0x2400..=0x2426 | 0x2440..=0x244A
        | 0x2500..=0x2775 | 0x2794..=0x2BFF | 0x2CF9..=0x2CFC | 0x2CFE | 0x2CFF
        | 0x2E00..=0x2E7F | 0x2E80..=0x2FFB
        | 0x3001..=0x3004 | 0x3008..=0x3020 | 0x3030 | 0x303D..=0x303F | 0x309B | 0x309C | 0x30A0 | 0x30FB
        | 0xA4FE | 0xA4FF | 0xA60D..=0xA60F | 0xA673 | 0xA67E
        | 0xFD3E | 0xFD3F | 0xFE10..=0xFE19 | 0xFE30..=0xFE6B
        | 0xFF01..=0xFF0F | 0xFF1A..=0xFF20 | 0xFF3B..=0xFF40 | 0xFF5B..=0xFF65
        | 0xFFE0..=0xFFEE | 0xFFFC | 0xFFFD
        | 0x1F000..=0x1FAFF
    )
}

/// Length of a run of `ch` at the start of `s`.
pub fn run_len(s: &[u8], ch: u8) -> usize {
    s.iter().take_while(|&&b| b == ch).count()
}

/// Whether the rest of `s` is only spaces/tabs (or empty).
pub fn is_blank_rest(s: &[u8]) -> bool {
    s.iter().all(|&b| is_space_or_tab(b))
}

/// `^(?:\*[ \t]*){3,}$|^(?:_[ \t]*){3,}$|^(?:-[ \t]*){3,}$`
pub fn thematic_break(s: &[u8]) -> bool {
    let Some(&c) = s.first() else { return false };
    if !matches!(c, b'*' | b'_' | b'-') {
        return false;
    }
    let mut n = 0;
    for &b in s {
        if b == c {
            n += 1;
        } else if !is_space_or_tab(b) {
            return false;
        }
    }
    n >= 3
}

/// `^(?:=+|-+)[ \t]*$`; returns the heading level.
pub fn setext_underline(s: &[u8]) -> Option<u8> {
    let c = *s.first()?;
    if c != b'=' && c != b'-' {
        return None;
    }
    let n = run_len(s, c);
    is_blank_rest(&s[n..]).then_some(if c == b'=' { 1 } else { 2 })
}

/// ATX opening: `#{1,6}` followed by space/tab/end (or, when `lenient`, by any
/// character other than `#`). Returns the number of `#`.
pub fn atx_marker(s: &[u8], lenient: bool) -> Option<usize> {
    let n = run_len(s, b'#');
    if n == 0 || n > 6 {
        return None;
    }
    match s.get(n) {
        None => Some(n),
        Some(&b) if is_space_or_tab(b) => Some(n),
        Some(_) if lenient => Some(n),
        _ => None,
    }
}

/// Opening code fence: ```` ``` ```` (info without backticks) or `~~~`. Returns
/// `(char, length)`.
pub fn code_fence(s: &[u8]) -> Option<(u8, usize)> {
    let c = *s.first()?;
    if c != b'`' && c != b'~' {
        return None;
    }
    let n = run_len(s, c);
    if n < 3 {
        return None;
    }
    if c == b'`' && s[n..].contains(&b'`') {
        return None;
    }
    Some((c, n))
}

/// Closing code fence of at least `min` `c` characters followed only by spaces.
pub fn closing_fence(s: &[u8], c: u8, min: usize) -> bool {
    let n = run_len(s, c);
    n >= min && is_blank_rest(&s[n..])
}

fn starts_with_ci(s: &[u8], prefix: &[u8]) -> bool {
    s.len() >= prefix.len() && s[..prefix.len()].eq_ignore_ascii_case(prefix)
}

const TYPE6: &[&str] = &[
    "address",
    "article",
    "aside",
    "base",
    "basefont",
    "blockquote",
    "body",
    "caption",
    "center",
    "col",
    "colgroup",
    "dd",
    "details",
    "dialog",
    "dir",
    "div",
    "dl",
    "dt",
    "fieldset",
    "figcaption",
    "figure",
    "footer",
    "form",
    "frame",
    "frameset",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "head",
    "header",
    "hr",
    "html",
    "iframe",
    "legend",
    "li",
    "link",
    "main",
    "menu",
    "menuitem",
    "nav",
    "noframes",
    "ol",
    "optgroup",
    "option",
    "p",
    "param",
    "search",
    "section",
    "summary",
    "table",
    "tbody",
    "td",
    "tfoot",
    "th",
    "thead",
    "title",
    "tr",
    "track",
    "ul",
];

/// HTML block start condition (types 1–7). `s` starts at `<`.
pub fn html_block_start(s: &[u8]) -> Option<u8> {
    if s.first() != Some(&b'<') {
        return None;
    }
    let rest = &s[1..];
    for tag in ["pre", "script", "style", "textarea"] {
        if starts_with_ci(rest, tag.as_bytes()) {
            match rest.get(tag.len()) {
                None | Some(b' ' | b'\t' | b'>' | b'\n' | b'\r') => return Some(1),
                _ => {}
            }
        }
    }
    if rest.starts_with(b"!--") {
        return Some(2);
    }
    if rest.starts_with(b"?") {
        return Some(3);
    }
    if rest.len() >= 2 && rest[0] == b'!' && rest[1].is_ascii_alphabetic() {
        return Some(4);
    }
    if rest.starts_with(b"![CDATA[") {
        return Some(5);
    }
    let name_start = if rest.first() == Some(&b'/') { 1 } else { 0 };
    let name_len = rest[name_start..]
        .iter()
        .take_while(|b| b.is_ascii_alphanumeric())
        .count();
    if name_len > 0 {
        let name = &rest[name_start..name_start + name_len];
        if TYPE6
            .iter()
            .any(|t| t.as_bytes().eq_ignore_ascii_case(name))
        {
            let after = &rest[name_start + name_len..];
            match after.first() {
                None | Some(b' ' | b'\t' | b'>' | b'\n' | b'\r') => return Some(6),
                Some(b'/') if after.get(1) == Some(&b'>') => return Some(6),
                _ => {}
            }
        }
    }
    // Type 7: a complete open or closing tag followed only by whitespace.
    let n = open_tag(s).or_else(|| closing_tag(s))?;
    is_blank_rest(&s[n..]).then_some(7)
}

/// HTML block end condition for types 1–5 on a line.
pub fn html_block_end(kind: u8, line: &[u8]) -> bool {
    match kind {
        1 => {
            let lower = line.to_ascii_lowercase();
            ["</pre>", "</script>", "</style>", "</textarea>"]
                .iter()
                .any(|t| lower.windows(t.len()).any(|w| w == t.as_bytes()))
        }
        2 => line.windows(3).any(|w| w == b"-->"),
        3 => line.windows(2).any(|w| w == b"?>"),
        4 => line.contains(&b'>'),
        5 => line.windows(3).any(|w| w == b"]]>"),
        _ => false,
    }
}

fn is_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r' | b'\x0c')
}

fn tag_name(s: &[u8]) -> usize {
    if !s.first().is_some_and(|b| b.is_ascii_alphabetic()) {
        return 0;
    }
    1 + s[1..]
        .iter()
        .take_while(|b| b.is_ascii_alphanumeric() || **b == b'-')
        .count()
}

/// Whitespace allowing at most one line ending.
fn ws_one_newline(s: &[u8]) -> usize {
    let mut i = 0;
    let mut newlines = 0;
    while i < s.len() && is_ws(s[i]) {
        if s[i] == b'\n' {
            newlines += 1;
            if newlines > 1 {
                break;
            }
        }
        i += 1;
    }
    i
}

/// Open tag `<name attr="v" ...>` or `<name/>`; returns the length.
pub fn open_tag(s: &[u8]) -> Option<usize> {
    if s.first() != Some(&b'<') {
        return None;
    }
    let mut i = 1;
    let n = tag_name(&s[i..]);
    if n == 0 {
        return None;
    }
    i += n;
    loop {
        // attribute: whitespace+ name (= value)?
        let ws = ws_one_newline(&s[i..]);
        let j = i + ws;
        let an = attr_name(&s[j..]);
        if ws > 0 && an > 0 {
            i = j + an;
            // optional value spec
            let ws2 = ws_one_newline(&s[i..]);
            if s.get(i + ws2) == Some(&b'=') {
                let k = i + ws2 + 1;
                let ws3 = ws_one_newline(&s[k..]);
                let v = attr_value(&s[k + ws3..])?;
                i = k + ws3 + v;
            }
            continue;
        }
        i = j;
        break;
    }
    if s.get(i) == Some(&b'/') {
        i += 1;
    }
    (s.get(i) == Some(&b'>')).then_some(i + 1)
}

fn attr_name(s: &[u8]) -> usize {
    match s.first() {
        Some(b) if b.is_ascii_alphabetic() || *b == b'_' || *b == b':' => {
            1 + s[1..]
                .iter()
                .take_while(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b':' | b'-'))
                .count()
        }
        _ => 0,
    }
}

fn attr_value(s: &[u8]) -> Option<usize> {
    match s.first()? {
        q @ (b'"' | b'\'') => s[1..].iter().position(|b| b == q).map(|p| p + 2),
        _ => {
            let n = s
                .iter()
                .take_while(|&&b| {
                    !matches!(b, b'"' | b'\'' | b'=' | b'<' | b'>' | b'`') && b > b' '
                })
                .count();
            (n > 0).then_some(n)
        }
    }
}

/// Closing tag `</name>`.
pub fn closing_tag(s: &[u8]) -> Option<usize> {
    if !s.starts_with(b"</") {
        return None;
    }
    let n = tag_name(&s[2..]);
    if n == 0 {
        return None;
    }
    let mut i = 2 + n;
    i += ws_one_newline(&s[i..]);
    (s.get(i) == Some(&b'>')).then_some(i + 1)
}

fn find(s: &[u8], needle: &[u8]) -> Option<usize> {
    s.windows(needle.len()).position(|w| w == needle)
}

/// Inline raw HTML (open/closing tag, comment, processing instruction,
/// declaration, CDATA); returns the length.
pub fn inline_html(s: &[u8]) -> Option<usize> {
    if s.first() != Some(&b'<') {
        return None;
    }
    if let Some(n) = open_tag(s).or_else(|| closing_tag(s)) {
        return Some(n);
    }
    if s.starts_with(b"<!--") {
        // `<!-->` and `<!--->` are complete comments (0.31).
        if s[4..].starts_with(b">") {
            return Some(5);
        }
        if s[4..].starts_with(b"->") {
            return Some(6);
        }
        return find(&s[4..], b"-->").map(|p| 4 + p + 3);
    }
    if s.starts_with(b"<?") {
        return find(&s[2..], b"?>").map(|p| 2 + p + 2);
    }
    if s.starts_with(b"<![CDATA[") {
        return find(&s[9..], b"]]>").map(|p| 9 + p + 3);
    }
    if s.len() > 2 && s[1] == b'!' && s[2].is_ascii_alphabetic() {
        return s[2..].iter().position(|&b| b == b'>').map(|p| 2 + p + 1);
    }
    None
}

/// Bullet or ordered list marker at the start of `s`. Returns
/// `(marker length, bullet char or ordered delimiter, ordered start)`.
pub fn list_marker(s: &[u8]) -> Option<(usize, u8, Option<u64>)> {
    match *s.first()? {
        c @ (b'-' | b'+' | b'*') => Some((1, c, None)),
        b'0'..=b'9' => {
            let n = s.iter().take_while(|b| b.is_ascii_digit()).count();
            if n > 9 {
                return None;
            }
            let d = *s.get(n)?;
            if d != b'.' && d != b')' {
                return None;
            }
            let num = std::str::from_utf8(&s[..n]).ok()?.parse().ok()?;
            Some((n + 1, d, Some(num)))
        }
        _ => None,
    }
}

/// Splits a table row into cell ranges (relative to `s`), honouring `\|`
/// escapes. Leading and trailing pipes are dropped; cell ranges are trimmed.
pub fn table_cells(s: &[u8]) -> Vec<std::ops::Range<usize>> {
    let mut cells = Vec::new();
    let mut i = 0;
    let len = s.len();
    // trim leading whitespace and an optional leading pipe
    while i < len && is_space_or_tab(s[i]) {
        i += 1;
    }
    if i < len && s[i] == b'|' {
        i += 1;
    }
    let mut start = i;
    let mut had_trailing_pipe = false;
    while i < len {
        match s[i] {
            b'\\' if i + 1 < len => i += 2,
            b'|' => {
                cells.push(start..i);
                i += 1;
                start = i;
                had_trailing_pipe = is_blank_rest(&s[i..]);
            }
            _ => i += 1,
        }
    }
    if !had_trailing_pipe || start < len && !is_blank_rest(&s[start..]) {
        cells.push(start..len);
    }
    cells
        .into_iter()
        .map(|r| {
            let mut a = r.start;
            let mut b = r.end;
            while a < b && is_space_or_tab(s[a]) {
                a += 1;
            }
            while b > a && is_space_or_tab(s[b - 1]) {
                b -= 1;
            }
            a..b
        })
        .collect()
}

/// Parses a GFM table delimiter row; returns the alignments.
pub fn table_delimiter_row(s: &[u8]) -> Option<Vec<crate::ast::Align>> {
    use crate::ast::Align;
    if !s.contains(&b'-') {
        return None;
    }
    let has_pipe = s.contains(&b'|');
    let cells = table_cells(s);
    if cells.is_empty() {
        return None;
    }
    let mut aligns = Vec::with_capacity(cells.len());
    for r in &cells {
        let c = &s[r.clone()];
        if c.is_empty() {
            return None;
        }
        let left = c[0] == b':';
        let right = c.len() > 1 && c[c.len() - 1] == b':';
        let inner = &c[usize::from(left)..c.len() - usize::from(right)];
        if inner.is_empty() || !inner.iter().all(|&b| b == b'-') {
            return None;
        }
        aligns.push(match (left, right) {
            (true, true) => Align::Center,
            (true, false) => Align::Left,
            (false, true) => Align::Right,
            (false, false) => Align::None,
        });
    }
    // A single column without any pipe is a setext underline or thematic break.
    (has_pipe || aligns.len() > 1).then_some(aligns)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn breaks_and_headings() {
        assert!(thematic_break(b"* * *"));
        assert!(thematic_break(b"---"));
        assert!(!thematic_break(b"--"));
        assert!(!thematic_break(b"-- a"));
        assert_eq!(setext_underline(b"===  "), Some(1));
        assert_eq!(setext_underline(b"= ="), None);
        assert_eq!(atx_marker(b"## x", false), Some(2));
        assert_eq!(atx_marker(b"##x", false), None);
        assert_eq!(atx_marker(b"##x", true), Some(2));
        assert_eq!(atx_marker(b"#######", false), None);
    }

    #[test]
    fn html() {
        assert_eq!(html_block_start(b"<div>"), Some(6));
        assert_eq!(html_block_start(b"<DIV class=x>"), Some(6));
        assert_eq!(html_block_start(b"<!-- c"), Some(2));
        assert_eq!(html_block_start(b"<a href=\"x\">"), Some(7));
        assert_eq!(html_block_start(b"<a href=\"x\"> text"), None);
        assert_eq!(inline_html(b"<a href=\"x\" title='y'>z"), Some(22));
        assert_eq!(inline_html(b"<!-- x -->"), Some(10));
        assert_eq!(inline_html(b"<a h=>"), None);
        assert_eq!(closing_tag(b"</em >"), Some(6));
    }

    #[test]
    fn tables() {
        use crate::ast::Align;
        assert_eq!(
            table_delimiter_row(b"| :-- | :-: | --: | --- |"),
            Some(vec![Align::Left, Align::Center, Align::Right, Align::None])
        );
        assert_eq!(table_delimiter_row(b"---"), None);
        assert_eq!(table_delimiter_row(b"--- | ---").map(|a| a.len()), Some(2));
        let s = b"| a | b \\| c |";
        let cells: Vec<_> = table_cells(s).into_iter().map(|r| &s[r]).collect();
        assert_eq!(cells, [&b"a"[..], &b"b \\| c"[..]]);
        let s = b"a|b";
        assert_eq!(table_cells(s).len(), 2);
        let s = b"| a |  ";
        assert_eq!(table_cells(s).len(), 1);
    }
}
