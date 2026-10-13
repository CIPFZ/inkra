//! Line-fed block parser (CommonMark 0.31 block structure + GFM tables and Inkra
//! extensions), producing blocks with exact source ranges.
//!
//! The parser is fed one line at a time (with its terminator) so the incremental
//! driver can stop as soon as the new parse re-synchronizes with the old one.

use crate::ast::{AlertKind, Align, Block, BlockKind, Fence, Line, ListKind, Span, Task};
use crate::options::ParseOptions;
use crate::refs::{normalize_label, parse_ref_def};
use crate::scan::{self, is_space_or_tab};

const CODE_INDENT: usize = 4;

#[derive(Debug, Clone)]
struct Row {
    span: Span,
    cells: Vec<Span>,
}

#[derive(Debug, Clone)]
struct FenceState {
    ch: u8,
    len: usize,
    indent: usize,
    open: Span,
    info: Option<Span>,
}

#[derive(Debug, Clone)]
enum NKind {
    Document,
    BlockQuote {
        alert: Option<(AlertKind, Span)>,
    },
    List {
        kind: ListKind,
        start: u64,
        tight: bool,
    },
    Item {
        number: Option<u64>,
        marker: Span,
        marker_offset: usize,
        padding: usize,
        task: Option<Task>,
    },
    FootnoteDef {
        label: String,
        marker: Span,
    },
    Paragraph,
    Heading {
        level: u8,
        setext: bool,
        marker: Span,
        closing: Option<Span>,
    },
    ThematicBreak,
    Code {
        fence: Option<FenceState>,
        close: Option<Span>,
    },
    Html {
        kind: u8,
    },
    Table {
        aligns: Vec<Align>,
        delimiter: Span,
        rows: Vec<Row>,
    },
    Math {
        open: Span,
        close: Option<Span>,
        /// Opened with `\[` (closes with `\]`).
        bracket: bool,
    },
    FrontMatter {
        open: Span,
        close: Option<Span>,
    },
    LinkDef {
        label: String,
        dest: String,
        title: Option<String>,
    },
    Removed,
}

impl NKind {
    fn can_contain(&self, child: &NKind) -> bool {
        match self {
            Self::Document
            | Self::BlockQuote { .. }
            | Self::Item { .. }
            | Self::FootnoteDef { .. } => !matches!(child, Self::Item { .. }),
            Self::List { .. } => matches!(child, Self::Item { .. }),
            _ => false,
        }
    }
    fn accepts_lines(&self) -> bool {
        matches!(
            self,
            Self::Paragraph
                | Self::Code { .. }
                | Self::Html { .. }
                | Self::Table { .. }
                | Self::Math { .. }
                | Self::FrontMatter { .. }
        )
    }
}

#[derive(Debug, Clone)]
struct Node {
    kind: NKind,
    parent: usize,
    children: Vec<usize>,
    open: bool,
    /// Offset of the block's first character.
    start: usize,
    start_line: usize,
    end: usize,
    end_line: usize,
    lines: Vec<Line>,
    /// Paragraph text (lines joined with `\n`), for link reference definitions.
    text: String,
    /// Byte length of each paragraph line in `text` (without `\n`).
    text_lens: Vec<usize>,
    /// Per content line: whether it was blank (indented code trimming).
    blank_lines: Vec<bool>,
    /// The block's first line began with only the document open (a valid restart
    /// point for incremental parsing). False for blocks split out of a paragraph.
    fresh: bool,
}

enum Cont {
    Matched,
    NotMatched,
    LineDone,
}

enum Start {
    None,
    Container,
    Leaf,
}

/// The block parser. Feed lines with [`BlockParser::feed`], then call
/// [`BlockParser::finish`].
pub struct BlockParser<'o> {
    o: &'o ParseOptions,
    nodes: Vec<Node>,
    tip: usize,
    oldtip: usize,
    all_closed: bool,
    last_matched: usize,
    cur: String,
    line_start: usize,
    line_no: usize,
    offset: usize,
    column: usize,
    next_nonspace: usize,
    next_nonspace_column: usize,
    indent: usize,
    indented: bool,
    blank: bool,
    partially_consumed_tab: bool,
    prev_line_end: usize,
    cur_line_end: usize,
    front_matter_close: Option<usize>,
    new_top: Option<usize>,
}

impl<'o> BlockParser<'o> {
    /// Starts parsing at document offset `base` (which must be a line start where
    /// only the document container is open). `front_matter_close` is the relative
    /// line number of the closing front matter fence when `base == 0` and the
    /// document starts with front matter.
    pub fn new(o: &'o ParseOptions, base: usize, front_matter_close: Option<usize>) -> Self {
        let doc = Node {
            kind: NKind::Document,
            parent: 0,
            children: Vec::new(),
            open: true,
            start: base,
            start_line: 0,
            end: base,
            end_line: 0,
            lines: Vec::new(),
            text: String::new(),
            text_lens: Vec::new(),
            blank_lines: Vec::new(),
            fresh: true,
        };
        Self {
            o,
            nodes: vec![doc],
            tip: 0,
            oldtip: 0,
            all_closed: true,
            last_matched: 0,
            cur: String::new(),
            line_start: base,
            line_no: 0,
            offset: 0,
            column: 0,
            next_nonspace: 0,
            next_nonspace_column: 0,
            indent: 0,
            indented: false,
            blank: false,
            partially_consumed_tab: false,
            prev_line_end: base,
            cur_line_end: base,
            front_matter_close,
            new_top: None,
        }
    }

    /// If the last fed line started a new *fresh* top-level block (one whose parse
    /// does not depend on earlier lines; a table split off a paragraph does), the
    /// offset of the start of that block's first line.
    pub fn new_top_level(&self) -> Option<usize> {
        self.new_top
            .filter(|&i| self.nodes[i].fresh)
            .map(|i| self.nodes[i].start)
    }

    /// Discards the top-level block started by the last fed line and finishes. Used
    /// by the incremental driver once the parse has re-synchronized.
    pub fn finish_before_new_top_level(mut self) -> (Vec<Block>, Vec<bool>) {
        if let Some(i) = self.new_top.take() {
            self.nodes[0].children.retain(|&c| c != i);
            self.nodes[i].kind = NKind::Removed;
            self.tip = 0;
        }
        self.output()
    }

    fn b(&self) -> &[u8] {
        self.cur.as_bytes()
    }

    fn peek(&self, i: usize) -> Option<u8> {
        self.cur.as_bytes().get(i).copied()
    }

    /// Feeds one line, including its terminator (`\n`, `\r\n`, `\r`, or none for
    /// the last line).
    pub fn feed(&mut self, line: &str) {
        let content_len = line.trim_end_matches(['\n', '\r']).len();
        self.cur.clear();
        self.cur.push_str(&line[..content_len]);
        self.prev_line_end = self.cur_line_end;
        self.cur_line_end = self.line_start + content_len;
        let before = self.nodes[0].children.last().copied();
        self.incorporate_line();
        let after = self.nodes[0].children.last().copied();
        self.new_top = if after != before { after } else { None };
        self.line_start += line.len();
        self.line_no += 1;
    }

    fn find_next_nonspace(&mut self) {
        let b = self.cur.as_bytes();
        let mut i = self.offset;
        let mut cols = self.column;
        while i < b.len() {
            match b[i] {
                b' ' => {
                    i += 1;
                    cols += 1;
                }
                b'\t' => {
                    i += 1;
                    cols += 4 - cols % 4;
                }
                _ => break,
            }
        }
        self.blank = i >= b.len();
        self.next_nonspace = i;
        self.next_nonspace_column = cols;
        self.indent = cols - self.column;
        self.indented = self.indent >= CODE_INDENT;
    }

    fn advance_next_nonspace(&mut self) {
        self.offset = self.next_nonspace;
        self.column = self.next_nonspace_column;
        self.partially_consumed_tab = false;
    }

    fn advance_offset(&mut self, mut count: usize, columns: bool) {
        while count > 0 {
            let Some(c) = self.peek(self.offset) else {
                break;
            };
            if c == b'\t' {
                let to_tab = 4 - self.column % 4;
                if columns {
                    self.partially_consumed_tab = to_tab > count;
                    let adv = to_tab.min(count);
                    self.column += adv;
                    if !self.partially_consumed_tab {
                        self.offset += 1;
                    }
                    count -= adv;
                } else {
                    self.partially_consumed_tab = false;
                    self.column += to_tab;
                    self.offset += 1;
                    count -= 1;
                }
            } else {
                self.partially_consumed_tab = false;
                // Advance one char (block prefixes are ASCII; be safe with UTF-8).
                let ch_len = self.cur[self.offset..]
                    .chars()
                    .next()
                    .map_or(1, char::len_utf8);
                self.offset += ch_len;
                self.column += 1;
                count -= 1;
            }
        }
    }

    fn incorporate_line(&mut self) {
        let mut container = 0;
        self.oldtip = self.tip;
        self.offset = 0;
        self.column = 0;
        self.blank = false;
        self.partially_consumed_tab = false;

        // Front matter occupies the very first lines of the document.
        if self.line_no == 0 && self.front_matter_close.is_some() && self.nodes[0].start == 0 {
            let span = self.line_start..self.cur_line_end;
            self.add_child(
                NKind::FrontMatter {
                    open: span,
                    close: None,
                },
                0,
            );
            return;
        }

        while let Some(&last) = self.nodes[container].children.last() {
            if !self.nodes[last].open {
                break;
            }
            container = last;
            self.find_next_nonspace();
            match self.continue_block(container) {
                Cont::Matched => {}
                Cont::NotMatched => {
                    container = self.nodes[container].parent;
                    break;
                }
                Cont::LineDone => return,
            }
        }

        self.all_closed = container == self.oldtip;
        self.last_matched = container;

        let k = &self.nodes[container].kind;
        let mut matched_leaf =
            !matches!(k, NKind::Paragraph | NKind::Table { .. }) && k.accepts_lines();
        while !matched_leaf {
            self.find_next_nonspace();
            if !self.indented && !self.maybe_special() {
                self.advance_next_nonspace();
                break;
            }
            match self.try_starts(container) {
                Start::Container => container = self.tip,
                Start::Leaf => {
                    container = self.tip;
                    matched_leaf = true;
                }
                Start::None => {
                    self.advance_next_nonspace();
                    break;
                }
            }
        }

        if !self.all_closed && !self.blank && matches!(self.nodes[self.tip].kind, NKind::Paragraph)
        {
            // Lazy paragraph continuation.
            self.add_line();
        } else {
            self.close_unmatched_blocks();
            let kind = self.nodes[container].kind.clone();
            if kind.accepts_lines() {
                self.add_line();
                if let NKind::Html { kind: t @ 1..=5 } = kind
                    && scan::html_block_end(t, &self.b()[self.offset..])
                {
                    self.finalize(container, self.line_no);
                }
            } else if self.offset < self.cur.len() && !self.blank {
                self.find_next_nonspace();
                let start = self.next_nonspace;
                self.add_child(NKind::Paragraph, start);
                self.advance_next_nonspace();
                self.add_line();
            }
        }
    }

    fn maybe_special(&self) -> bool {
        matches!(
            self.peek(self.next_nonspace),
            Some(
                b'#' | b'`'
                    | b'~'
                    | b'*'
                    | b'+'
                    | b'_'
                    | b'='
                    | b'<'
                    | b'>'
                    | b'-'
                    | b'|'
                    | b':'
                    | b'$'
                    | b'['
                    | b'0'..=b'9'
            )
        ) || (self.o.latex_delimiters && self.peek(self.next_nonspace) == Some(b'\\'))
    }

    fn continue_block(&mut self, idx: usize) -> Cont {
        let kind = self.nodes[idx].kind.clone();
        match kind {
            NKind::Document | NKind::List { .. } => Cont::Matched,
            NKind::BlockQuote { .. } => {
                if !self.indented && self.peek(self.next_nonspace) == Some(b'>') {
                    self.advance_next_nonspace();
                    self.advance_offset(1, false);
                    if self.peek(self.offset).is_some_and(is_space_or_tab) {
                        self.advance_offset(1, true);
                    }
                    Cont::Matched
                } else {
                    Cont::NotMatched
                }
            }
            NKind::Item {
                marker_offset,
                padding,
                ..
            } => {
                if self.blank {
                    if self.nodes[idx].children.is_empty() {
                        return Cont::NotMatched;
                    }
                    self.advance_next_nonspace();
                    Cont::Matched
                } else if self.indent >= marker_offset + padding {
                    self.advance_offset(marker_offset + padding, true);
                    Cont::Matched
                } else {
                    Cont::NotMatched
                }
            }
            NKind::FootnoteDef { .. } => {
                if self.blank {
                    if self.nodes[idx].children.is_empty() {
                        return Cont::NotMatched;
                    }
                    self.advance_next_nonspace();
                    Cont::Matched
                } else if self.indent >= 4 {
                    self.advance_offset(4, true);
                    Cont::Matched
                } else {
                    Cont::NotMatched
                }
            }
            NKind::Heading { .. }
            | NKind::ThematicBreak
            | NKind::LinkDef { .. }
            | NKind::Removed => Cont::NotMatched,
            NKind::Code {
                fence: Some(FenceState {
                    ch, len, indent, ..
                }),
                ..
            } => {
                let nns = self.next_nonspace;
                if self.indent <= 3 && scan::closing_fence(&self.b()[nns..], ch, len) {
                    let span = self.line_start + nns..self.cur_line_end;
                    if let NKind::Code { close, .. } = &mut self.nodes[idx].kind {
                        *close = Some(span);
                    }
                    self.finalize(idx, self.line_no);
                    return Cont::LineDone;
                }
                let mut i = indent;
                while i > 0 && self.peek(self.offset).is_some_and(is_space_or_tab) {
                    self.advance_offset(1, true);
                    i -= 1;
                }
                Cont::Matched
            }
            NKind::Code { fence: None, .. } => {
                if self.indent >= CODE_INDENT {
                    self.advance_offset(CODE_INDENT, true);
                    Cont::Matched
                } else if self.blank {
                    self.advance_next_nonspace();
                    Cont::Matched
                } else {
                    Cont::NotMatched
                }
            }
            NKind::Html { kind } => {
                if self.blank && (kind == 6 || kind == 7) {
                    Cont::NotMatched
                } else {
                    Cont::Matched
                }
            }
            NKind::Paragraph | NKind::Table { .. } => {
                if self.blank {
                    Cont::NotMatched
                } else {
                    Cont::Matched
                }
            }
            NKind::Math { bracket, .. } => {
                let nns = self.next_nonspace;
                let rest = &self.b()[nns..];
                let closer: &[u8] = if bracket { b"\\]" } else { b"$$" };
                if self.indent <= 3 && rest.starts_with(closer) && scan::is_blank_rest(&rest[2..]) {
                    let span = self.line_start + nns..self.cur_line_end;
                    if let NKind::Math { close, .. } = &mut self.nodes[idx].kind {
                        *close = Some(span);
                    }
                    self.finalize(idx, self.line_no);
                    return Cont::LineDone;
                }
                Cont::Matched
            }
            NKind::FrontMatter { .. } => {
                if Some(self.line_no) == self.front_matter_close {
                    let span = self.line_start..self.cur_line_end;
                    if let NKind::FrontMatter { close, .. } = &mut self.nodes[idx].kind {
                        *close = Some(span);
                    }
                    self.finalize(idx, self.line_no);
                    return Cont::LineDone;
                }
                Cont::Matched
            }
        }
    }

    fn try_starts(&mut self, container: usize) -> Start {
        let nns = self.next_nonspace;
        let c = self.peek(nns);
        let in_para = matches!(self.nodes[container].kind, NKind::Paragraph);
        // block quote
        if !self.indented && c == Some(b'>') {
            self.advance_next_nonspace();
            self.advance_offset(1, false);
            if self.peek(self.offset).is_some_and(is_space_or_tab) {
                self.advance_offset(1, true);
            }
            self.close_unmatched_blocks();
            self.add_child(NKind::BlockQuote { alert: None }, nns);
            return Start::Container;
        }
        // ATX heading
        if !self.indented
            && c == Some(b'#')
            && let Some(n) = scan::atx_marker(&self.b()[nns..], !self.o.strict_atx)
        {
            self.close_unmatched_blocks();
            let ls = self.line_start;
            let marker = ls + nns..ls + nns + n;
            let content_start = nns + n;
            let line = self.cur.as_bytes();
            // content: skip leading spaces; strip closing sequence
            let mut a = content_start;
            while a < line.len() && is_space_or_tab(line[a]) {
                a += 1;
            }
            let mut b = line.len();
            while b > a && is_space_or_tab(line[b - 1]) {
                b -= 1;
            }
            let mut closing = None;
            let hashes = line[a..b].iter().rev().take_while(|&&x| x == b'#').count();
            if hashes > 0 {
                let h_start = b - hashes;
                if h_start == a {
                    closing = Some(ls + h_start..ls + b);
                    b = a;
                } else if is_space_or_tab(line[h_start - 1]) {
                    closing = Some(ls + h_start..ls + b);
                    b = h_start;
                    while b > a && is_space_or_tab(line[b - 1]) {
                        b -= 1;
                    }
                }
            }
            let idx = self.add_child(
                NKind::Heading {
                    level: n as u8,
                    setext: false,
                    marker,
                    closing,
                },
                nns,
            );
            self.nodes[idx].lines.push(Line {
                span: ls + a..ls + b,
                pad: 0,
            });
            self.offset = self.cur.len();
            return Start::Leaf;
        }
        // fenced code
        if !self.indented
            && matches!(c, Some(b'`' | b'~'))
            && let Some((ch, len)) = scan::code_fence(&self.b()[nns..])
        {
            self.close_unmatched_blocks();
            let ls = self.line_start;
            let open = ls + nns..self.cur_line_end;
            let indent = self.indent;
            let rest = &self.cur.as_bytes()[nns + len..];
            let lead = rest.iter().take_while(|&&b| is_space_or_tab(b)).count();
            let trail = rest
                .iter()
                .rev()
                .take_while(|&&b| is_space_or_tab(b))
                .count();
            let info = (lead < rest.len()).then(|| {
                let a = ls + nns + len + lead;
                a..ls + nns + len + rest.len() - trail
            });
            let fence = FenceState {
                ch,
                len,
                indent,
                open,
                info,
            };
            self.add_child(
                NKind::Code {
                    fence: Some(fence),
                    close: None,
                },
                nns,
            );
            self.advance_next_nonspace();
            self.advance_offset(len, false);
            return Start::Leaf;
        }
        // math block (`$$`, or `\[` with `latex_delimiters`)
        let bracket =
            self.o.latex_delimiters && c == Some(b'\\') && self.b()[nns..].starts_with(b"\\[");
        if self.o.math_blocks
            && !self.indented
            && (bracket || (c == Some(b'$') && self.b()[nns..].starts_with(b"$$")))
        {
            let closer: &[u8] = if bracket { b"\\]" } else { b"$$" };
            self.close_unmatched_blocks();
            let ls = self.line_start;
            let rest = &self.cur.as_bytes()[nns + 2..];
            let trimmed_end = rest.len()
                - rest
                    .iter()
                    .rev()
                    .take_while(|&&b| is_space_or_tab(b))
                    .count();
            let single =
                trimmed_end >= 2 && rest[..trimmed_end].ends_with(closer) && trimmed_end > 2;
            if single {
                let close_start = ls + nns + 2 + trimmed_end - 2;
                let idx = self.add_child(
                    NKind::Math {
                        open: ls + nns..ls + nns + 2,
                        close: Some(close_start..close_start + 2),
                        bracket,
                    },
                    nns,
                );
                self.nodes[idx].lines.push(Line {
                    span: ls + nns + 2..close_start,
                    pad: 0,
                });
                self.finalize(idx, self.line_no);
                self.offset = self.cur.len();
                self.blank = true;
                return Start::Leaf;
            }
            self.add_child(
                NKind::Math {
                    open: ls + nns..ls + nns + 2,
                    close: None,
                    bracket,
                },
                nns,
            );
            self.advance_next_nonspace();
            self.advance_offset(2, false);
            return Start::Leaf;
        }
        // HTML block
        if !self.indented
            && c == Some(b'<')
            && let Some(kind) = scan::html_block_start(&self.b()[nns..])
        {
            let lazy_para = !self.all_closed
                && !self.blank
                && matches!(self.nodes[self.tip].kind, NKind::Paragraph);
            if kind < 7 || (!in_para && !lazy_para) {
                self.close_unmatched_blocks();
                let off = self.offset;
                self.add_child(NKind::Html { kind }, off);
                return Start::Leaf;
            }
        }
        // setext heading
        if !self.indented
            && in_para
            && let Some(level) = scan::setext_underline(&self.b()[nns..])
        {
            self.close_unmatched_blocks();
            let para = container;
            self.extract_refdefs(para);
            if !self.nodes[para].lines.is_empty() {
                let marker = self.line_start + nns..self.cur_line_end;
                self.nodes[para].kind = NKind::Heading {
                    level,
                    setext: true,
                    marker,
                    closing: None,
                };
                self.nodes[para].text.clear();
                self.offset = self.cur.len();
                return Start::Leaf;
            }
        }
        // thematic break
        if !self.indented && scan::thematic_break(&self.b()[nns..]) {
            self.close_unmatched_blocks();
            self.add_child(NKind::ThematicBreak, nns);
            self.offset = self.cur.len();
            return Start::Leaf;
        }
        // footnote definition
        if self.o.footnotes
            && !self.indented
            && c == Some(b'[')
            && !in_para
            && let Some((label, len)) = footnote_marker(&self.b()[nns..])
        {
            self.close_unmatched_blocks();
            let marker = self.line_start + nns..self.line_start + nns + len;
            self.add_child(NKind::FootnoteDef { label, marker }, nns);
            self.advance_next_nonspace();
            self.advance_offset(len, false);
            return Start::Container;
        }
        // list item
        if (!self.indented || matches!(self.nodes[container].kind, NKind::List { .. }))
            && let Some(data) = self.parse_list_marker(container)
        {
            self.close_unmatched_blocks();
            let matches_list = match &self.nodes[self.tip].kind {
                NKind::List { kind, .. } => *kind == data.kind,
                _ => false,
            };
            if !matches_list {
                self.add_child(
                    NKind::List {
                        kind: data.kind,
                        start: data.number.unwrap_or(1),
                        tight: true,
                    },
                    nns,
                );
            }
            self.add_child(
                NKind::Item {
                    number: data.number,
                    marker: data.marker,
                    marker_offset: data.marker_offset,
                    padding: data.padding,
                    task: None,
                },
                nns,
            );
            return Start::Container;
        }
        // indented code
        if self.o.indented_code
            && self.indented
            && !matches!(self.nodes[self.tip].kind, NKind::Paragraph)
            && !self.blank
        {
            self.advance_offset(CODE_INDENT, true);
            self.close_unmatched_blocks();
            let off = self.offset;
            self.add_child(
                NKind::Code {
                    fence: None,
                    close: None,
                },
                off,
            );
            return Start::Leaf;
        }
        // table (GFM): delimiter row under a paragraph whose last line is the header
        if self.o.tables
            && !self.indented
            && in_para
            && self.all_closed_or_container_is(container)
            && let Some(aligns) = scan::table_delimiter_row(&self.b()[nns..])
            && self.try_table(container, aligns, nns)
        {
            return Start::Leaf;
        }
        Start::None
    }

    fn all_closed_or_container_is(&self, container: usize) -> bool {
        // The delimiter row must belong to the same container as the paragraph
        // (not a lazy continuation line).
        self.tip == container
    }

    fn try_table(&mut self, para: usize, aligns: Vec<Align>, nns: usize) -> bool {
        // Header candidate: the paragraph's last line.
        let Some(last) = self.nodes[para].lines.last().cloned() else {
            return false;
        };
        let nlines = self.nodes[para].lines.len();
        let text_start: usize = self.nodes[para].text_lens[..nlines - 1]
            .iter()
            .map(|l| l + 1)
            .sum();
        let header_text = self.nodes[para].text
            [text_start..text_start + self.nodes[para].text_lens[nlines - 1]]
            .to_owned();
        let cells = scan::table_cells(header_text.as_bytes());
        if cells.len() != aligns.len() {
            return false;
        }
        self.close_unmatched_blocks();
        // Split the header line off the paragraph.
        let base = last.span.start;
        let header = Row {
            span: last.span.clone(),
            cells: cells.iter().map(|r| base + r.start..base + r.end).collect(),
        };
        {
            let p = &mut self.nodes[para];
            p.lines.pop();
            p.text_lens.pop();
            p.text.truncate(text_start);
        }
        let parent = self.nodes[para].parent;
        let delimiter = self.line_start + nns..self.cur_line_end;
        let header_is_whole_para = self.nodes[para].lines.is_empty();
        let para_fresh = self.nodes[para].fresh;
        if self.nodes[para].lines.is_empty() {
            self.nodes[para].kind = NKind::Removed;
            self.nodes[parent].children.retain(|&c| c != para);
            self.tip = parent;
        } else {
            self.finalize(para, self.line_no - 1);
            // The paragraph ended on the line before the header.
            let p = &mut self.nodes[para];
            p.end_line = self.line_no.saturating_sub(2).max(p.start_line);
            p.end = p.lines.last().map_or(p.start, |l| l.span.end);
        }
        let fresh = para_fresh && header_is_whole_para;
        let idx = self.add_child(
            NKind::Table {
                aligns,
                delimiter,
                rows: vec![header],
            },
            nns,
        );
        let n = &mut self.nodes[idx];
        n.fresh = fresh;
        n.start = base;
        n.start_line = self.line_no.saturating_sub(1);
        self.offset = self.cur.len();
        true
    }

    fn parse_list_marker(&mut self, container: usize) -> Option<ListMarker> {
        if self.indent >= 4 {
            return None;
        }
        let nns = self.next_nonspace;
        let rest = &self.b()[nns..];
        let (mlen, ch, number) = scan::list_marker(rest)?;
        let in_para = matches!(self.nodes[container].kind, NKind::Paragraph);
        if number.is_some() && in_para && number != Some(1) {
            return None;
        }
        match rest.get(mlen) {
            None | Some(b' ' | b'\t') => {}
            _ => return None,
        }
        if in_para && scan::is_blank_rest(&rest[mlen..]) {
            return None;
        }
        let kind = if number.is_some() {
            ListKind::Ordered(ch as char)
        } else {
            ListKind::Bullet(ch as char)
        };
        let marker_offset = self.indent;
        let marker = self.line_start + nns..self.line_start + nns + mlen;
        self.advance_next_nonspace();
        self.advance_offset(mlen, true);
        let spaces_start_col = self.column;
        let spaces_start_offset = self.offset;
        loop {
            self.advance_offset(1, true);
            let next = self.peek(self.offset);
            if !(self.column - spaces_start_col < 5 && next.is_some_and(is_space_or_tab)) {
                break;
            }
        }
        let blank_item = self.peek(self.offset).is_none();
        let spaces_after = self.column - spaces_start_col;
        let padding = if !(1..5).contains(&spaces_after) || blank_item {
            self.column = spaces_start_col;
            self.offset = spaces_start_offset;
            if self.peek(self.offset).is_some_and(is_space_or_tab) {
                self.advance_offset(1, true);
            }
            mlen + 1
        } else {
            mlen + spaces_after
        };
        Some(ListMarker {
            kind,
            number,
            marker,
            marker_offset,
            padding,
        })
    }

    fn close_unmatched_blocks(&mut self) {
        if !self.all_closed {
            while self.oldtip != self.last_matched {
                let parent = self.nodes[self.oldtip].parent;
                self.finalize(self.oldtip, self.line_no.wrapping_sub(1));
                self.oldtip = parent;
            }
            self.all_closed = true;
        }
    }

    fn add_child(&mut self, kind: NKind, offset_in_line: usize) -> usize {
        while !self.nodes[self.tip].kind.can_contain(&kind) {
            let t = self.tip;
            self.finalize(t, self.line_no.wrapping_sub(1));
        }
        let idx = self.nodes.len();
        let start = self.line_start + offset_in_line;
        self.nodes.push(Node {
            kind,
            parent: self.tip,
            children: Vec::new(),
            open: true,
            start,
            start_line: self.line_no,
            end: self.cur_line_end,
            end_line: self.line_no,
            lines: Vec::new(),
            text: String::new(),
            text_lens: Vec::new(),
            blank_lines: Vec::new(),
            fresh: true,
        });
        let tip = self.tip;
        self.nodes[tip].children.push(idx);
        self.tip = idx;
        idx
    }

    fn add_line(&mut self) {
        let mut pad = 0u8;
        if self.partially_consumed_tab {
            self.offset += 1;
            pad = (4 - self.column % 4) as u8;
        }
        let tip = self.tip;
        let off = self.offset.min(self.cur.len());
        let span = self.line_start + off..self.cur_line_end;
        let rest_blank = scan::is_blank_rest(&self.cur.as_bytes()[off..]);
        let opening_line = self.nodes[tip].start_line == self.line_no;
        match &self.nodes[tip].kind {
            NKind::Code { fence: Some(_), .. } if opening_line => return,
            // The delimiter row (the line after the header) is not a data row.
            NKind::Table { .. } if self.nodes[tip].start_line + 1 == self.line_no => return,
            NKind::Math { .. } if opening_line && rest_blank => return,
            _ => {}
        }
        match &mut self.nodes[tip].kind {
            NKind::Table { rows, .. } => {
                let text = &self.cur.as_bytes()[off..];
                let base = self.line_start + off;
                let cells = scan::table_cells(text)
                    .into_iter()
                    .map(|r| base + r.start..base + r.end)
                    .collect();
                rows.push(Row { span, cells });
            }
            NKind::Paragraph => {
                let n = &mut self.nodes[tip];
                let content = &self.cur[off..];
                if !n.text.is_empty() || !n.text_lens.is_empty() {
                    n.text.push('\n');
                }
                n.text.push_str(content);
                n.text_lens.push(content.len());
                n.lines.push(Line { span, pad });
            }
            _ => {
                let n = &mut self.nodes[tip];
                n.lines.push(Line { span, pad });
                n.blank_lines.push(rest_blank && pad == 0);
            }
        }
    }

    fn finalize(&mut self, idx: usize, line_no: usize) {
        let parent = self.nodes[idx].parent;
        if !self.nodes[idx].open {
            self.tip = parent;
            return;
        }
        self.nodes[idx].open = false;
        let end = if line_no == self.line_no {
            self.cur_line_end
        } else {
            self.prev_line_end
        };
        {
            let n = &mut self.nodes[idx];
            n.end_line = line_no.max(n.start_line);
            n.end = end.max(n.start);
        }
        let kind = self.nodes[idx].kind.clone();
        match kind {
            NKind::Paragraph => self.finalize_paragraph(idx),
            NKind::Code { fence: None, .. } => {
                // Strip trailing blank lines from indented code.
                let n = &mut self.nodes[idx];
                while n.blank_lines.last() == Some(&true) {
                    n.blank_lines.pop();
                    n.lines.pop();
                }
                if let Some(l) = n.lines.last() {
                    n.end = l.span.end;
                }
                let lines = n.lines.len();
                n.end_line = n.start_line + lines.saturating_sub(1);
            }
            NKind::Item { .. } => {
                if let Some(&last) = self.nodes[idx].children.last() {
                    let (e, el) = (self.nodes[last].end, self.nodes[last].end_line);
                    let n = &mut self.nodes[idx];
                    n.end = e;
                    n.end_line = el;
                } else {
                    let n = &mut self.nodes[idx];
                    n.end_line = n.start_line;
                    if let NKind::Item { marker, .. } = &n.kind {
                        n.end = marker.end;
                    }
                }
                if self.o.tasks {
                    self.detect_task(idx);
                }
            }
            NKind::List { .. } => {
                let mut tight = true;
                let items = self.nodes[idx].children.clone();
                'outer: for (i, &item) in items.iter().enumerate() {
                    if i + 1 < items.len() && self.ends_with_blank_line(item, items[i + 1]) {
                        tight = false;
                        break;
                    }
                    let subs = self.nodes[item].children.clone();
                    for (j, &sub) in subs.iter().enumerate() {
                        if j + 1 < subs.len() && self.ends_with_blank_line(sub, subs[j + 1]) {
                            tight = false;
                            break 'outer;
                        }
                    }
                }
                if let Some(&last) = items.last() {
                    let (e, el) = (self.nodes[last].end, self.nodes[last].end_line);
                    let n = &mut self.nodes[idx];
                    n.end = e;
                    n.end_line = el;
                }
                if let NKind::List { tight: t, .. } = &mut self.nodes[idx].kind {
                    *t = tight;
                }
            }
            NKind::BlockQuote { .. } => {
                if self.o.alerts {
                    self.detect_alert(idx);
                }
            }
            NKind::FootnoteDef { .. } => {
                if let Some(&last) = self.nodes[idx].children.last() {
                    let (e, el) = (self.nodes[last].end, self.nodes[last].end_line);
                    let n = &mut self.nodes[idx];
                    n.end = e;
                    n.end_line = el;
                } else if let NKind::FootnoteDef { marker, .. } = &self.nodes[idx].kind {
                    let e = marker.end;
                    let n = &mut self.nodes[idx];
                    n.end = e;
                    n.end_line = n.start_line;
                }
            }
            _ => {}
        }
        self.tip = parent;
    }

    fn ends_with_blank_line(&self, a: usize, b: usize) -> bool {
        self.nodes[a].end_line + 1 != self.nodes[b].start_line
    }

    /// Moves leading link reference definitions out of paragraph `idx` into sibling
    /// `LinkDef` blocks placed before it.
    fn extract_refdefs(&mut self, idx: usize) {
        if !self.nodes[idx].text.starts_with('[') {
            return;
        }
        let mut consumed = 0usize;
        let mut defs = Vec::new();
        {
            let n = &self.nodes[idx];
            let full = format!("{}\n", n.text);
            while full[consumed..].starts_with('[') {
                match parse_ref_def(&full[consumed..]) {
                    Some(d) => {
                        let len = d.len;
                        defs.push((consumed, len, d));
                        consumed += len;
                    }
                    None => break,
                }
            }
        }
        if defs.is_empty() {
            return;
        }
        // Map text offsets to line indices.
        let lens = self.nodes[idx].text_lens.clone();
        let mut starts = Vec::with_capacity(lens.len() + 1);
        let mut acc = 0;
        for l in &lens {
            starts.push(acc);
            acc += l + 1;
        }
        starts.push(acc);
        let line_of = |off: usize| starts.iter().rposition(|&s| s <= off).unwrap_or(0);
        let parent = self.nodes[idx].parent;
        let mut new_nodes = Vec::new();
        for (off, len, d) in &defs {
            let first = line_of(*off);
            let last = line_of(off + len - 1).min(lens.len() - 1);
            let lines = self.nodes[idx].lines[first..=last].to_vec();
            let span = lines[0].span.start..lines[lines.len() - 1].span.end;
            let start_line = self.nodes[idx].start_line + first;
            new_nodes.push(Node {
                kind: NKind::LinkDef {
                    label: d.label.clone(),
                    dest: d.dest.clone(),
                    title: d.title.clone(),
                },
                parent,
                children: Vec::new(),
                open: false,
                start: span.start,
                start_line,
                end: span.end,
                end_line: self.nodes[idx].start_line + last,
                lines,
                text: String::new(),
                text_lens: Vec::new(),
                blank_lines: Vec::new(),
                fresh: first == 0 && self.nodes[idx].fresh,
            });
        }
        let consumed_lines = if consumed >= acc {
            lens.len()
        } else {
            line_of(consumed)
        };
        {
            let n = &mut self.nodes[idx];
            n.lines.drain(..consumed_lines);
            n.text_lens.drain(..consumed_lines);
            n.text = if consumed > n.text.len() {
                String::new()
            } else {
                n.text[consumed.min(n.text.len())..].to_owned()
            };
            n.start_line += consumed_lines;
            n.fresh = false;
            if let Some(l) = n.lines.first() {
                n.start = l.span.start;
            }
        }
        let pos = self.nodes[parent]
            .children
            .iter()
            .position(|&c| c == idx)
            .unwrap_or(0);
        let mut ids = Vec::new();
        for node in new_nodes {
            ids.push(self.nodes.len());
            self.nodes.push(node);
        }
        let ch = &mut self.nodes[parent].children;
        for (k, id) in ids.into_iter().enumerate() {
            ch.insert(pos + k, id);
        }
        if self.nodes[idx].lines.is_empty() {
            self.nodes[idx].kind = NKind::Removed;
            self.nodes[parent].children.retain(|&c| c != idx);
        }
    }

    fn finalize_paragraph(&mut self, idx: usize) {
        self.extract_refdefs(idx);
        if matches!(self.nodes[idx].kind, NKind::Removed) {
            return;
        }
        let n = &mut self.nodes[idx];
        if let Some(l) = n.lines.last() {
            n.end = l.span.end;
        }
        n.end_line = n.start_line + n.lines.len().saturating_sub(1);
    }

    fn detect_task(&mut self, item: usize) {
        let Some(&first) = self.nodes[item].children.first() else {
            return;
        };
        if !matches!(self.nodes[first].kind, NKind::Paragraph) {
            return;
        }
        let text = &self.nodes[first].text;
        let b = text.as_bytes();
        if b.len() < 3 || b[0] != b'[' || b[2] != b']' {
            return;
        }
        let checked = match b[1] {
            b' ' => false,
            b'x' | b'X' => true,
            _ => return,
        };
        let first_len = self.nodes[first].text_lens[0];
        match b.get(3) {
            Some(b' ' | b'\t') => {}
            None => {}
            Some(b'\n') if first_len == 3 => {}
            _ => return,
        }
        let start = self.nodes[first].lines[0].span.start;
        let span = start..start + 3;
        // Skip the checkbox and following whitespace in the paragraph.
        let mut skip = 3;
        while skip < first_len && is_space_or_tab(b[skip]) {
            skip += 1;
        }
        let p = &mut self.nodes[first];
        if skip >= first_len {
            p.lines.remove(0);
            p.text_lens.remove(0);
            p.text = p.text.get(first_len + 1..).unwrap_or_default().to_owned();
            if p.lines.is_empty() {
                p.kind = NKind::Removed;
                self.nodes[item].children.remove(0);
            } else {
                p.start = p.lines[0].span.start;
                p.start_line += 1;
            }
        } else {
            p.lines[0].span.start += skip;
            p.text_lens[0] -= skip;
            p.text = p.text[skip..].to_owned();
            p.start += skip;
        }
        if let NKind::Item { task, .. } = &mut self.nodes[item].kind {
            *task = Some(Task { checked, span });
        }
    }

    fn detect_alert(&mut self, quote: usize) {
        let Some(&first) = self.nodes[quote].children.first() else {
            return;
        };
        if !matches!(self.nodes[first].kind, NKind::Paragraph) {
            return;
        }
        let first_len = self.nodes[first].text_lens[0];
        let line = self.nodes[first].text[..first_len].trim_end();
        let Some(name) = line.strip_prefix("[!").and_then(|r| r.strip_suffix(']')) else {
            return;
        };
        let Some(kind) = AlertKind::from_name(name) else {
            return;
        };
        let start = self.nodes[first].lines[0].span.start;
        let marker = start..start + line.len();
        let p = &mut self.nodes[first];
        p.lines.remove(0);
        p.text_lens.remove(0);
        p.text = p.text.get(first_len + 1..).unwrap_or_default().to_owned();
        if p.lines.is_empty() {
            p.kind = NKind::Removed;
            self.nodes[quote].children.remove(0);
        } else {
            p.start = p.lines[0].span.start;
            p.start_line += 1;
        }
        if let NKind::BlockQuote { alert } = &mut self.nodes[quote].kind {
            *alert = Some((kind, marker));
        }
    }

    /// Finishes parsing and returns the top-level blocks.
    pub fn finish(mut self) -> (Vec<Block>, Vec<bool>) {
        if self.line_no > 0 {
            // Finalize against the last fed line.
            self.line_no -= 1;
        }
        while self.tip != 0 {
            let t = self.tip;
            self.finalize(t, self.line_no);
        }
        self.output()
    }

    /// Top-level blocks and, per block, whether it is a fresh restart point.
    fn output(&self) -> (Vec<Block>, Vec<bool>) {
        self.nodes[0]
            .children
            .iter()
            .filter_map(|&c| self.to_block(c).map(|b| (b, self.nodes[c].fresh)))
            .unzip()
    }

    fn to_block(&self, idx: usize) -> Option<Block> {
        let n = &self.nodes[idx];
        let span = n.start..n.end.max(n.start);
        let children = || {
            n.children
                .iter()
                .filter_map(|&c| self.to_block(c))
                .collect::<Vec<_>>()
        };
        let block = match &n.kind {
            NKind::Removed | NKind::Document => return None,
            NKind::BlockQuote { alert } => Block {
                children: children(),
                ..Block::new(
                    BlockKind::BlockQuote {
                        alert: alert.clone(),
                    },
                    span,
                )
            },
            NKind::List { kind, start, tight } => Block {
                children: children(),
                ..Block::new(
                    BlockKind::List {
                        kind: *kind,
                        start: *start,
                        tight: *tight,
                    },
                    span,
                )
            },
            NKind::Item {
                number,
                marker,
                marker_offset,
                padding,
                task,
                ..
            } => {
                let task = task.clone();
                Block {
                    children: children(),
                    ..Block::new(
                        BlockKind::ListItem {
                            marker: marker.clone(),
                            number: *number,
                            task,
                            content_offset: marker_offset + padding,
                        },
                        span,
                    )
                }
            }
            NKind::FootnoteDef { label, marker } => Block {
                children: children(),
                ..Block::new(
                    BlockKind::FootnoteDef {
                        label: label.clone(),
                        marker: marker.clone(),
                    },
                    span,
                )
            },
            NKind::Paragraph => {
                let toc =
                    self.o.toc && n.lines.len() == 1 && n.text.trim().eq_ignore_ascii_case("[toc]");
                let kind = if toc {
                    BlockKind::Toc
                } else {
                    BlockKind::Paragraph
                };
                Block {
                    lines: n.lines.clone(),
                    ..Block::new(kind, span)
                }
            }
            NKind::Heading {
                level,
                setext,
                marker,
                closing,
            } => {
                let span = if *setext {
                    span.start..marker.end.max(span.end)
                } else {
                    span
                };
                Block {
                    lines: n.lines.clone(),
                    ..Block::new(
                        BlockKind::Heading {
                            level: *level,
                            setext: *setext,
                            marker: marker.clone(),
                            closing: closing.clone(),
                        },
                        span,
                    )
                }
            }
            NKind::ThematicBreak => Block::new(BlockKind::ThematicBreak, span),
            NKind::Code { fence, close } => {
                let fence = fence.as_ref().map(|f| Fence {
                    ch: f.ch as char,
                    len: f.len,
                    indent: f.indent,
                    open: f.open.clone(),
                    info: f.info.clone(),
                    close: close.clone(),
                });
                Block {
                    lines: n.lines.clone(),
                    ..Block::new(BlockKind::CodeBlock { fence }, span)
                }
            }
            NKind::Html { kind } => Block {
                lines: n.lines.clone(),
                ..Block::new(BlockKind::HtmlBlock { kind: *kind }, span)
            },
            NKind::Table {
                aligns,
                delimiter,
                rows,
            } => {
                let ncols = aligns.len();
                let rows = rows
                    .iter()
                    .enumerate()
                    .map(|(i, r)| {
                        let cells = r
                            .cells
                            .iter()
                            .take(ncols)
                            .map(|c| Block {
                                lines: vec![Line {
                                    span: c.clone(),
                                    pad: 0,
                                }],
                                ..Block::new(BlockKind::TableCell, c.clone())
                            })
                            .collect();
                        Block {
                            children: cells,
                            ..Block::new(BlockKind::TableRow { header: i == 0 }, r.span.clone())
                        }
                    })
                    .collect();
                Block {
                    children: rows,
                    ..Block::new(
                        BlockKind::Table {
                            aligns: aligns.clone(),
                            delimiter: delimiter.clone(),
                        },
                        span,
                    )
                }
            }
            NKind::Math { open, close, .. } => Block {
                lines: n.lines.clone(),
                ..Block::new(
                    BlockKind::MathBlock {
                        open: open.clone(),
                        close: close.clone(),
                    },
                    span,
                )
            },
            NKind::FrontMatter { open, close } => {
                let close = close.clone().unwrap_or(n.end..n.end);
                Block {
                    lines: n.lines.clone(),
                    ..Block::new(
                        BlockKind::FrontMatter {
                            open: open.clone(),
                            close,
                        },
                        span,
                    )
                }
            }
            NKind::LinkDef { label, dest, title } => Block {
                lines: n.lines.clone(),
                ..Block::new(
                    BlockKind::LinkDef {
                        label: label.clone(),
                        dest: dest.clone(),
                        title: title.clone(),
                    },
                    span,
                )
            },
        };
        Some(block)
    }
}

struct ListMarker {
    kind: ListKind,
    number: Option<u64>,
    marker: Span,
    marker_offset: usize,
    padding: usize,
}

/// `[^label]:` at the start of `s`; returns `(normalized label, length)`.
fn footnote_marker(s: &[u8]) -> Option<(String, usize)> {
    if !s.starts_with(b"[^") {
        return None;
    }
    let close = s[2..].iter().position(|&b| b == b']')? + 2;
    let label = &s[2..close];
    if label.is_empty() || label.iter().any(|b| b.is_ascii_whitespace() || *b == b'[') {
        return None;
    }
    if s.get(close + 1) != Some(&b':') {
        return None;
    }
    let label = std::str::from_utf8(label).ok()?;
    Some((normalize_label(label), close + 2))
}
