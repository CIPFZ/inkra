//! TeX math source → math list (TeXbook chapter 26, simplified). The parser
//! never fails: unknown commands become [`Node::Error`] nodes that are rendered
//! as red source text, and the problem is reported.

use crate::symbols::{self, Class, Variant};

/// Math styles (D, T, S, SS); cramping is tracked separately.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MathStyle {
    /// `\displaystyle`.
    Display,
    /// `\textstyle`.
    Text,
    /// `\scriptstyle`.
    Script,
    /// `\scriptscriptstyle`.
    ScriptScript,
}

/// Column alignment in arrays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColAlign {
    /// Left.
    Left,
    /// Center.
    Center,
    /// Right.
    Right,
}

/// Kinds of arrays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArrayKind {
    /// `matrix` & co: centered columns, text style cells.
    Matrix,
    /// `cases`: left-aligned, text style.
    Cases,
    /// `aligned`, `align*`, `split`: alternating right/left columns, display style.
    Aligned,
    /// `gather`, `gathered`: centered rows, display style.
    Gathered,
    /// `array{…}`, `subarray`, `\substack`: explicit columns.
    Array,
}

/// A math list item.
#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    /// A character with its class and letter style.
    Sym {
        /// Character.
        ch: char,
        /// Class.
        class: Class,
        /// Letter style.
        variant: Variant,
    },
    /// A large operator character (`\sum`).
    BigOp {
        /// Character.
        ch: char,
        /// Limits above/below in display style (`None`: operator default).
        limits: Option<bool>,
        /// Default for this operator.
        default_limits: bool,
    },
    /// An operator name (`\sin`, `\operatorname{…}`).
    OpName {
        /// Name.
        name: String,
        /// Limits.
        limits: Option<bool>,
        /// Default for this operator.
        default_limits: bool,
    },
    /// Text (`\text{…}`).
    Text {
        /// Content.
        text: String,
        /// Bold.
        bold: bool,
        /// Italic.
        italic: bool,
    },
    /// A braced group (an Ord atom).
    Group(Vec<Node>),
    /// Nucleus with scripts.
    Scripts {
        /// Nucleus.
        base: Box<Node>,
        /// Superscript.
        sup: Option<Vec<Node>>,
        /// Subscript.
        sub: Option<Vec<Node>>,
    },
    /// A fraction or a stack (`\binom`, `\atop`).
    Frac {
        /// Numerator.
        num: Vec<Node>,
        /// Denominator.
        den: Vec<Node>,
        /// Draw the rule.
        rule: bool,
        /// Forced style (`\dfrac`, `\tfrac`).
        style: Option<MathStyle>,
        /// Surrounding delimiters (`\binom`).
        delims: Option<(char, char)>,
    },
    /// A radical.
    Sqrt {
        /// Degree.
        index: Option<Vec<Node>>,
        /// Radicand.
        body: Vec<Node>,
    },
    /// `\left … \middle … \right`.
    LeftRight {
        /// Left delimiter (`None`: `.`).
        left: Option<char>,
        /// Body; `\middle` appears as [`Node::Middle`].
        body: Vec<Node>,
        /// Right delimiter.
        right: Option<char>,
    },
    /// `\middle` delimiter.
    Middle(Option<char>),
    /// `\big(` & co: a delimiter of fixed size (1–4).
    Big {
        /// Delimiter.
        ch: Option<char>,
        /// Size step.
        size: u8,
        /// Class.
        class: Class,
    },
    /// An accent over (or under) the body.
    Accent {
        /// Accent character.
        ch: char,
        /// Body.
        body: Vec<Node>,
        /// Stretch to the body width.
        stretchy: bool,
        /// Below the body.
        under: bool,
    },
    /// `\overline`.
    Overline(Vec<Node>),
    /// `\underline`.
    Underline(Vec<Node>),
    /// `\overbrace` / `\underbrace` (labels come as scripts).
    Brace {
        /// Body.
        body: Vec<Node>,
        /// Above.
        over: bool,
    },
    /// `\overset{over}{base}`, `\underset`, `\stackrel`.
    Stack {
        /// Base.
        base: Vec<Node>,
        /// Above.
        over: Option<Vec<Node>>,
        /// Below.
        under: Option<Vec<Node>>,
        /// Class of the result.
        class: Class,
    },
    /// An array environment.
    Array {
        /// Kind.
        kind: ArrayKind,
        /// Rows of cells.
        rows: Vec<Vec<Vec<Node>>>,
        /// Explicit column alignments (`array{lcr}`).
        cols: Vec<ColAlign>,
        /// Surrounding delimiters.
        delims: (Option<char>, Option<char>),
        /// Smaller (`smallmatrix`, `\substack`).
        small: bool,
        /// Vertical rules: before column `i` (`array{c|c}`); `i == ncols` is the right edge.
        vlines: Vec<usize>,
    },
    /// Horizontal space in mu (1/18 em; negative allowed).
    Space(f32),
    /// Horizontal space in em.
    SpaceEm(f32),
    /// A colored list.
    Color {
        /// RGB.
        rgb: [u8; 3],
        /// Body.
        body: Vec<Node>,
    },
    /// `\boxed`.
    Boxed(Vec<Node>),
    /// A style switch for the rest of the list.
    Style(MathStyle),
    /// A list forced into a class (`\mathbin{…}`).
    Class {
        /// Class.
        class: Class,
        /// Body.
        body: Vec<Node>,
    },
    /// `\phantom` & co.
    Phantom {
        /// Body.
        body: Vec<Node>,
        /// Keeps width.
        width: bool,
        /// Keeps height and depth.
        height: bool,
    },
    /// `\not` over a symbol without a precomposed negation.
    Not(Box<Node>),
    /// Unparsable or unsupported source, rendered in red.
    Error(String),
}

/// A parsed formula.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Parsed {
    /// The math list.
    pub list: Vec<Node>,
    /// `\tag{…}` text (without parentheses), `\tag*` raw.
    pub tag: Option<(String, bool)>,
    /// `\label{…}`.
    pub labels: Vec<String>,
    /// Problems (unknown commands, unbalanced groups).
    pub errors: Vec<String>,
    /// The formula uses an environment that is numbered (AMS rule, MD-008).
    pub numbered_env: bool,
    /// `\notag` / `\nonumber`.
    pub notag: bool,
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Cmd(String),
    Char(char),
    Open,
    Close,
    Sup,
    Sub,
    Amp,
    RowSep,
    Prime(u8),
}

fn tokenize(src: &str) -> Vec<Tok> {
    let cs: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < cs.len() {
        let c = cs[i];
        match c {
            '\\' => {
                let j = i + 1;
                if j >= cs.len() {
                    out.push(Tok::Char('\\'));
                    i = j;
                } else if cs[j].is_ascii_alphabetic() {
                    let mut k = j;
                    while k < cs.len() && cs[k].is_ascii_alphabetic() {
                        k += 1;
                    }
                    // `\begin*`-style starred names are handled by the parser.
                    out.push(Tok::Cmd(cs[j..k].iter().collect()));
                    i = k;
                } else if cs[j] == '\\' {
                    out.push(Tok::RowSep);
                    i = j + 1;
                } else {
                    out.push(Tok::Cmd(cs[j].to_string()));
                    i = j + 1;
                }
            }
            '{' => {
                out.push(Tok::Open);
                i += 1;
            }
            '}' => {
                out.push(Tok::Close);
                i += 1;
            }
            '^' => {
                out.push(Tok::Sup);
                i += 1;
            }
            '_' => {
                out.push(Tok::Sub);
                i += 1;
            }
            '&' => {
                out.push(Tok::Amp);
                i += 1;
            }
            '%' => {
                while i < cs.len() && cs[i] != '\n' {
                    i += 1;
                }
            }
            '~' => {
                out.push(Tok::Cmd(" ".into()));
                i += 1;
            }
            '\'' => {
                let mut n = 0u8;
                while i < cs.len() && cs[i] == '\'' {
                    n = n.saturating_add(1);
                    i += 1;
                }
                out.push(Tok::Prime(n));
            }
            c if c.is_whitespace() => i += 1,
            c => {
                out.push(Tok::Char(c));
                i += 1;
            }
        }
    }
    out
}

/// Where a list ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stop {
    End,
    Group,
    Bracket,
    Right,
    Cell,
}

/// Why a list ended.
#[derive(Debug, Clone, PartialEq)]
enum Ended {
    Eof,
    Close,
    Bracket,
    Right(Option<char>),
    Amp,
    RowSep,
    EndEnv(String),
}

struct Parser {
    refs: Vec<(String, String)>,
    toks: Vec<Tok>,
    pos: usize,
    out: Parsed,
    variant: Variant,
    depth: usize,
}

const MAX_DEPTH: usize = 64;

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos)
    }

    fn next(&mut self) -> Option<Tok> {
        let t = self.toks.get(self.pos).cloned();
        if t.is_some() {
            self.pos += 1;
        }
        t
    }

    fn err(&mut self, msg: impl Into<String>) {
        let m = msg.into();
        if !self.out.errors.contains(&m) {
            self.out.errors.push(m);
        }
    }

    /// Raw text of a braced argument (for `\text`, `\color`, `\begin`).
    fn raw_arg(&mut self) -> String {
        match self.peek() {
            Some(Tok::Open) => {
                self.next();
                let mut depth = 1;
                let mut s = String::new();
                while let Some(t) = self.next() {
                    match t {
                        Tok::Open => {
                            depth += 1;
                            s.push('{');
                        }
                        Tok::Close => {
                            depth -= 1;
                            if depth == 0 {
                                return s;
                            }
                            s.push('}');
                        }
                        Tok::Char(c) => s.push(c),
                        Tok::Cmd(c)
                            if c == " " || c == "," || c == ";" || c == ":" || c == "quad" =>
                        {
                            s.push(' ')
                        }
                        Tok::Cmd(c) if c.len() == 1 => s.push_str(&c),
                        Tok::Cmd(c) => {
                            s.push('\\');
                            s.push_str(&c);
                        }
                        Tok::Sup => s.push('^'),
                        Tok::Sub => s.push('_'),
                        Tok::Amp => s.push('&'),
                        Tok::RowSep => s.push('\n'),
                        Tok::Prime(n) => s.push_str(&"'".repeat(n as usize)),
                    }
                }
                self.err("missing }");
                s
            }
            Some(Tok::Char(_)) => match self.next() {
                Some(Tok::Char(c)) => c.to_string(),
                _ => String::new(),
            },
            _ => String::new(),
        }
    }

    /// Text-mode argument: the raw text with spaces preserved. Tokenizing drops
    /// whitespace, so text arguments are reconstructed from the source.
    fn text_arg(&mut self, src: &TextSource) -> String {
        if let Some(Tok::Open) = self.peek() {
            let idx = self.pos;
            if let Some(t) = src.text_at(idx) {
                // Skip the tokens of this group.
                let mut depth = 0;
                while let Some(tok) = self.next() {
                    match tok {
                        Tok::Open => depth += 1,
                        Tok::Close => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                return t;
            }
        }
        self.raw_arg()
    }

    /// One argument as a math list: a group or a single atom.
    fn arg(&mut self, src: &TextSource) -> Vec<Node> {
        match self.peek() {
            Some(Tok::Open) => {
                self.next();
                let (l, end) = self.list(Stop::Group, src);
                if end != Ended::Close {
                    self.err("missing }");
                }
                l
            }
            None => {
                self.err("missing argument");
                Vec::new()
            }
            _ => match self.atom(src) {
                Some(n) => vec![n],
                None => Vec::new(),
            },
        }
    }

    /// `[…]` optional argument.
    fn opt_arg(&mut self, src: &TextSource) -> Option<Vec<Node>> {
        if self.peek() == Some(&Tok::Char('[')) {
            self.next();
            let (l, end) = self.list(Stop::Bracket, src);
            if end != Ended::Bracket {
                self.err("missing ]");
            }
            Some(l)
        } else {
            None
        }
    }

    fn opt_raw(&mut self) -> Option<String> {
        if self.peek() == Some(&Tok::Char('[')) {
            self.next();
            let mut s = String::new();
            while let Some(t) = self.next() {
                match t {
                    Tok::Char(']') => return Some(s),
                    Tok::Char(c) => s.push(c),
                    _ => {}
                }
            }
            Some(s)
        } else {
            None
        }
    }

    /// A delimiter after `\left`, `\big`, …
    fn delim(&mut self) -> Option<Option<char>> {
        match self.next()? {
            Tok::Char('.') => Some(None),
            Tok::Char(c) => Some(Some(c)),
            Tok::Cmd(name) => {
                if let Some(c) = symbols::delimiter(&name) {
                    Some(Some(c))
                } else if name == "|" {
                    Some(Some('‖'))
                } else {
                    self.err(format!("bad delimiter \\{name}"));
                    Some(None)
                }
            }
            _ => {
                self.err("missing delimiter");
                Some(None)
            }
        }
    }

    fn list(&mut self, stop: Stop, src: &TextSource) -> (Vec<Node>, Ended) {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            self.depth -= 1;
            self.err("nesting too deep");
            // Consume the rest so the caller terminates.
            self.pos = self.toks.len();
            return (Vec::new(), Ended::Eof);
        }
        let saved_variant = self.variant;
        let mut out: Vec<Node> = Vec::new();
        let ended = loop {
            let Some(t) = self.peek().cloned() else {
                break Ended::Eof;
            };
            match t {
                Tok::Close => {
                    self.next();
                    if stop == Stop::End {
                        self.err("unmatched }");
                        continue;
                    }
                    break Ended::Close;
                }
                Tok::Char(']') if stop == Stop::Bracket => {
                    self.next();
                    break Ended::Bracket;
                }
                Tok::Amp => {
                    self.next();
                    if stop == Stop::Cell {
                        break Ended::Amp;
                    }
                    // Outside arrays `&` is ignored (aligned blocks without env).
                    continue;
                }
                Tok::RowSep => {
                    self.next();
                    if stop == Stop::Cell {
                        break Ended::RowSep;
                    }
                    continue;
                }
                Tok::Cmd(ref c) if c == "right" => {
                    self.next();
                    let d = self.delim().unwrap_or(None);
                    if stop == Stop::Right {
                        break Ended::Right(d);
                    }
                    self.err("\\right without \\left");
                    continue;
                }
                Tok::Cmd(ref c) if c == "end" => {
                    self.next();
                    let name = self.raw_arg();
                    if stop == Stop::Cell {
                        break Ended::EndEnv(name);
                    }
                    self.err(format!("\\end{{{name}}} without \\begin"));
                    continue;
                }
                Tok::Cmd(ref c)
                    if matches!(c.as_str(), "over" | "atop" | "choose" | "brace" | "brack") =>
                {
                    // Generalized fractions take the whole list so far.
                    self.next();
                    let kind = c.clone();
                    let num = std::mem::take(&mut out);
                    let (den, end) = self.list(stop, src);
                    let (rule, delims) = match kind.as_str() {
                        "over" => (true, None),
                        "atop" => (false, None),
                        "choose" => (false, Some(('(', ')'))),
                        "brace" => (false, Some(('{', '}'))),
                        _ => (false, Some(('[', ']'))),
                    };
                    out.push(Node::Frac {
                        num,
                        den,
                        rule,
                        style: None,
                        delims,
                    });
                    break end;
                }
                _ => {}
            }
            // Style switches inside the list.
            if let Some(Tok::Cmd(c)) = self.peek() {
                let st = match c.as_str() {
                    "displaystyle" => Some(MathStyle::Display),
                    "textstyle" => Some(MathStyle::Text),
                    "scriptstyle" => Some(MathStyle::Script),
                    "scriptscriptstyle" => Some(MathStyle::ScriptScript),
                    _ => None,
                };
                if let Some(st) = st {
                    self.next();
                    out.push(Node::Style(st));
                    continue;
                }
                if c == "color" {
                    self.next();
                    let model = self.opt_raw();
                    let spec = self.raw_arg();
                    let rgb = parse_color(model.as_deref(), &spec);
                    if rgb.is_none() {
                        self.err(format!("unknown color {spec}"));
                    }
                    let (body, end) = self.list(stop, src);
                    out.push(Node::Color {
                        rgb: rgb.unwrap_or([255, 0, 0]),
                        body,
                    });
                    break end;
                }
                if matches!(c.as_str(), "limits" | "nolimits") {
                    let on = c == "limits";
                    self.next();
                    if let Some(last) = out.last_mut() {
                        set_limits(last, on);
                    }
                    continue;
                }
            }
            match self.peek() {
                Some(Tok::Sup) | Some(Tok::Sub) | Some(Tok::Prime(_)) => {
                    // Scripts without a nucleus (or a second set after a scripted
                    // atom): attach to an empty nucleus or the previous atom.
                    let base = match out.pop() {
                        Some(Node::Scripts { base, sup, sub }) => {
                            let n = Node::Scripts { base, sup, sub };
                            self.scripts(n, src)
                        }
                        Some(prev) => self.scripts(prev, src),
                        None => self.scripts(Node::Group(Vec::new()), src),
                    };
                    out.push(base);
                }
                _ => {
                    if let Some(a) = self.atom(src) {
                        let a = if matches!(self.peek(), Some(Tok::Sup | Tok::Sub | Tok::Prime(_)))
                        {
                            self.scripts(a, src)
                        } else {
                            a
                        };
                        out.push(a);
                    }
                }
            }
        };
        self.variant = saved_variant;
        self.depth -= 1;
        (out, ended)
    }

    fn scripts(&mut self, base: Node, src: &TextSource) -> Node {
        let (base, mut sup, mut sub) = match base {
            Node::Scripts { base, sup, sub } => (*base, sup, sub),
            b => (b, None, None),
        };
        let mut base_slot = Some(base);
        loop {
            match self.peek() {
                Some(Tok::Sup) => {
                    self.next();
                    let a = self.arg(src);
                    match &mut sup {
                        Some(s) => {
                            // `x'^2`: primes and the superscript share one script.
                            s.extend(a);
                        }
                        None => sup = Some(a),
                    }
                }
                Some(Tok::Sub) => {
                    self.next();
                    let a = self.arg(src);
                    if sub.is_some() {
                        self.err("double subscript");
                    }
                    sub = Some(a);
                }
                Some(Tok::Prime(n)) => {
                    let n = *n;
                    self.next();
                    let ch = match n {
                        1 => '′',
                        2 => '″',
                        3 => '‴',
                        _ => '⁗',
                    };
                    // Unicode primes are designed at superscript position at
                    // full size: they follow the nucleus instead of becoming a
                    // (tiny, too high) superscript.
                    let p = Node::Sym {
                        ch,
                        class: Class::Ord,
                        variant: Variant::Upright,
                    };
                    match &mut sup {
                        Some(s) => s.insert(0, p),
                        None => {
                            let b = base_slot.take().unwrap_or(Node::Group(Vec::new()));
                            base_slot = Some(Node::Group(vec![b, p]));
                        }
                    }
                }
                _ => break,
            }
        }
        let base = base_slot.unwrap_or(Node::Group(Vec::new()));
        if sup.is_none() && sub.is_none() {
            return base;
        }
        Node::Scripts {
            base: Box::new(base),
            sup,
            sub,
        }
    }

    fn sym(&self, ch: char, class: Class) -> Node {
        Node::Sym {
            ch,
            class,
            variant: self.variant,
        }
    }

    /// One atom (no scripts).
    fn atom(&mut self, src: &TextSource) -> Option<Node> {
        let t = self.next()?;
        Some(match t {
            Tok::Open => {
                let (l, end) = self.list(Stop::Group, src);
                if end != Ended::Close {
                    self.err("missing }");
                }
                Node::Group(l)
            }
            Tok::Char(c) => {
                if c == '-' {
                    self.sym('−', Class::Bin)
                } else if c == '*' {
                    self.sym('∗', Class::Bin)
                } else if is_cjk(c) {
                    // CJK in math mode is set as text.
                    let mut s = c.to_string();
                    while let Some(Tok::Char(n)) = self.peek() {
                        if !is_cjk(*n) {
                            break;
                        }
                        s.push(*n);
                        self.next();
                    }
                    Node::Text {
                        text: s,
                        bold: false,
                        italic: false,
                    }
                } else {
                    self.sym(c, symbols::char_class(c))
                }
            }
            Tok::Close | Tok::Amp | Tok::RowSep | Tok::Sup | Tok::Sub => return None,
            Tok::Prime(_) => return None,
            Tok::Cmd(name) => self.command(&name, src),
        })
    }

    fn command(&mut self, name: &str, src: &TextSource) -> Node {
        if let Some((ch, class)) = symbols::symbol(name) {
            return self.sym(ch, class);
        }
        if let Some((ch, limits)) = symbols::operator(name) {
            return Node::BigOp {
                ch,
                limits: None,
                default_limits: limits,
            };
        }
        if let Some(limits) = symbols::function(name) {
            let display_name = match name {
                "liminf" => "lim inf",
                "limsup" => "lim sup",
                "argmax" => "arg max",
                "argmin" => "arg min",
                n => n,
            };
            return Node::OpName {
                name: display_name.into(),
                limits: None,
                default_limits: limits,
            };
        }
        let space = |mu: f32| Node::Space(mu);
        match name {
            // Escaped characters.
            "{" => self.sym('{', Class::Open),
            "}" => self.sym('}', Class::Close),
            "|" => self.sym('‖', Class::Ord),
            "#" | "$" | "%" | "&" | "_" => Node::Sym {
                ch: name.chars().next().unwrap_or('?'),
                class: Class::Ord,
                variant: Variant::Upright,
            },
            "langle" => self.sym('⟨', Class::Open),
            "rangle" => self.sym('⟩', Class::Close),
            "lbrace" => self.sym('{', Class::Open),
            "rbrace" => self.sym('}', Class::Close),
            "lfloor" => self.sym('⌊', Class::Open),
            "rfloor" => self.sym('⌋', Class::Close),
            "lceil" => self.sym('⌈', Class::Open),
            "rceil" => self.sym('⌉', Class::Close),
            "lvert" => self.sym('|', Class::Open),
            "rvert" => self.sym('|', Class::Close),
            "lVert" => self.sym('‖', Class::Open),
            "rVert" => self.sym('‖', Class::Close),
            "vert" => self.sym('|', Class::Ord),
            "Vert" => self.sym('‖', Class::Ord),
            "lbrack" => self.sym('[', Class::Open),
            "rbrack" => self.sym(']', Class::Close),
            // Spaces.
            "," | "thinspace" => space(3.0),
            ":" | ">" | "medspace" => space(4.0),
            ";" | "thickspace" => space(5.0),
            "!" | "negthinspace" => space(-3.0),
            "negmedspace" => space(-4.0),
            "negthickspace" => space(-5.0),
            " " => Node::SpaceEm(0.25),
            "quad" => Node::SpaceEm(1.0),
            "qquad" => Node::SpaceEm(2.0),
            "enspace" => Node::SpaceEm(0.5),
            "hspace" | "kern" | "mkern" | "mskip" | "hskip" => {
                let s = if name == "hspace" || matches!(self.peek(), Some(Tok::Open)) {
                    self.raw_arg()
                } else {
                    // `\kern3mu`: read until a unit.
                    let mut s = String::new();
                    while let Some(Tok::Char(c)) = self.peek() {
                        s.push(*c);
                        self.next();
                        if s.ends_with("mu")
                            || s.ends_with("em")
                            || s.ends_with("pt")
                            || s.ends_with("ex")
                        {
                            break;
                        }
                    }
                    s
                };
                match length_em(&s) {
                    Some(em) => Node::SpaceEm(em),
                    None => {
                        self.err(format!("bad length {s}"));
                        Node::SpaceEm(0.0)
                    }
                }
            }
            // Fractions and stacks.
            "frac" | "dfrac" | "tfrac" | "cfrac" => {
                let num = self.arg(src);
                let den = self.arg(src);
                let style = match name {
                    "dfrac" | "cfrac" => Some(MathStyle::Display),
                    "tfrac" => Some(MathStyle::Text),
                    _ => None,
                };
                Node::Frac {
                    num,
                    den,
                    rule: true,
                    style,
                    delims: None,
                }
            }
            "binom" | "dbinom" | "tbinom" => {
                let num = self.arg(src);
                let den = self.arg(src);
                let style = match name {
                    "dbinom" => Some(MathStyle::Display),
                    "tbinom" => Some(MathStyle::Text),
                    _ => None,
                };
                Node::Frac {
                    num,
                    den,
                    rule: false,
                    style,
                    delims: Some(('(', ')')),
                }
            }
            "genfrac" => {
                let l = self.raw_arg();
                let r = self.raw_arg();
                let thick = self.raw_arg();
                let st = self.raw_arg();
                let num = self.arg(src);
                let den = self.arg(src);
                let d = |s: &str| {
                    s.trim()
                        .trim_start_matches('\\')
                        .chars()
                        .next()
                        .filter(|c| *c != '.')
                };
                let delims = match (d(&l), d(&r)) {
                    (None, None) => None,
                    (a, b) => Some((a.unwrap_or(' '), b.unwrap_or(' '))),
                };
                let style = match st.trim() {
                    "0" => Some(MathStyle::Display),
                    "1" => Some(MathStyle::Text),
                    "2" => Some(MathStyle::Script),
                    "3" => Some(MathStyle::ScriptScript),
                    _ => None,
                };
                Node::Frac {
                    num,
                    den,
                    rule: thick.trim() != "0pt" && thick.trim() != "0",
                    style,
                    delims,
                }
            }
            "sqrt" => {
                let index = self.opt_arg(src);
                let body = self.arg(src);
                Node::Sqrt { index, body }
            }
            "root" => {
                // `\root n \of x`
                let mut idx = Vec::new();
                while let Some(t) = self.peek() {
                    if *t == Tok::Cmd("of".into()) {
                        self.next();
                        break;
                    }
                    if let Some(a) = self.atom(src) {
                        idx.push(a);
                    } else {
                        break;
                    }
                }
                let body = self.arg(src);
                Node::Sqrt {
                    index: Some(idx),
                    body,
                }
            }
            // Delimiters.
            "left" => {
                let left = self.delim().unwrap_or(None);
                let (body, end) = self.list(Stop::Right, src);
                let right = match end {
                    Ended::Right(d) => d,
                    _ => {
                        self.err("missing \\right");
                        None
                    }
                };
                Node::LeftRight { left, body, right }
            }
            "middle" => Node::Middle(self.delim().unwrap_or(None)),
            "big" | "Big" | "bigg" | "Bigg" | "bigl" | "Bigl" | "biggl" | "Biggl" | "bigr"
            | "Bigr" | "biggr" | "Biggr" | "bigm" | "Bigm" | "biggm" | "Biggm" => {
                let size = match name.trim_end_matches(['l', 'r', 'm']) {
                    "big" => 1,
                    "Big" => 2,
                    "bigg" => 3,
                    _ => 4,
                };
                let ch = self.delim().unwrap_or(None);
                let class = if name.ends_with('l') {
                    Class::Open
                } else if name.ends_with('r') {
                    Class::Close
                } else if name.ends_with('m') {
                    Class::Rel
                } else {
                    Class::Ord
                };
                Node::Big { ch, size, class }
            }
            // Accents.
            "overline" => Node::Overline(self.arg(src)),
            "underline" => Node::Underline(self.arg(src)),
            "overbrace" | "underbrace" => Node::Brace {
                body: self.arg(src),
                over: name == "overbrace",
            },
            "overset" | "underset" | "stackrel" => {
                let a = self.arg(src);
                let base = self.arg(src);
                let class = base_class(&base);
                if name == "underset" {
                    Node::Stack {
                        base,
                        over: None,
                        under: Some(a),
                        class,
                    }
                } else {
                    Node::Stack {
                        base,
                        over: Some(a),
                        under: None,
                        class,
                    }
                }
            }
            "xrightarrow" | "xleftarrow" | "xLeftarrow" | "xRightarrow" | "xleftrightarrow"
            | "xLeftrightarrow" | "xmapsto" | "xhookrightarrow" | "xhookleftarrow" => {
                let under = self.opt_arg(src);
                let over = self.arg(src);
                let ch = match name {
                    "xrightarrow" => '→',
                    "xleftarrow" => '←',
                    "xLeftarrow" => '⇐',
                    "xRightarrow" => '⇒',
                    "xleftrightarrow" => '↔',
                    "xLeftrightarrow" => '⇔',
                    "xmapsto" => '↦',
                    "xhookrightarrow" => '↪',
                    _ => '↩',
                };
                Node::Stack {
                    base: vec![Node::Accent {
                        ch,
                        body: Vec::new(),
                        stretchy: true,
                        under: false,
                    }],
                    over: Some(over),
                    under,
                    class: Class::Rel,
                }
            }
            // Fonts.
            "mathrm" | "mathbf" | "mathit" | "mathbb" | "mathcal" | "mathscr" | "mathfrak"
            | "mathsf" | "mathtt" | "boldsymbol" | "bm" | "mathbfit" | "mathnormal" | "rm"
            | "bf" | "it" | "cal" | "sf" | "tt" | "Bbb" => {
                let v = match name {
                    "mathrm" | "rm" => Variant::Upright,
                    "mathbf" | "bf" => Variant::Bold,
                    "mathit" | "it" => Variant::Italic,
                    "mathbb" | "Bbb" => Variant::DoubleStruck,
                    "mathcal" | "mathscr" | "cal" => Variant::Script,
                    "mathfrak" => Variant::Fraktur,
                    "mathsf" | "sf" => Variant::SansSerif,
                    "mathtt" | "tt" => Variant::Monospace,
                    "mathnormal" => Variant::Normal,
                    _ => Variant::BoldItalic,
                };
                let old = std::mem::replace(&mut self.variant, v);
                let n = if matches!(name, "rm" | "bf" | "it" | "cal" | "sf" | "tt") {
                    // Old-style switches: the rest of the group.
                    return Node::Group(Vec::new());
                } else {
                    Node::Group(self.arg(src))
                };
                self.variant = old;
                n
            }
            "text" | "textrm" | "textnormal" | "mbox" | "hbox" | "textup" | "textmd" | "textsf"
            | "texttt" => Node::Text {
                text: self.text_arg(src),
                bold: false,
                italic: false,
            },
            "textbf" => Node::Text {
                text: self.text_arg(src),
                bold: true,
                italic: false,
            },
            "textit" | "emph" | "textsl" => Node::Text {
                text: self.text_arg(src),
                bold: false,
                italic: true,
            },
            "operatorname" | "operatornamewithlimits" => {
                let star = self.peek() == Some(&Tok::Char('*'));
                if star {
                    self.next();
                }
                let name_text = self.text_arg(src);
                let limits = star || name == "operatornamewithlimits";
                Node::OpName {
                    name: name_text,
                    limits: None,
                    default_limits: limits,
                }
            }
            "mathop" => Node::Class {
                class: Class::Op,
                body: self.arg(src),
            },
            "mathbin" => Node::Class {
                class: Class::Bin,
                body: self.arg(src),
            },
            "mathrel" => Node::Class {
                class: Class::Rel,
                body: self.arg(src),
            },
            "mathord" => Node::Class {
                class: Class::Ord,
                body: self.arg(src),
            },
            "mathopen" => Node::Class {
                class: Class::Open,
                body: self.arg(src),
            },
            "mathclose" => Node::Class {
                class: Class::Close,
                body: self.arg(src),
            },
            "mathpunct" => Node::Class {
                class: Class::Punct,
                body: self.arg(src),
            },
            "mathinner" => Node::Class {
                class: Class::Inner,
                body: self.arg(src),
            },
            // Colors and boxes.
            "textcolor" | "colorbox" => {
                let model = self.opt_raw();
                let spec = self.raw_arg();
                let rgb = parse_color(model.as_deref(), &spec);
                if rgb.is_none() {
                    self.err(format!("unknown color {spec}"));
                }
                let body = self.arg(src);
                Node::Color {
                    rgb: rgb.unwrap_or([255, 0, 0]),
                    body,
                }
            }
            "boxed" | "fbox" => Node::Boxed(self.arg(src)),
            "phantom" => Node::Phantom {
                body: self.arg(src),
                width: true,
                height: true,
            },
            "hphantom" => Node::Phantom {
                body: self.arg(src),
                width: true,
                height: false,
            },
            "vphantom" => Node::Phantom {
                body: self.arg(src),
                width: false,
                height: true,
            },
            "smash" => {
                let _ = self.opt_raw();
                Node::Phantom {
                    body: self.arg(src),
                    width: false,
                    height: false,
                }
            }
            "not" => {
                let next = self.atom(src);
                match next {
                    Some(Node::Sym { ch, class, variant }) => {
                        match symbols::NEGATIONS.iter().find(|(a, _)| *a == ch) {
                            Some(&(_, n)) => Node::Sym {
                                ch: n,
                                class,
                                variant,
                            },
                            None => Node::Not(Box::new(Node::Sym { ch, class, variant })),
                        }
                    }
                    Some(n) => Node::Not(Box::new(n)),
                    None => self.sym('/', Class::Ord),
                }
            }
            // Modular arithmetic.
            "bmod" => Node::OpName {
                name: "mod".into(),
                limits: None,
                default_limits: false,
            },
            "pmod" | "mod" | "pod" => {
                let a = self.arg(src);
                let mut l = vec![Node::SpaceEm(if name == "mod" { 0.9 } else { 0.5 })];
                if name != "mod" {
                    l.push(self.sym('(', Class::Open));
                }
                if name != "pod" {
                    l.push(Node::Text {
                        text: "mod".into(),
                        bold: false,
                        italic: false,
                    });
                    l.push(Node::Space(6.0));
                }
                l.extend(a);
                if name != "mod" {
                    l.push(self.sym(')', Class::Close));
                }
                Node::Group(l)
            }
            // Equation metadata.
            "tag" => {
                let star = self.peek() == Some(&Tok::Char('*'));
                if star {
                    self.next();
                }
                let t = self.text_arg(src);
                self.out.tag = Some((t, star));
                Node::Group(Vec::new())
            }
            "label" => {
                let l = self.raw_arg();
                self.out.labels.push(l);
                Node::Group(Vec::new())
            }
            "notag" | "nonumber" => {
                self.out.notag = true;
                Node::Group(Vec::new())
            }
            "ref" | "eqref" => {
                let l = self.raw_arg();
                // Resolved against the document's labels (MD-008); unknown: `??`.
                let n = self
                    .refs
                    .iter()
                    .find(|(k, _)| *k == l)
                    .map_or_else(|| "??".to_owned(), |(_, v)| v.clone());
                let t = if name == "eqref" { format!("({n})") } else { n };
                Node::Text {
                    text: t,
                    bold: false,
                    italic: false,
                }
            }
            "substack" => {
                let rows = self.cells_in_group(src);
                Node::Array {
                    kind: ArrayKind::Array,
                    rows,
                    cols: vec![ColAlign::Center],
                    delims: (None, None),
                    small: true,
                    vlines: Vec::new(),
                }
            }
            "begin" => self.environment(src),
            // No-ops.
            "displaylimits" | "relax" | "nobreak" | "allowbreak" | "strut" | "mathstrut"
            | "left." | "nolinebreak" => Node::Group(Vec::new()),
            "cdotp" => self.sym('·', Class::Punct),
            "dfrac*" => Node::Group(Vec::new()),
            _ => {
                if let Some((ch, stretchy)) = symbols::ACCENTS
                    .iter()
                    .find(|(n, _, _)| *n == name)
                    .map(|&(_, c, s)| (c, s))
                {
                    return Node::Accent {
                        ch,
                        body: self.arg(src),
                        stretchy,
                        under: false,
                    };
                }
                if let Some((ch, stretchy)) = symbols::UNDER_ACCENTS
                    .iter()
                    .find(|(n, _, _)| *n == name)
                    .map(|&(_, c, s)| (c, s))
                {
                    return Node::Accent {
                        ch,
                        body: self.arg(src),
                        stretchy,
                        under: true,
                    };
                }
                self.err(format!("unsupported command \\{name}"));
                Node::Error(format!("\\{name}"))
            }
        }
    }

    /// Rows of a `{…}` group separated by `\\` (for `\substack`).
    fn cells_in_group(&mut self, src: &TextSource) -> Vec<Vec<Vec<Node>>> {
        let mut rows = Vec::new();
        if self.peek() != Some(&Tok::Open) {
            return rows;
        }
        self.next();
        let mut row = Vec::new();
        loop {
            let start = self.pos;
            // Parse a cell up to `\\`, `&` or `}`.
            let mut cell = Vec::new();
            let end = loop {
                match self.peek() {
                    None => break None,
                    Some(Tok::RowSep) => {
                        self.next();
                        break Some(true);
                    }
                    Some(Tok::Close) => {
                        self.next();
                        break Some(false);
                    }
                    Some(Tok::Amp) => {
                        self.next();
                    }
                    _ => {
                        if let Some(a) = self.atom(src) {
                            let a =
                                if matches!(self.peek(), Some(Tok::Sup | Tok::Sub | Tok::Prime(_)))
                                {
                                    self.scripts(a, src)
                                } else {
                                    a
                                };
                            cell.push(a);
                        } else if self.pos == start {
                            self.next();
                        }
                    }
                }
            };
            row.push(cell);
            rows.push(std::mem::take(&mut row));
            match end {
                Some(true) => continue,
                _ => break,
            }
        }
        rows
    }

    fn environment(&mut self, src: &TextSource) -> Node {
        let mut name = self.raw_arg();
        if self.peek() == Some(&Tok::Char('*')) {
            self.next();
            name.push('*');
        }
        let base = name.trim_end_matches('*').to_owned();
        let mut cols = Vec::new();
        let mut vlines = Vec::new();
        if base == "array" || base == "subarray" {
            let spec = self.raw_arg();
            for c in spec.chars() {
                match c {
                    'l' => cols.push(ColAlign::Left),
                    'c' => cols.push(ColAlign::Center),
                    'r' => cols.push(ColAlign::Right),
                    '|' => vlines.push(cols.len()),
                    _ => {}
                }
            }
        }
        if base == "alignat" || base == "alignedat" {
            let _ = self.raw_arg();
        }
        // Cells.
        let mut rows: Vec<Vec<Vec<Node>>> = Vec::new();
        let mut row: Vec<Vec<Node>> = Vec::new();
        loop {
            let (cell, end) = self.list(Stop::Cell, src);
            row.push(cell);
            match end {
                Ended::Amp => {}
                Ended::RowSep => {
                    // `\\[2pt]` spacing is accepted and ignored.
                    let _ = self.opt_raw();
                    rows.push(std::mem::take(&mut row));
                }
                Ended::EndEnv(n) => {
                    if n.trim_end_matches('*') != base {
                        self.err(format!("\\begin{{{name}}} ended by \\end{{{n}}}"));
                    }
                    rows.push(std::mem::take(&mut row));
                    break;
                }
                _ => {
                    self.err(format!("missing \\end{{{name}}}"));
                    rows.push(std::mem::take(&mut row));
                    break;
                }
            }
        }
        // Drop a trailing empty row (`a \\ b \\`).
        if rows.len() > 1 && rows.last().is_some_and(|r| r.iter().all(Vec::is_empty)) {
            rows.pop();
        }
        let numbered = matches!(
            base.as_str(),
            "equation" | "align" | "gather" | "multline" | "eqnarray" | "alignat"
        ) && !name.ends_with('*');
        if numbered {
            self.out.numbered_env = true;
        }
        let (kind, delims, small) = match base.as_str() {
            "matrix" => (ArrayKind::Matrix, (None, None), false),
            "pmatrix" => (ArrayKind::Matrix, (Some('('), Some(')')), false),
            "bmatrix" => (ArrayKind::Matrix, (Some('['), Some(']')), false),
            "Bmatrix" => (ArrayKind::Matrix, (Some('{'), Some('}')), false),
            "vmatrix" => (ArrayKind::Matrix, (Some('|'), Some('|')), false),
            "Vmatrix" => (ArrayKind::Matrix, (Some('‖'), Some('‖')), false),
            "smallmatrix" => (ArrayKind::Matrix, (None, None), true),
            "cases" => (ArrayKind::Cases, (Some('{'), None), false),
            "dcases" => (ArrayKind::Cases, (Some('{'), None), false),
            "rcases" => (ArrayKind::Cases, (None, Some('}')), false),
            "aligned" | "align" | "split" | "alignat" | "alignedat" | "eqnarray" | "flalign" => {
                (ArrayKind::Aligned, (None, None), false)
            }
            "gathered" | "gather" | "equation" | "multline" | "displaymath" => {
                (ArrayKind::Gathered, (None, None), false)
            }
            "array" => (ArrayKind::Array, (None, None), false),
            "subarray" => (ArrayKind::Array, (None, None), true),
            _ => {
                self.err(format!("unsupported environment {name}"));
                (ArrayKind::Matrix, (None, None), false)
            }
        };
        if matches!(base.as_str(), "equation" | "displaymath")
            && rows.len() == 1
            && rows[0].len() == 1
        {
            return Node::Group(rows.pop().and_then(|mut r| r.pop()).unwrap_or_default());
        }
        Node::Array {
            kind,
            rows,
            cols,
            delims,
            small,
            vlines,
        }
    }
}

fn set_limits(n: &mut Node, on: bool) {
    match n {
        Node::BigOp { limits, .. } | Node::OpName { limits, .. } => *limits = Some(on),
        Node::Scripts { base, .. } => set_limits(base, on),
        _ => {}
    }
}

/// The class a stacked result takes from its base (`\stackrel{?}{=}` is a Rel).
fn base_class(base: &[Node]) -> Class {
    match base {
        [Node::Sym { class, .. }] => *class,
        [Node::BigOp { .. }] => Class::Op,
        _ => Class::Ord,
    }
}

fn parse_color(model: Option<&str>, spec: &str) -> Option<[u8; 3]> {
    match model.map(str::trim) {
        Some("HTML") => symbols::color(spec),
        Some("RGB") => {
            let v: Vec<u8> = spec
                .split(',')
                .filter_map(|x| x.trim().parse().ok())
                .collect();
            (v.len() == 3).then(|| [v[0], v[1], v[2]])
        }
        Some("rgb") => {
            let v: Vec<f32> = spec
                .split(',')
                .filter_map(|x| x.trim().parse().ok())
                .collect();
            (v.len() == 3).then(|| {
                [
                    (v[0] * 255.0) as u8,
                    (v[1] * 255.0) as u8,
                    (v[2] * 255.0) as u8,
                ]
            })
        }
        Some("gray") => spec
            .trim()
            .parse::<f32>()
            .ok()
            .map(|g| [(g * 255.0) as u8; 3]),
        _ => symbols::color(spec),
    }
}

/// A TeX length in em (`3mu`, `1em`, `2pt`, `0.5ex`, `1cm`).
pub fn length_em(s: &str) -> Option<f32> {
    let s = s.trim();
    let split = s.find(|c: char| c.is_ascii_alphabetic())?;
    let (n, unit) = s.split_at(split);
    let n: f32 = n.trim().parse().ok()?;
    Some(match unit.trim() {
        "mu" => n / 18.0,
        "em" => n,
        "ex" => n * 0.43,
        "pt" => n / 10.0,
        "px" => n / 16.0,
        "mm" => n * 0.284,
        "cm" => n * 2.845,
        "in" => n * 7.227,
        _ => return None,
    })
}

fn is_cjk(c: char) -> bool {
    matches!(c as u32, 0x3000..=0x30FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xAC00..=0xD7AF | 0xF900..=0xFAFF | 0xFF00..=0xFFEF)
}

/// Text-mode arguments need the original spacing, which the tokenizer drops:
/// for every `{` token, the raw text up to its matching `}`.
struct TextSource {
    raw: Vec<Option<String>>,
}

impl TextSource {
    fn new(src: &str) -> Self {
        // Walk the source with the same tokenization rules, recording each group.
        let cs: Vec<char> = src.chars().collect();
        let mut raw = Vec::new();
        let mut stack: Vec<(usize, usize)> = Vec::new(); // (token index, char index after `{`)
        let mut tok = 0usize;
        let mut i = 0;
        while i < cs.len() {
            match cs[i] {
                '\\' => {
                    let j = i + 1;
                    if j < cs.len() && cs[j].is_ascii_alphabetic() {
                        let mut k = j;
                        while k < cs.len() && cs[k].is_ascii_alphabetic() {
                            k += 1;
                        }
                        i = k;
                    } else {
                        i = (j + 1).min(cs.len());
                    }
                    raw.push(None);
                    tok += 1;
                }
                '{' => {
                    raw.push(None);
                    stack.push((tok, i + 1));
                    tok += 1;
                    i += 1;
                }
                '}' => {
                    if let Some((t, start)) = stack.pop() {
                        let text: String = cs[start..i].iter().collect();
                        raw[t] = Some(unescape_text(&text));
                    }
                    raw.push(None);
                    tok += 1;
                    i += 1;
                }
                '%' => {
                    while i < cs.len() && cs[i] != '\n' {
                        i += 1;
                    }
                }
                '\'' => {
                    while i < cs.len() && cs[i] == '\'' {
                        i += 1;
                    }
                    raw.push(None);
                    tok += 1;
                }
                c if c.is_whitespace() => i += 1,
                _ => {
                    raw.push(None);
                    tok += 1;
                    i += 1;
                }
            }
        }
        Self { raw }
    }

    fn text_at(&self, tok: usize) -> Option<String> {
        self.raw.get(tok).cloned().flatten()
    }
}

fn unescape_text(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '\\' => match it.peek().copied() {
                Some(n) if !n.is_ascii_alphabetic() => {
                    it.next();
                    out.push(if n == ' ' || n == ',' || n == ';' {
                        ' '
                    } else {
                        n
                    });
                }
                Some(_) => {
                    // A command inside text: keep its name readable (`\alpha` → α).
                    let mut name = String::new();
                    while let Some(n) = it.peek().copied().filter(char::is_ascii_alphabetic) {
                        name.push(n);
                        it.next();
                    }
                    match symbols::symbol(&name) {
                        Some((ch, _)) => out.push(ch),
                        None if name == "quad" => out.push_str("  "),
                        None => {}
                    }
                }
                None => {}
            },
            '{' | '}' => {}
            '~' => out.push('\u{a0}'),
            c => out.push(c),
        }
    }
    out
}

/// Parses TeX math source.
pub fn parse(src: &str) -> Parsed {
    parse_with_refs(src, &[])
}

/// Parses TeX math source; `\ref{l}`/`\eqref{l}` resolve through `refs`.
pub fn parse_with_refs(src: &str, refs: &[(String, String)]) -> Parsed {
    let text = TextSource::new(src);
    let mut p = Parser {
        refs: refs.to_vec(),
        toks: tokenize(src),
        pos: 0,
        out: Parsed::default(),
        variant: Variant::Normal,
        depth: 0,
    };
    let (list, _) = p.list(Stop::End, &text);
    let mut out = p.out;
    out.list = list;
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_common_constructs() {
        let p = parse(r"\frac{a}{b} + \sqrt[3]{x^2_i} \le \sum_{k=1}^n k");
        assert!(p.errors.is_empty(), "{:?}", p.errors);
        assert!(matches!(p.list[0], Node::Frac { rule: true, .. }));
        assert!(matches!(p.list[2], Node::Sqrt { index: Some(_), .. }));
        assert!(matches!(
            &p.list[4],
            Node::BigOp { .. } | Node::Scripts { .. }
        ));
        let p = parse(r"\left( \frac{1}{2} \right]");
        assert!(matches!(
            p.list[0],
            Node::LeftRight {
                left: Some('('),
                right: Some(']'),
                ..
            }
        ));
        let p = parse(r"\begin{pmatrix} a & b \\ c & d \end{pmatrix}");
        match &p.list[0] {
            Node::Array { rows, delims, .. } => {
                assert_eq!(rows.len(), 2);
                assert_eq!(rows[0].len(), 2);
                assert_eq!(*delims, (Some('('), Some(')')));
            }
            n => panic!("{n:?}"),
        }
        let p = parse(r"\text{if } x \tag{1.2} \label{eq:a}");
        assert_eq!(p.tag, Some(("1.2".into(), false)));
        assert_eq!(p.labels, vec!["eq:a"]);
        assert!(matches!(&p.list[0], Node::Text { text, .. } if text == "if "));
        let p = parse(r"f'(x) \not= \foo");
        assert_eq!(p.errors, vec!["unsupported command \\foo".to_string()]);
        assert!(
            p.list
                .iter()
                .any(|n| matches!(n, Node::Sym { ch: '≠', .. }))
        );
        assert!(p.list.iter().any(|n| matches!(n, Node::Error(_))));
        // Never panics on garbage.
        for s in [
            "{",
            "}",
            "\\left(",
            "x^",
            "\\begin{matrix",
            "\\frac",
            "^^^",
            "\\sqrt[",
            "\\end{x}",
        ] {
            let _ = parse(s);
        }
    }

    #[test]
    fn lengths() {
        assert_eq!(length_em("18mu"), Some(1.0));
        assert_eq!(length_em("2em"), Some(2.0));
        assert_eq!(length_em("x"), None);
    }
}
