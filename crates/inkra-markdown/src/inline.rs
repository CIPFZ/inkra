//! Inline parser (CommonMark 0.31 inlines + GFM strikethrough/autolinks and
//! Inkra extensions). Works on the joined content of a leaf block and maps every
//! range back to document offsets.

use std::collections::HashMap;

use crate::ast::{Inline, InlineKind, Line, LinkSyntax, Span};
use crate::options::ParseOptions;
use crate::refs::{self, normalize_label, scan_entity, unescape};
use crate::scan::{self, is_punctuation, is_unicode_whitespace};

/// A link reference definition target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkTarget {
    /// Destination.
    pub dest: String,
    /// Title.
    pub title: Option<String>,
}

/// Context for inline parsing: definitions found in the document.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct InlineContext {
    /// Link reference definitions by normalized label (first definition wins).
    pub refs: HashMap<String, LinkTarget>,
    /// Defined footnote labels (normalized).
    pub footnotes: std::collections::HashSet<String>,
}

/// Joined content of a leaf with a mapping to document offsets.
#[derive(Debug, Clone)]
pub struct Subject {
    /// The text.
    pub text: String,
    /// `(subject offset, document offset, length)` per line.
    segs: Vec<(usize, usize, usize)>,
}

impl Subject {
    /// Builds the subject from leaf `lines` read from `src`. When `trim_end` is
    /// set, final whitespace is removed (paragraph rule). When `unescape_pipes` is
    /// set (table cells), `\|` becomes `|` before inline parsing (GFM).
    pub fn new<S: crate::Source + ?Sized>(
        src: &S,
        lines: &[Line],
        trim_end: bool,
        unescape_pipes: bool,
    ) -> Self {
        let mut text = String::new();
        let mut segs = Vec::with_capacity(lines.len());
        for (i, l) in lines.iter().enumerate() {
            if i > 0 {
                text.push('\n');
            }
            let s = src.slice(l.span.clone());
            if unescape_pipes && s.contains("\\|") {
                let mut last = 0;
                let b = s.as_bytes();
                let mut j = 0;
                while j < b.len() {
                    if b[j] == b'\\' && b.get(j + 1) == Some(&b'|') {
                        segs.push((text.len(), l.span.start + last, j - last));
                        text.push_str(&s[last..j]);
                        last = j + 1;
                        j += 2;
                    } else {
                        j += 1;
                    }
                }
                segs.push((text.len(), l.span.start + last, s.len() - last));
                text.push_str(&s[last..]);
            } else {
                segs.push((text.len(), l.span.start, s.len()));
                text.push_str(&s);
            }
        }
        if trim_end {
            let t = text.trim_end_matches([' ', '\t', '\n', '\r']).len();
            text.truncate(t);
            while let Some(last) = segs.last_mut() {
                if last.0 > t {
                    segs.pop();
                } else {
                    last.2 = last.2.min(t - last.0);
                    break;
                }
            }
        }
        Self { text, segs }
    }

    /// Maps a subject offset to a document offset.
    pub fn map(&self, pos: usize) -> usize {
        if self.segs.is_empty() {
            return 0;
        }
        let i = match self.segs.binary_search_by(|s| s.0.cmp(&pos)) {
            Ok(i) => i,
            Err(0) => 0,
            Err(i) => i - 1,
        };
        let (s, d, len) = self.segs[i];
        d + (pos - s).min(len)
    }

    fn map_span(&self, r: &Span) -> Span {
        let a = self.map(r.start);
        let b = if r.end > r.start {
            self.map_end(r.end)
        } else {
            a
        };
        a..b.max(a)
    }

    /// Splits a subject range at segment boundaries so each piece maps to a
    /// contiguous source range.
    fn split(&self, r: &Span) -> Vec<Span> {
        let mut out = Vec::new();
        let mut start = r.start;
        for &(s, _, len) in &self.segs {
            let seg_end = s + len;
            if seg_end <= start || s >= r.end {
                continue;
            }
            let a = start.max(s);
            let b = r.end.min(seg_end);
            if a < b {
                out.push(a..b);
            }
            start = b;
        }
        if out.is_empty() {
            out.push(r.clone());
        }
        out
    }

    /// Maps an exclusive end offset (a position right after a `\n` maps to the
    /// start of the next line's content).
    fn map_end(&self, pos: usize) -> usize {
        // An end exactly at the end of a segment belongs to that segment, not to
        // the start of the following one.
        let i = self.segs.partition_point(|&(s, _, len)| s + len < pos);
        if let Some(&(s, d, len)) = self.segs.get(i)
            && s + len == pos
            && len > 0
        {
            return d + (pos - s);
        }
        self.map(pos)
    }
}

const NONE: usize = usize::MAX;

#[derive(Debug, Clone)]
struct N {
    kind: InlineKind,
    span: Span,
    parent: usize,
    first: usize,
    last: usize,
    prev: usize,
    next: usize,
    /// Text node that holds delimiter or bracket characters.
    literal: bool,
}

#[derive(Debug, Clone)]
struct Delim {
    cc: u8,
    numdelims: usize,
    origdelims: usize,
    node: usize,
    prev: usize,
    next: usize,
    can_open: bool,
    can_close: bool,
}

#[derive(Debug, Clone)]
struct Bracket {
    node: usize,
    prev: usize,
    prev_delim: usize,
    index: usize,
    image: bool,
    active: bool,
    bracket_after: bool,
}

struct Parser<'a> {
    o: &'a ParseOptions,
    cx: &'a InlineContext,
    s: &'a [u8],
    text: &'a str,
    pos: usize,
    nodes: Vec<N>,
    delims: Vec<Delim>,
    delim_top: usize,
    brackets: Vec<Bracket>,
    bracket_top: usize,
    backticks: Option<HashMap<usize, Vec<usize>>>,
}

/// Parses inline content.
pub fn parse_inlines(subject: &Subject, o: &ParseOptions, cx: &InlineContext) -> Vec<Inline> {
    let mut p = Parser {
        o,
        cx,
        s: subject.text.as_bytes(),
        text: &subject.text,
        pos: 0,
        nodes: Vec::new(),
        delims: Vec::new(),
        delim_top: NONE,
        brackets: Vec::new(),
        bracket_top: NONE,
        backticks: None,
    };
    let root = p.new_node(InlineKind::Text, 0..0);
    while p.pos < p.s.len() {
        p.parse_inline(root);
    }
    p.process_emphasis(NONE);
    if o.extended_autolinks {
        p.extended_autolinks(root);
    }
    p.collect(root, subject)
}

fn char_at(s: &str, pos: usize) -> Option<char> {
    s.get(pos..).and_then(|r| r.chars().next())
}

fn char_before(s: &str, pos: usize) -> Option<char> {
    s.get(..pos).and_then(|r| r.chars().next_back())
}

impl<'a> Parser<'a> {
    fn new_node(&mut self, kind: InlineKind, span: Span) -> usize {
        self.nodes.push(N {
            kind,
            span,
            parent: NONE,
            first: NONE,
            last: NONE,
            prev: NONE,
            next: NONE,
            literal: false,
        });
        self.nodes.len() - 1
    }

    fn append(&mut self, parent: usize, child: usize) {
        let last = self.nodes[parent].last;
        self.nodes[child].parent = parent;
        self.nodes[child].prev = last;
        self.nodes[child].next = NONE;
        if last == NONE {
            self.nodes[parent].first = child;
        } else {
            self.nodes[last].next = child;
        }
        self.nodes[parent].last = child;
    }

    fn unlink(&mut self, n: usize) {
        let (parent, prev, next) = (self.nodes[n].parent, self.nodes[n].prev, self.nodes[n].next);
        if prev != NONE {
            self.nodes[prev].next = next;
        } else if parent != NONE {
            self.nodes[parent].first = next;
        }
        if next != NONE {
            self.nodes[next].prev = prev;
        } else if parent != NONE {
            self.nodes[parent].last = prev;
        }
        self.nodes[n].parent = NONE;
        self.nodes[n].prev = NONE;
        self.nodes[n].next = NONE;
    }

    fn insert_after(&mut self, anchor: usize, n: usize) {
        let next = self.nodes[anchor].next;
        let parent = self.nodes[anchor].parent;
        self.nodes[n].parent = parent;
        self.nodes[n].prev = anchor;
        self.nodes[n].next = next;
        self.nodes[anchor].next = n;
        if next != NONE {
            self.nodes[next].prev = n;
        } else if parent != NONE {
            self.nodes[parent].last = n;
        }
    }

    fn text_node(&mut self, parent: usize, span: Span) -> usize {
        let n = self.new_node(InlineKind::Text, span);
        self.append(parent, n);
        n
    }

    fn peek(&self) -> Option<u8> {
        self.s.get(self.pos).copied()
    }

    fn parse_inline(&mut self, block: usize) {
        let c = self.s[self.pos];
        let handled = match c {
            b'\n' => {
                self.parse_newline(block);
                true
            }
            b'\\' if self.o.latex_delimiters && self.s.get(self.pos + 1) == Some(&b'(') => {
                self.parse_paren_math(block) || {
                    self.parse_backslash(block);
                    true
                }
            }
            b'\\' => {
                self.parse_backslash(block);
                true
            }
            b'`' => {
                self.parse_backticks(block);
                true
            }
            b'*' | b'_' => self.handle_delim(c, block),
            b'~' if self.o.strikethrough || self.o.subscript => self.handle_delim(c, block),
            b'=' if self.o.highlight && self.s.get(self.pos + 1) == Some(&b'=') => {
                self.handle_delim(c, block)
            }
            b'^' if self.o.superscript => self.handle_delim(c, block),
            b'[' => {
                let start = self.pos;
                self.pos += 1;
                let n = self.text_node(block, start..start + 1);
                self.nodes[n].literal = true;
                self.add_bracket(n, start, false);
                true
            }
            b'!' => {
                let start = self.pos;
                self.pos += 1;
                if self.peek() == Some(b'[') {
                    self.pos += 1;
                    let n = self.text_node(block, start..start + 2);
                    self.nodes[n].literal = true;
                    self.add_bracket(n, start + 1, true);
                } else {
                    self.text_node(block, start..start + 1);
                }
                true
            }
            b']' => {
                self.parse_close_bracket(block);
                true
            }
            b'<' => self.parse_autolink(block) || self.parse_html(block),
            b'&' => self.parse_entity(block),
            b'$' if self.o.inline_math => self.parse_math(block),
            b':' if self.o.emoji => self.parse_emoji(block),
            _ => false,
        };
        if !handled {
            self.parse_string(block);
        }
    }

    fn is_special(&self, c: u8) -> bool {
        match c {
            b'\n' | b'`' | b'[' | b']' | b'\\' | b'!' | b'<' | b'&' | b'*' | b'_' => true,
            b'~' => self.o.strikethrough || self.o.subscript,
            b'=' => self.o.highlight,
            b'^' => self.o.superscript,
            b'$' => self.o.inline_math,
            b':' => self.o.emoji,
            _ => false,
        }
    }

    fn parse_string(&mut self, block: usize) {
        let start = self.pos;
        self.pos += 1;
        while self.pos < self.s.len() && !self.is_special(self.s[self.pos]) {
            self.pos += 1;
        }
        // keep char boundaries
        while self.pos < self.s.len() && !self.text.is_char_boundary(self.pos) {
            self.pos += 1;
        }
        self.text_node(block, start..self.pos);
    }

    fn parse_newline(&mut self, block: usize) {
        let nl = self.pos;
        self.pos += 1;
        let last = self.nodes[block].last;
        let mut start = nl;
        let mut hard = false;
        if last != NONE
            && matches!(self.nodes[last].kind, InlineKind::Text)
            && !self.nodes[last].literal
        {
            let sp = self.nodes[last].span.clone();
            let lit = &self.s[sp.clone()];
            let spaces = lit.iter().rev().take_while(|&&b| b == b' ').count();
            if spaces > 0 {
                hard = spaces >= 2;
                self.nodes[last].span.end -= spaces;
                start = sp.end - spaces;
            }
        }
        // gobble leading spaces of the next line
        while self.peek() == Some(b' ') {
            self.pos += 1;
        }
        let kind = if hard {
            InlineKind::HardBreak
        } else {
            InlineKind::SoftBreak
        };
        let n = self.new_node(kind, start..nl + 1);
        self.append(block, n);
    }

    fn parse_backslash(&mut self, block: usize) {
        let start = self.pos;
        self.pos += 1;
        match self.peek() {
            Some(b'\n') => {
                self.pos += 1;
                let n = self.new_node(InlineKind::HardBreak, start..self.pos);
                self.append(block, n);
            }
            Some(c) if c.is_ascii_punctuation() => {
                self.pos += 1;
                let n = self.new_node(InlineKind::Escape, start..self.pos);
                self.append(block, n);
            }
            _ => {
                self.text_node(block, start..start + 1);
            }
        }
    }

    fn backtick_runs(&mut self) -> &HashMap<usize, Vec<usize>> {
        if self.backticks.is_none() {
            let mut map: HashMap<usize, Vec<usize>> = HashMap::new();
            let mut i = 0;
            while i < self.s.len() {
                if self.s[i] == b'`' {
                    let n = scan::run_len(&self.s[i..], b'`');
                    map.entry(n).or_default().push(i);
                    i += n;
                } else {
                    i += 1;
                }
            }
            self.backticks = Some(map);
        }
        self.backticks.as_ref().unwrap_or_else(|| unreachable!())
    }

    fn parse_backticks(&mut self, block: usize) {
        let start = self.pos;
        let n = scan::run_len(&self.s[start..], b'`');
        let after_open = start + n;
        let close = self
            .backtick_runs()
            .get(&n)
            .and_then(|v| v.iter().copied().find(|&p| p >= after_open));
        match close {
            Some(c) => {
                let raw = &self.text[after_open..c];
                let mut contents = raw.replace(['\n'], " ");
                if contents.len() > 1
                    && contents.starts_with(' ')
                    && contents.ends_with(' ')
                    && contents.bytes().any(|b| b != b' ')
                {
                    contents = contents[1..contents.len() - 1].to_owned();
                }
                self.pos = c + n;
                let node = self.new_node(
                    InlineKind::Code {
                        content: after_open..c,
                        text: contents,
                    },
                    start..self.pos,
                );
                self.append(block, node);
            }
            None => {
                self.pos = after_open;
                self.text_node(block, start..after_open);
            }
        }
    }

    fn is_cjk(c: char) -> bool {
        inkra_text::segment::is_cjk(c)
    }

    fn scan_delims(&self, cc: u8) -> (usize, bool, bool) {
        let start = self.pos;
        let n = scan::run_len(&self.s[start..], cc);
        let before = char_before(self.text, start).unwrap_or('\n');
        let after = char_at(self.text, start + n).unwrap_or('\n');
        let after_ws = is_unicode_whitespace(after);
        let before_ws = is_unicode_whitespace(before);
        let cjk = self.o.cjk_emphasis;
        let after_p = is_punctuation(after) || (cjk && Self::is_cjk(after));
        let before_p = is_punctuation(before) || (cjk && Self::is_cjk(before));
        let left = !after_ws && (!after_p || before_ws || before_p);
        let right = !before_ws && (!before_p || after_ws || after_p);
        let (can_open, can_close) = if cc == b'_' {
            (left && (!right || before_p), right && (!left || after_p))
        } else {
            (left, right)
        };
        (n, can_open, can_close)
    }

    fn handle_delim(&mut self, cc: u8, block: usize) -> bool {
        let (n, can_open, can_close) = self.scan_delims(cc);
        if n == 0 {
            return false;
        }
        let start = self.pos;
        self.pos += n;
        let node = self.text_node(block, start..self.pos);
        self.nodes[node].literal = true;
        let valid_len = match cc {
            b'~' => n <= 2,
            b'=' => n == 2,
            b'^' => n == 1,
            _ => true,
        };
        if (can_open || can_close) && valid_len {
            let d = Delim {
                cc,
                numdelims: n,
                origdelims: n,
                node,
                prev: self.delim_top,
                next: NONE,
                can_open,
                can_close,
            };
            self.delims.push(d);
            let idx = self.delims.len() - 1;
            if self.delim_top != NONE {
                self.delims[self.delim_top].next = idx;
            }
            self.delim_top = idx;
        }
        true
    }

    fn remove_delim(&mut self, d: usize) {
        let (prev, next) = (self.delims[d].prev, self.delims[d].next);
        if prev != NONE {
            self.delims[prev].next = next;
        }
        if next != NONE {
            self.delims[next].prev = prev;
        } else {
            self.delim_top = prev;
        }
    }

    fn process_emphasis(&mut self, stack_bottom: usize) {
        let mut openers_bottom: HashMap<(u8, bool, usize), usize> = HashMap::new();
        // first closer above stack_bottom
        let mut closer = self.delim_top;
        while closer != NONE && self.delims[closer].prev != stack_bottom {
            closer = self.delims[closer].prev;
        }
        while closer != NONE {
            let cd = self.delims[closer].clone();
            if !cd.can_close {
                closer = cd.next;
                continue;
            }
            let key = match cd.cc {
                b'*' | b'_' => (cd.cc, cd.can_open, cd.origdelims % 3),
                _ => (cd.cc, false, cd.origdelims),
            };
            let bottom = *openers_bottom.get(&key).unwrap_or(&stack_bottom);
            let mut opener = cd.prev;
            let mut found = false;
            while opener != NONE && opener != stack_bottom && opener != bottom {
                let od = &self.delims[opener];
                if od.cc == cd.cc && od.can_open {
                    let ok = match cd.cc {
                        b'*' | b'_' => {
                            let odd = (cd.can_open || od.can_close)
                                && !cd.origdelims.is_multiple_of(3)
                                && (od.origdelims + cd.origdelims).is_multiple_of(3);
                            !odd
                        }
                        b'~' => {
                            od.numdelims == cd.numdelims
                                && (self.o.strikethrough
                                    && (od.numdelims == 2 || !self.o.subscript)
                                    || self.o.subscript
                                        && od.numdelims == 1
                                        && self.no_space_between(opener, closer))
                        }
                        b'^' => {
                            od.numdelims == 1
                                && cd.numdelims == 1
                                && self.no_space_between(opener, closer)
                        }
                        b'=' => od.numdelims == 2 && cd.numdelims == 2,
                        _ => false,
                    };
                    if ok {
                        found = true;
                        break;
                    }
                }
                opener = self.delims[opener].prev;
            }
            let old_closer = closer;
            if found {
                let od = self.delims[opener].clone();
                let use_delims = match cd.cc {
                    b'*' | b'_' => {
                        if cd.numdelims >= 2 && od.numdelims >= 2 {
                            2
                        } else {
                            1
                        }
                    }
                    _ => cd.numdelims,
                };
                let o_inl = od.node;
                let c_inl = cd.node;
                self.delims[opener].numdelims -= use_delims;
                self.delims[closer].numdelims -= use_delims;
                let osp = self.nodes[o_inl].span.clone();
                let csp = self.nodes[c_inl].span.clone();
                let open = osp.end - use_delims..osp.end;
                let close = csp.start..csp.start + use_delims;
                self.nodes[o_inl].span.end -= use_delims;
                self.nodes[c_inl].span.start += use_delims;
                let kind = match (cd.cc, use_delims) {
                    (b'*' | b'_', 1) => InlineKind::Emph {
                        open: open.clone(),
                        close: close.clone(),
                    },
                    (b'*' | b'_', _) => InlineKind::Strong {
                        open: open.clone(),
                        close: close.clone(),
                    },
                    (b'~', 1) if self.o.subscript => InlineKind::Subscript {
                        open: open.clone(),
                        close: close.clone(),
                    },
                    (b'~', _) => InlineKind::Strike {
                        open: open.clone(),
                        close: close.clone(),
                    },
                    (b'^', _) => InlineKind::Superscript {
                        open: open.clone(),
                        close: close.clone(),
                    },
                    _ => InlineKind::Highlight {
                        open: open.clone(),
                        close: close.clone(),
                    },
                };
                let emph = self.new_node(kind, open.start..close.end);
                let mut tmp = self.nodes[o_inl].next;
                while tmp != NONE && tmp != c_inl {
                    let next = self.nodes[tmp].next;
                    self.unlink(tmp);
                    self.append(emph, tmp);
                    tmp = next;
                }
                self.insert_after(o_inl, emph);
                // remove delimiters between opener and closer
                let mut d = cd.prev;
                while d != NONE && d != opener {
                    let p = self.delims[d].prev;
                    self.remove_delim(d);
                    d = p;
                }
                self.delims[opener].next = closer;
                self.delims[closer].prev = opener;
                if self.delims[opener].numdelims == 0 {
                    self.unlink(o_inl);
                    self.remove_delim(opener);
                }
                if self.delims[closer].numdelims == 0 {
                    self.unlink(c_inl);
                    let next = self.delims[closer].next;
                    self.remove_delim(closer);
                    closer = next;
                }
            } else {
                closer = cd.next;
                openers_bottom.insert(key, self.delims[old_closer].prev);
                if !cd.can_open {
                    self.remove_delim(old_closer);
                }
            }
        }
        while self.delim_top != NONE && self.delim_top != stack_bottom {
            let t = self.delim_top;
            self.remove_delim(t);
        }
    }

    fn no_space_between(&self, opener: usize, closer: usize) -> bool {
        let a = self.nodes[self.delims[opener].node].span.end;
        let b = self.nodes[self.delims[closer].node].span.start;
        a < b && !has_unescaped_whitespace(&self.text[a..b])
    }

    fn add_bracket(&mut self, node: usize, index: usize, image: bool) {
        if self.bracket_top != NONE {
            self.brackets[self.bracket_top].bracket_after = true;
        }
        self.brackets.push(Bracket {
            node,
            prev: self.bracket_top,
            prev_delim: self.delim_top,
            index,
            image,
            active: true,
            bracket_after: false,
        });
        self.bracket_top = self.brackets.len() - 1;
    }

    fn remove_bracket(&mut self) {
        if self.bracket_top != NONE {
            self.bracket_top = self.brackets[self.bracket_top].prev;
        }
    }

    fn spnl(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t')) {
            self.pos += 1;
        }
        if self.peek() == Some(b'\n') {
            self.pos += 1;
        }
        while matches!(self.peek(), Some(b' ' | b'\t')) {
            self.pos += 1;
        }
    }

    fn parse_close_bracket(&mut self, block: usize) {
        let close_pos = self.pos;
        self.pos += 1;
        let startpos = self.pos;
        if self.bracket_top == NONE {
            self.text_node(block, close_pos..startpos);
            return;
        }
        let ob = self.brackets[self.bracket_top].clone();
        if !ob.active {
            self.text_node(block, close_pos..startpos);
            self.remove_bracket();
            return;
        }
        let is_image = ob.image;
        let savepos = self.pos;
        let mut matched: Option<(String, Option<String>, LinkSyntax)> = None;

        // Inline link
        if self.peek() == Some(b'(') {
            self.pos += 1;
            self.spnl();
            let dest_start = self.pos;
            let dest = if self.peek() == Some(b')') {
                Some((0, String::new()))
            } else {
                refs::scan_destination(&self.s[self.pos..])
                    .map(|(len, raw)| (len, unescape(std::str::from_utf8(raw).unwrap_or_default())))
            };
            if let Some((dlen, dest)) = dest {
                self.pos += dlen;
                let dest_span = dest_start..self.pos;
                let before_title = self.pos;
                self.spnl();
                let mut title = None;
                if self.pos > before_title
                    && let Some((tlen, raw)) = refs::scan_title(&self.s[self.pos..])
                {
                    title = Some(unescape(std::str::from_utf8(raw).unwrap_or_default()));
                    self.pos += tlen;
                    self.spnl();
                }
                if self.peek() == Some(b')') {
                    self.pos += 1;
                    matched = Some((dest, title, LinkSyntax::Inline { dest: dest_span }));
                }
            }
            if matched.is_none() {
                self.pos = savepos;
            }
        }

        // Footnote reference [^label]
        if matched.is_none()
            && !is_image
            && self.o.footnotes
            && self.s.get(ob.index + 1) == Some(&b'^')
            && self.peek() != Some(b'[')
        {
            let label = normalize_label(&self.text[ob.index + 2..close_pos]);
            if !label.is_empty() && self.cx.footnotes.contains(&label) {
                let node = self.new_node(InlineKind::FootnoteRef { label }, ob.index..startpos);
                // drop everything after the opener
                let mut tmp = self.nodes[ob.node].next;
                while tmp != NONE {
                    let next = self.nodes[tmp].next;
                    self.unlink(tmp);
                    tmp = next;
                }
                self.append(block, node);
                self.process_emphasis(ob.prev_delim);
                self.remove_bracket();
                self.unlink(ob.node);
                return;
            }
        }

        if matched.is_none() {
            let before_label = self.pos;
            let n = if self.s[self.pos..].starts_with(b"[]") {
                2
            } else {
                refs::scan_label(&self.s[self.pos..]).unwrap_or(0)
            };
            let mut syntax = LinkSyntax::Shortcut;
            let reflabel = if n > 2 {
                syntax = LinkSyntax::Reference {
                    label: normalize_label(&self.text[before_label + 1..before_label + n - 1]),
                };
                Some(self.text[before_label + 1..before_label + n - 1].to_owned())
            } else if !ob.bracket_after {
                if n == 2 {
                    syntax = LinkSyntax::Collapsed;
                }
                Some(self.text[ob.index + 1..close_pos].to_owned())
            } else {
                None
            };
            if n == 0 {
                self.pos = savepos;
            } else {
                self.pos = before_label + n;
            }
            let mut found = false;
            if let Some(label) = reflabel
                && let Some(t) = self.cx.refs.get(&normalize_label(&label))
            {
                matched = Some((t.dest.clone(), t.title.clone(), syntax));
                found = true;
            }
            if !found {
                self.pos = savepos;
            }
        }

        match matched {
            Some((dest, title, syntax)) => {
                let text_span = ob.index + 1..close_pos;
                let start = if is_image { ob.index - 1 } else { ob.index };
                let kind = if is_image {
                    InlineKind::Image {
                        dest,
                        title,
                        text: text_span,
                        syntax,
                    }
                } else {
                    InlineKind::Link {
                        dest,
                        title,
                        text: text_span,
                        syntax,
                    }
                };
                let node = self.new_node(kind, start..self.pos);
                let mut tmp = self.nodes[ob.node].next;
                while tmp != NONE {
                    let next = self.nodes[tmp].next;
                    self.unlink(tmp);
                    self.append(node, tmp);
                    tmp = next;
                }
                self.append(block, node);
                self.process_emphasis(ob.prev_delim);
                self.remove_bracket();
                self.unlink(ob.node);
                if !is_image {
                    let mut b = self.bracket_top;
                    while b != NONE {
                        if !self.brackets[b].image {
                            self.brackets[b].active = false;
                        }
                        b = self.brackets[b].prev;
                    }
                }
            }
            None => {
                self.remove_bracket();
                self.pos = startpos;
                self.text_node(block, close_pos..startpos);
            }
        }
    }

    fn parse_autolink(&mut self, block: usize) -> bool {
        let rest = &self.s[self.pos..];
        let start = self.pos;
        // URI autolink
        if rest.len() > 2 && rest[1].is_ascii_alphabetic() {
            let scheme_len = rest[1..]
                .iter()
                .take_while(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'.' | b'-'))
                .count();
            if (2..=32).contains(&scheme_len) && rest.get(1 + scheme_len) == Some(&b':') {
                let body_start = 2 + scheme_len;
                let body = rest[body_start..]
                    .iter()
                    .take_while(|&&b| b > b' ' && b != b'<' && b != b'>')
                    .count();
                if rest.get(body_start + body) == Some(&b'>') {
                    let end = start + body_start + body + 1;
                    let dest = self.text[start + 1..end - 1].to_owned();
                    let node = self.new_node(
                        InlineKind::Link {
                            dest,
                            title: None,
                            text: start + 1..end - 1,
                            syntax: LinkSyntax::Autolink,
                        },
                        start..end,
                    );
                    let t = self.new_node(InlineKind::Text, start + 1..end - 1);
                    self.append(node, t);
                    self.append(block, node);
                    self.pos = end;
                    return true;
                }
            }
        }
        // e-mail autolink
        if let Some(len) = email_autolink(&rest[1..])
            && rest.get(1 + len) == Some(&b'>')
        {
            let end = start + len + 2;
            let addr = self.text[start + 1..end - 1].to_owned();
            let node = self.new_node(
                InlineKind::Link {
                    dest: format!("mailto:{addr}"),
                    title: None,
                    text: start + 1..end - 1,
                    syntax: LinkSyntax::Autolink,
                },
                start..end,
            );
            let t = self.new_node(InlineKind::Text, start + 1..end - 1);
            self.append(node, t);
            self.append(block, node);
            self.pos = end;
            return true;
        }
        false
    }

    fn parse_html(&mut self, block: usize) -> bool {
        match scan::inline_html(&self.s[self.pos..]) {
            Some(len) => {
                let n = self.new_node(InlineKind::Html, self.pos..self.pos + len);
                self.append(block, n);
                self.pos += len;
                true
            }
            None => false,
        }
    }

    fn parse_entity(&mut self, block: usize) -> bool {
        match scan_entity(&self.s[self.pos..]) {
            Some((len, text)) => {
                let n = self.new_node(InlineKind::Entity { text }, self.pos..self.pos + len);
                self.append(block, n);
                self.pos += len;
                true
            }
            None => false,
        }
    }

    fn parse_math(&mut self, block: usize) -> bool {
        let start = self.pos;
        let s = self.s;
        let double = s.get(start + 1) == Some(&b'$');
        let open = if double { 2 } else { 1 };
        let body = start + open;
        let loose = self.o.loose_inline_math;
        match s.get(body) {
            None => return false,
            Some(b) if !loose && b.is_ascii_whitespace() && !double => return false,
            Some(b'$') if !double => return false,
            _ => {}
        }
        let mut i = body;
        while i < s.len() {
            match s[i] {
                b'\\' => i += 2,
                b'$' => {
                    if double {
                        if s.get(i + 1) == Some(&b'$') && i > body {
                            let n =
                                self.new_node(InlineKind::Math { content: body..i }, start..i + 2);
                            self.append(block, n);
                            self.pos = i + 2;
                            return true;
                        }
                        i += 1;
                        continue;
                    }
                    let prev_ws = s[i - 1].is_ascii_whitespace();
                    let next_digit = s.get(i + 1).is_some_and(u8::is_ascii_digit);
                    if i > body && (loose || !prev_ws) && !next_digit {
                        let n = self.new_node(InlineKind::Math { content: body..i }, start..i + 1);
                        self.append(block, n);
                        self.pos = i + 1;
                        return true;
                    }
                    i += 1;
                }
                _ => i += 1,
            }
        }
        false
    }

    /// `\( … \)` inline math (MD-035, `latex_delimiters`).
    fn parse_paren_math(&mut self, block: usize) -> bool {
        let start = self.pos;
        let s = self.s;
        let body = start + 2;
        let mut i = body;
        while i + 1 < s.len() {
            if s[i] == b'\\' {
                if s[i + 1] == b')' && i > body {
                    let n = self.new_node(InlineKind::Math { content: body..i }, start..i + 2);
                    self.append(block, n);
                    self.pos = i + 2;
                    return true;
                }
                i += 2;
                continue;
            }
            if s[i] == b'\n' && s.get(i + 1) == Some(&b'\n') {
                break;
            }
            i += 1;
        }
        false
    }

    fn parse_emoji(&mut self, block: usize) -> bool {
        let start = self.pos;
        let rest = &self.s[start + 1..];
        let n = rest
            .iter()
            .take_while(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'+' | b'-'))
            .count();
        if n == 0 || rest.get(n) != Some(&b':') {
            return false;
        }
        let name = &self.text[start + 1..start + 1 + n];
        match crate::emoji::lookup(name) {
            Some(emoji) => {
                let node = self.new_node(InlineKind::Emoji { emoji }, start..start + n + 2);
                self.append(block, node);
                self.pos = start + n + 2;
                true
            }
            None => false,
        }
    }

    /// GFM extended autolinks: rewrites matching parts of text nodes into links.
    fn extended_autolinks(&mut self, parent: usize) {
        let mut child = self.nodes[parent].first;
        while child != NONE {
            let next = self.nodes[child].next;
            match self.nodes[child].kind {
                InlineKind::Link { .. } | InlineKind::Image { .. } | InlineKind::Code { .. } => {}
                InlineKind::Text => {
                    // merge following contiguous text nodes first
                    let mut nx = next;
                    while nx != NONE
                        && matches!(self.nodes[nx].kind, InlineKind::Text)
                        && self.nodes[nx].span.start == self.nodes[child].span.end
                    {
                        let after = self.nodes[nx].next;
                        self.nodes[child].span.end = self.nodes[nx].span.end;
                        self.nodes[child].literal = false;
                        self.unlink(nx);
                        nx = after;
                    }
                    self.autolink_text(child);
                    child = nx;
                    continue;
                }
                _ => self.extended_autolinks(child),
            }
            child = next;
        }
    }

    fn autolink_text(&mut self, node: usize) {
        let span = self.nodes[node].span.clone();
        let s = self.s;
        let mut i = span.start;
        let mut cur = node;
        while i < span.end {
            let at_boundary =
                i == 0 || matches!(s[i - 1], b' ' | b'\t' | b'\n' | b'*' | b'_' | b'~' | b'(');
            let floor = self.nodes[cur].span.start;
            let found = if at_boundary && s[i..span.end].starts_with(b"www.") {
                www_link(&s[i..span.end])
                    .map(|len| (i, len, format!("http://{}", &self.text[i..i + len])))
            } else if at_boundary
                && (s[i..span.end].starts_with(b"http://")
                    || s[i..span.end].starts_with(b"https://")
                    || s[i..span.end].starts_with(b"ftp://"))
            {
                url_link(&s[i..span.end]).map(|len| (i, len, self.text[i..i + len].to_owned()))
            } else if s[i] == b'@' {
                email_link(s, floor, i, span.end)
                    .map(|(st, len)| (st, len, format!("mailto:{}", &self.text[st..st + len])))
            } else {
                None
            };
            if let Some((link_start, len, dest)) = found {
                let link_end = link_start + len;
                // split: [cur.start, link_start) text, link, [link_end, end) text
                let tail = self.new_node(InlineKind::Text, link_end..self.nodes[cur].span.end);
                self.nodes[cur].span.end = link_start;
                let link = self.new_node(
                    InlineKind::Link {
                        dest,
                        title: None,
                        text: link_start..link_end,
                        syntax: LinkSyntax::Extended,
                    },
                    link_start..link_end,
                );
                let inner = self.new_node(InlineKind::Text, link_start..link_end);
                self.append(link, inner);
                self.insert_after(cur, link);
                self.insert_after(link, tail);
                cur = tail;
                i = link_end;
            } else {
                i += 1;
            }
        }
    }

    fn collect(&self, parent: usize, subject: &Subject) -> Vec<Inline> {
        let mut out: Vec<Inline> = Vec::new();
        let mut c = self.nodes[parent].first;
        while c != NONE {
            let idx = c;
            let n = &self.nodes[idx];
            c = n.next;
            if matches!(n.kind, InlineKind::Text) && n.span.is_empty() {
                continue;
            }
            if matches!(n.kind, InlineKind::Text) {
                for piece in subject.split(&n.span) {
                    let span = subject.map_span(&piece);
                    if let Some(last) = out.last_mut()
                        && matches!(last.kind, InlineKind::Text)
                        && last.span.end == span.start
                    {
                        last.span.end = span.end;
                        continue;
                    }
                    out.push(Inline::leaf(InlineKind::Text, span));
                }
                continue;
            }
            let span = subject.map_span(&n.span);
            let kind = map_kind(&n.kind, subject);
            let children = self.collect(idx, subject);
            out.push(Inline {
                kind,
                span,
                children,
            });
        }
        out
    }
}

/// Whether `s` contains whitespace that is not backslash-escaped.
fn has_unescaped_whitespace(s: &str) -> bool {
    // true when the text contains whitespace that is not escaped
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\\' {
            i += 2;
            continue;
        }
        if b[i].is_ascii_whitespace() {
            return true;
        }
        i += 1;
    }
    false
}

fn map_kind(k: &InlineKind, s: &Subject) -> InlineKind {
    let m = |r: &Span| s.map_span(r);
    match k {
        InlineKind::Code { content, text } => InlineKind::Code {
            content: m(content),
            text: text.clone(),
        },
        InlineKind::Math { content } => InlineKind::Math {
            content: m(content),
        },
        InlineKind::Emph { open, close } => InlineKind::Emph {
            open: m(open),
            close: m(close),
        },
        InlineKind::Strong { open, close } => InlineKind::Strong {
            open: m(open),
            close: m(close),
        },
        InlineKind::Strike { open, close } => InlineKind::Strike {
            open: m(open),
            close: m(close),
        },
        InlineKind::Highlight { open, close } => InlineKind::Highlight {
            open: m(open),
            close: m(close),
        },
        InlineKind::Superscript { open, close } => InlineKind::Superscript {
            open: m(open),
            close: m(close),
        },
        InlineKind::Subscript { open, close } => InlineKind::Subscript {
            open: m(open),
            close: m(close),
        },
        InlineKind::Link {
            dest,
            title,
            text,
            syntax,
        } => InlineKind::Link {
            dest: dest.clone(),
            title: title.clone(),
            text: m(text),
            syntax: map_syntax(syntax, s),
        },
        InlineKind::Image {
            dest,
            title,
            text,
            syntax,
        } => InlineKind::Image {
            dest: dest.clone(),
            title: title.clone(),
            text: m(text),
            syntax: map_syntax(syntax, s),
        },
        other => other.clone(),
    }
}

fn map_syntax(syn: &LinkSyntax, s: &Subject) -> LinkSyntax {
    match syn {
        LinkSyntax::Inline { dest } => LinkSyntax::Inline {
            dest: s.map_span(dest),
        },
        other => other.clone(),
    }
}

fn email_autolink(s: &[u8]) -> Option<usize> {
    let local = s
        .iter()
        .take_while(|&&b| b.is_ascii_alphanumeric() || b".!#$%&'*+/=?^_`{|}~-".contains(&b))
        .count();
    if local == 0 || s.get(local) != Some(&b'@') {
        return None;
    }
    let mut i = local + 1;
    loop {
        let label_start = i;
        let n = s[i..]
            .iter()
            .take_while(|b| b.is_ascii_alphanumeric() || **b == b'-')
            .count();
        if n == 0 || n > 63 || s[label_start] == b'-' || s[label_start + n - 1] == b'-' {
            return None;
        }
        i += n;
        if s.get(i) == Some(&b'.') && s.get(i + 1).is_some_and(u8::is_ascii_alphanumeric) {
            i += 1;
            continue;
        }
        return Some(i);
    }
}

fn trim_autolink_end(s: &[u8], mut len: usize) -> usize {
    loop {
        if len == 0 {
            return 0;
        }
        let c = s[len - 1];
        if matches!(
            c,
            b'?' | b'!' | b'.' | b',' | b':' | b'*' | b'_' | b'~' | b'\'' | b'"'
        ) {
            len -= 1;
            continue;
        }
        if c == b')' {
            let opens = s[..len].iter().filter(|&&b| b == b'(').count();
            let closes = s[..len].iter().filter(|&&b| b == b')').count();
            if closes > opens {
                len -= 1;
                continue;
            }
        }
        if c == b';' {
            // trailing entity-like reference
            let amp = s[..len].iter().rposition(|&b| b == b'&');
            if let Some(a) = amp
                && s[a + 1..len - 1].iter().all(u8::is_ascii_alphanumeric)
                && a + 1 < len - 1
            {
                len = a;
                continue;
            }
        }
        return len;
    }
}

fn domain_len(s: &[u8]) -> Option<usize> {
    let mut i = 0;
    let mut segments = Vec::new();
    let mut seg_start = 0;
    while i < s.len() {
        let b = s[i];
        if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b >= 0x80 {
            i += 1;
        } else if b == b'.'
            && i > seg_start
            && s.get(i + 1).is_some_and(|c| {
                c.is_ascii_alphanumeric() || *c == b'-' || *c == b'_' || *c >= 0x80
            })
        {
            segments.push(seg_start..i);
            i += 1;
            seg_start = i;
        } else {
            break;
        }
    }
    if i == seg_start {
        return None;
    }
    segments.push(seg_start..i);
    if segments.len() < 2 && !s.starts_with(b"localhost") {
        // at least one period
        return None;
    }
    for seg in segments.iter().rev().take(2) {
        if s[seg.clone()].contains(&b'_') {
            return None;
        }
    }
    Some(i)
}

fn www_link(s: &[u8]) -> Option<usize> {
    let d = domain_len(s)?;
    let path = s[d..]
        .iter()
        .take_while(|&&b| !b.is_ascii_whitespace() && b != b'<')
        .count();
    Some(trim_autolink_end(s, d + path))
}

fn url_link(s: &[u8]) -> Option<usize> {
    let scheme = s.iter().position(|&b| b == b':').map_or(7, |p| p + 3);
    let d = domain_len(&s[scheme..])?;
    let path = s[scheme + d..]
        .iter()
        .take_while(|&&b| !b.is_ascii_whitespace() && b != b'<')
        .count();
    Some(trim_autolink_end(s, scheme + d + path))
}

fn email_local_len(s: &[u8], floor: usize, at: usize) -> usize {
    s[floor..at]
        .iter()
        .rev()
        .take_while(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'+' | b'_' | b'-'))
        .count()
}

/// E-mail extended autolink around the `@` at `at`; returns `(start, length)`.
fn email_link(s: &[u8], floor: usize, at: usize, end: usize) -> Option<(usize, usize)> {
    let local = email_local_len(s, floor, at);
    if local == 0 {
        return None;
    }
    let start = at - local;
    let mut i = at + 1;
    let mut dots = 0;
    while i < end && (s[i].is_ascii_alphanumeric() || matches!(s[i], b'-' | b'_' | b'.')) {
        if s[i] == b'.' {
            dots += 1;
        }
        i += 1;
    }
    // trailing '.' is not part of the address
    while i > at + 1 && s[i - 1] == b'.' {
        i -= 1;
        dots -= 1;
    }
    if dots == 0 || matches!(s[i - 1], b'-' | b'_') {
        return None;
    }
    Some((start, i - start))
}
