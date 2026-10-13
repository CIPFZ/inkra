//! TeX control sequences and characters: their Unicode code points and atom
//! classes (TeXbook chapter 17, `unicode-math` names).

/// TeX atom classes; they decide the spacing between neighbours.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Class {
    /// Ordinary.
    Ord,
    /// Large operator.
    Op,
    /// Binary operation.
    Bin,
    /// Relation.
    Rel,
    /// Opening delimiter.
    Open,
    /// Closing delimiter.
    Close,
    /// Punctuation.
    Punct,
    /// Inner (fractions, `\left…\right`).
    Inner,
}

use Class::*;

/// Symbols with fixed code points: `(name, char, class)`.
pub const SYMBOLS: &[(&str, char, Class)] = &[
    // Greek lowercase (italic is applied by the letter mapping).
    ("alpha", 'α', Ord),
    ("beta", 'β', Ord),
    ("gamma", 'γ', Ord),
    ("delta", 'δ', Ord),
    ("epsilon", 'ϵ', Ord),
    ("varepsilon", 'ε', Ord),
    ("zeta", 'ζ', Ord),
    ("eta", 'η', Ord),
    ("theta", 'θ', Ord),
    ("vartheta", 'ϑ', Ord),
    ("iota", 'ι', Ord),
    ("kappa", 'κ', Ord),
    ("varkappa", 'ϰ', Ord),
    ("lambda", 'λ', Ord),
    ("mu", 'μ', Ord),
    ("nu", 'ν', Ord),
    ("xi", 'ξ', Ord),
    ("omicron", 'ο', Ord),
    ("pi", 'π', Ord),
    ("varpi", 'ϖ', Ord),
    ("rho", 'ρ', Ord),
    ("varrho", 'ϱ', Ord),
    ("sigma", 'σ', Ord),
    ("varsigma", 'ς', Ord),
    ("tau", 'τ', Ord),
    ("upsilon", 'υ', Ord),
    ("phi", 'ϕ', Ord),
    ("varphi", 'φ', Ord),
    ("chi", 'χ', Ord),
    ("psi", 'ψ', Ord),
    ("omega", 'ω', Ord),
    ("digamma", 'ϝ', Ord),
    // Greek uppercase (upright).
    ("Gamma", 'Γ', Ord),
    ("Delta", 'Δ', Ord),
    ("Theta", 'Θ', Ord),
    ("Lambda", 'Λ', Ord),
    ("Xi", 'Ξ', Ord),
    ("Pi", 'Π', Ord),
    ("Sigma", 'Σ', Ord),
    ("Upsilon", 'Υ', Ord),
    ("Phi", 'Φ', Ord),
    ("Psi", 'Ψ', Ord),
    ("Omega", 'Ω', Ord),
    ("varGamma", '𝛤', Ord),
    ("varDelta", '𝛥', Ord),
    ("varTheta", '𝛩', Ord),
    ("varLambda", '𝛬', Ord),
    ("varXi", '𝛯', Ord),
    ("varPi", '𝛱', Ord),
    ("varSigma", '𝛴', Ord),
    ("varUpsilon", '𝛶', Ord),
    ("varPhi", '𝛷', Ord),
    ("varPsi", '𝛹', Ord),
    ("varOmega", '𝛺', Ord),
    // Hebrew and letter-like.
    ("aleph", 'ℵ', Ord),
    ("beth", 'ℶ', Ord),
    ("gimel", 'ℷ', Ord),
    ("daleth", 'ℸ', Ord),
    ("hbar", 'ℏ', Ord),
    ("hslash", 'ℏ', Ord),
    ("ell", 'ℓ', Ord),
    ("wp", '℘', Ord),
    ("Re", 'ℜ', Ord),
    ("Im", 'ℑ', Ord),
    ("partial", '𝜕', Ord),
    ("imath", '𝚤', Ord),
    ("jmath", '𝚥', Ord),
    ("mho", '℧', Ord),
    ("Finv", 'Ⅎ', Ord),
    ("Game", '⅁', Ord),
    ("eth", 'ð', Ord),
    // Miscellaneous ordinary symbols.
    ("infty", '∞', Ord),
    ("nabla", '∇', Ord),
    ("forall", '∀', Ord),
    ("exists", '∃', Ord),
    ("nexists", '∄', Ord),
    ("emptyset", '∅', Ord),
    ("varnothing", '∅', Ord),
    ("neg", '¬', Ord),
    ("lnot", '¬', Ord),
    ("angle", '∠', Ord),
    ("measuredangle", '∡', Ord),
    ("sphericalangle", '∢', Ord),
    ("triangle", '△', Ord),
    ("vartriangle", '△', Ord),
    ("triangledown", '▽', Ord),
    ("blacktriangle", '▲', Ord),
    ("blacktriangledown", '▼', Ord),
    ("prime", '′', Ord),
    ("backprime", '‵', Ord),
    ("top", '⊤', Ord),
    ("bot", '⊥', Ord),
    ("flat", '♭', Ord),
    ("natural", '♮', Ord),
    ("sharp", '♯', Ord),
    ("clubsuit", '♣', Ord),
    ("diamondsuit", '♢', Ord),
    ("heartsuit", '♡', Ord),
    ("spadesuit", '♠', Ord),
    ("degree", '°', Ord),
    ("checkmark", '✓', Ord),
    ("Box", '□', Ord),
    ("square", '□', Ord),
    ("blacksquare", '■', Ord),
    ("Diamond", '◇', Ord),
    ("lozenge", '◊', Ord),
    ("blacklozenge", '⧫', Ord),
    ("bigstar", '★', Ord),
    ("surd", '√', Ord),
    ("S", '§', Ord),
    ("P", '¶', Ord),
    ("copyright", '©', Ord),
    ("pounds", '£', Ord),
    ("yen", '¥', Ord),
    ("euro", '€', Ord),
    ("complement", '∁', Ord),
    ("therefore", '∴', Rel),
    ("because", '∵', Rel),
    ("dots", '…', Inner),
    ("ldots", '…', Inner),
    ("cdots", '⋯', Inner),
    ("dotsc", '…', Inner),
    ("dotsb", '⋯', Inner),
    ("dotsm", '⋯', Inner),
    ("dotsi", '⋯', Inner),
    ("vdots", '⋮', Ord),
    ("ddots", '⋱', Inner),
    ("iddots", '⋰', Inner),
    ("colon", ':', Punct),
    ("cdotp", '·', Punct),
    ("ldotp", '.', Punct),
    ("backslash", '\\', Ord),
    ("smallsetminus", '∖', Bin),
    // Binary operators.
    ("pm", '±', Bin),
    ("mp", '∓', Bin),
    ("times", '×', Bin),
    ("div", '÷', Bin),
    ("cdot", '⋅', Bin),
    ("ast", '∗', Bin),
    ("star", '⋆', Bin),
    ("circ", '∘', Bin),
    ("bullet", '∙', Bin),
    ("cap", '∩', Bin),
    ("cup", '∪', Bin),
    ("uplus", '⊎', Bin),
    ("sqcap", '⊓', Bin),
    ("sqcup", '⊔', Bin),
    ("vee", '∨', Bin),
    ("lor", '∨', Bin),
    ("wedge", '∧', Bin),
    ("land", '∧', Bin),
    ("setminus", '∖', Bin),
    ("oplus", '⊕', Bin),
    ("ominus", '⊖', Bin),
    ("otimes", '⊗', Bin),
    ("oslash", '⊘', Bin),
    ("odot", '⊙', Bin),
    ("circledcirc", '⊚', Bin),
    ("circleddash", '⊝', Bin),
    ("circledast", '⊛', Bin),
    ("boxplus", '⊞', Bin),
    ("boxminus", '⊟', Bin),
    ("boxtimes", '⊠', Bin),
    ("boxdot", '⊡', Bin),
    ("dagger", '†', Bin),
    ("ddagger", '‡', Bin),
    ("amalg", '⨿', Bin),
    ("wr", '≀', Bin),
    ("diamond", '⋄', Bin),
    ("bigtriangleup", '△', Bin),
    ("bigtriangledown", '▽', Bin),
    ("triangleleft", '◁', Bin),
    ("triangleright", '▷', Bin),
    ("lhd", '⊲', Bin),
    ("rhd", '⊳', Bin),
    ("unlhd", '⊴', Bin),
    ("unrhd", '⊵', Bin),
    ("ltimes", '⋉', Bin),
    ("rtimes", '⋊', Bin),
    ("leftthreetimes", '⋋', Bin),
    ("rightthreetimes", '⋌', Bin),
    ("curlywedge", '⋏', Bin),
    ("curlyvee", '⋎', Bin),
    ("dotplus", '∔', Bin),
    ("divideontimes", '⋇', Bin),
    ("intercal", '⊺', Bin),
    ("barwedge", '⊼', Bin),
    ("veebar", '⊻', Bin),
    ("doublebarwedge", '⩞', Bin),
    ("Cap", '⋒', Bin),
    ("Cup", '⋓', Bin),
    ("centerdot", '⋅', Bin),
    // Relations.
    ("le", '≤', Rel),
    ("leq", '≤', Rel),
    ("ge", '≥', Rel),
    ("geq", '≥', Rel),
    ("ne", '≠', Rel),
    ("neq", '≠', Rel),
    ("equiv", '≡', Rel),
    ("sim", '∼', Rel),
    ("simeq", '≃', Rel),
    ("approx", '≈', Rel),
    ("approxeq", '≊', Rel),
    ("cong", '≅', Rel),
    ("ncong", '≇', Rel),
    ("propto", '∝', Rel),
    ("varpropto", '∝', Rel),
    ("ll", '≪', Rel),
    ("gg", '≫', Rel),
    ("lll", '⋘', Rel),
    ("ggg", '⋙', Rel),
    ("subset", '⊂', Rel),
    ("supset", '⊃', Rel),
    ("subseteq", '⊆', Rel),
    ("supseteq", '⊇', Rel),
    ("subsetneq", '⊊', Rel),
    ("supsetneq", '⊋', Rel),
    ("nsubseteq", '⊈', Rel),
    ("nsupseteq", '⊉', Rel),
    ("Subset", '⋐', Rel),
    ("Supset", '⋑', Rel),
    ("sqsubset", '⊏', Rel),
    ("sqsupset", '⊐', Rel),
    ("sqsubseteq", '⊑', Rel),
    ("sqsupseteq", '⊒', Rel),
    ("in", '∈', Rel),
    ("notin", '∉', Rel),
    ("ni", '∋', Rel),
    ("owns", '∋', Rel),
    ("mid", '∣', Rel),
    ("nmid", '∤', Rel),
    ("parallel", '∥', Rel),
    ("nparallel", '∦', Rel),
    ("perp", '⟂', Rel),
    ("models", '⊨', Rel),
    ("vdash", '⊢', Rel),
    ("dashv", '⊣', Rel),
    ("vDash", '⊨', Rel),
    ("Vdash", '⊩', Rel),
    ("nvdash", '⊬', Rel),
    ("nvDash", '⊭', Rel),
    ("prec", '≺', Rel),
    ("succ", '≻', Rel),
    ("preceq", '⪯', Rel),
    ("succeq", '⪰', Rel),
    ("precsim", '≾', Rel),
    ("succsim", '≿', Rel),
    ("doteq", '≐', Rel),
    ("doteqdot", '≑', Rel),
    ("asymp", '≍', Rel),
    ("bowtie", '⋈', Rel),
    ("Join", '⨝', Rel),
    ("smile", '⌣', Rel),
    ("frown", '⌢', Rel),
    ("leqslant", '⩽', Rel),
    ("geqslant", '⩾', Rel),
    ("leqq", '≦', Rel),
    ("geqq", '≧', Rel),
    ("lesssim", '≲', Rel),
    ("gtrsim", '≳', Rel),
    ("lessapprox", '⪅', Rel),
    ("gtrapprox", '⪆', Rel),
    ("lessgtr", '≶', Rel),
    ("gtrless", '≷', Rel),
    ("nless", '≮', Rel),
    ("ngtr", '≯', Rel),
    ("nleq", '≰', Rel),
    ("ngeq", '≱', Rel),
    ("lneq", '⪇', Rel),
    ("gneq", '⪈', Rel),
    ("nsim", '≁', Rel),
    ("coloneqq", '≔', Rel),
    ("eqqcolon", '≕', Rel),
    ("coloneq", '≔', Rel),
    ("triangleq", '≜', Rel),
    ("circeq", '≗', Rel),
    ("bumpeq", '≏', Rel),
    ("Bumpeq", '≎', Rel),
    ("risingdotseq", '≓', Rel),
    ("fallingdotseq", '≒', Rel),
    ("eqcirc", '≖', Rel),
    ("trianglelefteq", '⊴', Rel),
    ("trianglerighteq", '⊵', Rel),
    ("vartriangleleft", '⊲', Rel),
    ("vartriangleright", '⊳', Rel),
    ("pitchfork", '⋔', Rel),
    ("between", '≬', Rel),
    ("multimap", '⊸', Rel),
    // Arrows.
    ("to", '→', Rel),
    ("rightarrow", '→', Rel),
    ("leftarrow", '←', Rel),
    ("gets", '←', Rel),
    ("leftrightarrow", '↔', Rel),
    ("Rightarrow", '⇒', Rel),
    ("Leftarrow", '⇐', Rel),
    ("Leftrightarrow", '⇔', Rel),
    ("implies", '⟹', Rel),
    ("impliedby", '⟸', Rel),
    ("iff", '⟺', Rel),
    ("mapsto", '↦', Rel),
    ("longmapsto", '⟼', Rel),
    ("longrightarrow", '⟶', Rel),
    ("longleftarrow", '⟵', Rel),
    ("longleftrightarrow", '⟷', Rel),
    ("Longrightarrow", '⟹', Rel),
    ("Longleftarrow", '⟸', Rel),
    ("Longleftrightarrow", '⟺', Rel),
    ("uparrow", '↑', Rel),
    ("downarrow", '↓', Rel),
    ("updownarrow", '↕', Rel),
    ("Uparrow", '⇑', Rel),
    ("Downarrow", '⇓', Rel),
    ("Updownarrow", '⇕', Rel),
    ("nearrow", '↗', Rel),
    ("searrow", '↘', Rel),
    ("swarrow", '↙', Rel),
    ("nwarrow", '↖', Rel),
    ("hookrightarrow", '↪', Rel),
    ("hookleftarrow", '↩', Rel),
    ("rightharpoonup", '⇀', Rel),
    ("rightharpoondown", '⇁', Rel),
    ("leftharpoonup", '↼', Rel),
    ("leftharpoondown", '↽', Rel),
    ("rightleftharpoons", '⇌', Rel),
    ("leftrightharpoons", '⇋', Rel),
    ("leadsto", '⇝', Rel),
    ("rightsquigarrow", '⇝', Rel),
    ("nrightarrow", '↛', Rel),
    ("nleftarrow", '↚', Rel),
    ("nRightarrow", '⇏', Rel),
    ("nLeftarrow", '⇍', Rel),
    ("nleftrightarrow", '↮', Rel),
    ("nLeftrightarrow", '⇎', Rel),
    ("twoheadrightarrow", '↠', Rel),
    ("twoheadleftarrow", '↞', Rel),
    ("rightrightarrows", '⇉', Rel),
    ("leftleftarrows", '⇇', Rel),
    ("rightleftarrows", '⇄', Rel),
    ("leftrightarrows", '⇆', Rel),
    ("upuparrows", '⇈', Rel),
    ("downdownarrows", '⇊', Rel),
    ("curvearrowright", '↷', Rel),
    ("curvearrowleft", '↶', Rel),
    ("circlearrowright", '↻', Rel),
    ("circlearrowleft", '↺', Rel),
    ("Lsh", '↰', Rel),
    ("Rsh", '↱', Rel),
    ("looparrowright", '↬', Rel),
    ("looparrowleft", '↫', Rel),
    ("rightarrowtail", '↣', Rel),
    ("leftarrowtail", '↢', Rel),
    ("Rrightarrow", '⇛', Rel),
    ("Lleftarrow", '⇚', Rel),
    ("longleftrightarrows", '⟺', Rel),
    // Punctuation.
    ("ldotp", '.', Punct),
];

/// Large operators: `(name, char, limits in display style)`.
pub const OPERATORS: &[(&str, char, bool)] = &[
    ("sum", '∑', true),
    ("prod", '∏', true),
    ("coprod", '∐', true),
    ("int", '∫', false),
    ("iint", '∬', false),
    ("iiint", '∭', false),
    ("iiiint", '⨌', false),
    ("oint", '∮', false),
    ("oiint", '∯', false),
    ("oiiint", '∰', false),
    ("intop", '∫', true),
    ("smallint", '∫', false),
    ("bigcup", '⋃', true),
    ("bigcap", '⋂', true),
    ("bigvee", '⋁', true),
    ("bigwedge", '⋀', true),
    ("bigoplus", '⨁', true),
    ("bigotimes", '⨂', true),
    ("bigodot", '⨀', true),
    ("biguplus", '⨄', true),
    ("bigsqcup", '⨆', true),
];

/// Named functions (upright text, operator spacing): `(name, limits in display)`.
pub const FUNCTIONS: &[(&str, bool)] = &[
    ("sin", false),
    ("cos", false),
    ("tan", false),
    ("cot", false),
    ("sec", false),
    ("csc", false),
    ("arcsin", false),
    ("arccos", false),
    ("arctan", false),
    ("arccot", false),
    ("sinh", false),
    ("cosh", false),
    ("tanh", false),
    ("coth", false),
    ("sech", false),
    ("csch", false),
    ("log", false),
    ("ln", false),
    ("lg", false),
    ("exp", false),
    ("dim", false),
    ("deg", false),
    ("ker", false),
    ("hom", false),
    ("arg", false),
    ("sgn", false),
    ("tr", false),
    ("Tr", false),
    ("rank", false),
    ("det", true),
    ("gcd", true),
    ("lcm", false),
    ("min", true),
    ("max", true),
    ("sup", true),
    ("inf", true),
    ("lim", true),
    ("liminf", true),
    ("limsup", true),
    ("Pr", true),
    ("argmax", true),
    ("argmin", true),
];

/// Delimiters usable with `\left`, `\right`, `\big…`: `(name, char)`.
pub const DELIMITERS: &[(&str, char)] = &[
    ("lbrace", '{'),
    ("rbrace", '}'),
    ("{", '{'),
    ("}", '}'),
    ("langle", '⟨'),
    ("rangle", '⟩'),
    ("lvert", '|'),
    ("rvert", '|'),
    ("vert", '|'),
    ("|", '‖'),
    ("Vert", '‖'),
    ("lVert", '‖'),
    ("rVert", '‖'),
    ("lfloor", '⌊'),
    ("rfloor", '⌋'),
    ("lceil", '⌈'),
    ("rceil", '⌉'),
    ("lbrack", '['),
    ("rbrack", ']'),
    ("lgroup", '⟮'),
    ("rgroup", '⟯'),
    ("lmoustache", '⎰'),
    ("rmoustache", '⎱'),
    ("llbracket", '⟦'),
    ("rrbracket", '⟧'),
    ("uparrow", '↑'),
    ("downarrow", '↓'),
    ("updownarrow", '↕'),
    ("Uparrow", '⇑'),
    ("Downarrow", '⇓'),
    ("Updownarrow", '⇕'),
    ("backslash", '\\'),
];

/// Accents: `(name, combining char, stretchy)`.
pub const ACCENTS: &[(&str, char, bool)] = &[
    ("hat", '\u{302}', false),
    ("widehat", '\u{302}', true),
    ("tilde", '\u{303}', false),
    ("widetilde", '\u{303}', true),
    ("bar", '\u{304}', false),
    ("vec", '\u{20D7}', false),
    ("dot", '\u{307}', false),
    ("ddot", '\u{308}', false),
    ("dddot", '\u{20DB}', false),
    ("ddddot", '\u{20DC}', false),
    ("acute", '\u{301}', false),
    ("grave", '\u{300}', false),
    ("breve", '\u{306}', false),
    ("check", '\u{30C}', false),
    ("widecheck", '\u{30C}', true),
    ("mathring", '\u{30A}', false),
    ("overrightarrow", '→', true),
    ("overleftarrow", '←', true),
    ("overleftrightarrow", '↔', true),
    ("Overrightarrow", '⇒', true),
    ("overgroup", '⏠', true),
    ("overparen", '⏜', true),
];

/// Accents placed below: `(name, char, stretchy)`.
pub const UNDER_ACCENTS: &[(&str, char, bool)] = &[
    ("underrightarrow", '→', true),
    ("underleftarrow", '←', true),
    ("underleftrightarrow", '↔', true),
    ("undergroup", '⏡', true),
    ("underparen", '⏝', true),
];

/// Named colors (`\color{…}`), `xcolor` base names plus common `dvipsnames`.
pub const COLORS: &[(&str, [u8; 3])] = &[
    ("black", [0, 0, 0]),
    ("white", [255, 255, 255]),
    ("red", [255, 0, 0]),
    ("green", [0, 128, 0]),
    ("blue", [0, 0, 255]),
    ("cyan", [0, 255, 255]),
    ("magenta", [255, 0, 255]),
    ("yellow", [255, 255, 0]),
    ("gray", [128, 128, 128]),
    ("grey", [128, 128, 128]),
    ("darkgray", [64, 64, 64]),
    ("lightgray", [191, 191, 191]),
    ("brown", [191, 128, 64]),
    ("lime", [191, 255, 0]),
    ("olive", [128, 128, 0]),
    ("orange", [255, 128, 0]),
    ("pink", [255, 191, 191]),
    ("purple", [191, 0, 64]),
    ("teal", [0, 128, 128]),
    ("violet", [128, 0, 128]),
    ("navy", [0, 0, 128]),
    ("maroon", [128, 0, 0]),
    ("gold", [255, 215, 0]),
    ("silver", [192, 192, 192]),
    ("orangered", [255, 69, 0]),
    ("forestgreen", [34, 139, 34]),
    ("royalblue", [65, 105, 225]),
    ("skyblue", [135, 206, 235]),
    ("crimson", [220, 20, 60]),
    ("salmon", [250, 128, 114]),
    ("aqua", [0, 255, 255]),
    ("fuchsia", [255, 0, 255]),
];

/// `\not` followed by a symbol: precomposed negations.
pub const NEGATIONS: &[(char, char)] = &[
    ('=', '≠'),
    ('<', '≮'),
    ('>', '≯'),
    ('≤', '≰'),
    ('≥', '≱'),
    ('≡', '≢'),
    ('∼', '≁'),
    ('≃', '≄'),
    ('≈', '≉'),
    ('≅', '≇'),
    ('∈', '∉'),
    ('∋', '∌'),
    ('⊂', '⊄'),
    ('⊃', '⊅'),
    ('⊆', '⊈'),
    ('⊇', '⊉'),
    ('∣', '∤'),
    ('∥', '∦'),
    ('≺', '⊀'),
    ('≻', '⊁'),
    ('→', '↛'),
    ('←', '↚'),
    ('⇒', '⇏'),
    ('⇐', '⇍'),
    ('↔', '↮'),
    ('⇔', '⇎'),
    ('⊢', '⊬'),
    ('⊨', '⊭'),
    ('∃', '∄'),
];

/// Looks up a symbol command.
pub fn symbol(name: &str) -> Option<(char, Class)> {
    SYMBOLS
        .iter()
        .find(|(n, _, _)| *n == name)
        .map(|&(_, c, k)| (c, k))
}

/// Looks up a large operator.
pub fn operator(name: &str) -> Option<(char, bool)> {
    OPERATORS
        .iter()
        .find(|(n, _, _)| *n == name)
        .map(|&(_, c, l)| (c, l))
}

/// Looks up a named function.
pub fn function(name: &str) -> Option<bool> {
    FUNCTIONS.iter().find(|(n, _)| *n == name).map(|&(_, l)| l)
}

/// Looks up a delimiter command.
pub fn delimiter(name: &str) -> Option<char> {
    DELIMITERS.iter().find(|(n, _)| *n == name).map(|&(_, c)| c)
}

/// The class of a literal character in math mode.
pub fn char_class(c: char) -> Class {
    match c {
        '+' | '-' | '*' | '−' => Bin,
        '=' | '<' | '>' | ':' => Rel,
        '(' | '[' | '{' | '⟨' | '⌊' | '⌈' => Open,
        ')' | ']' | '}' | '⟩' | '⌋' | '⌉' | '!' | '?' => Close,
        ',' | ';' => Punct,
        '.' | '/' | '|' | '@' | '#' | '"' => Ord,
        _ => {
            if let Some(&(_, _, k)) = SYMBOLS.iter().find(|(_, s, _)| *s == c) {
                k
            } else if OPERATORS.iter().any(|(_, s, _)| *s == c) {
                Op
            } else {
                Ord
            }
        }
    }
}

/// Parses a color: a name, `#rgb`, `#rrggbb`, or `RRGGBB` (with `[HTML]`).
pub fn color(spec: &str) -> Option<[u8; 3]> {
    let s = spec.trim();
    let hex = s.strip_prefix('#').unwrap_or(s);
    let h = |i: usize, n: usize| u8::from_str_radix(&hex[i..i + n], 16).ok();
    if hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return Some([h(0, 2)?, h(2, 2)?, h(4, 2)?]);
    }
    if s.starts_with('#') && hex.len() == 3 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return Some([h(0, 1)? * 17, h(1, 1)? * 17, h(2, 1)? * 17]);
    }
    let lower = s.to_ascii_lowercase();
    COLORS.iter().find(|(n, _)| *n == lower).map(|(_, c)| *c)
}

/// Letter styles of `\mathbf`, `\mathbb` & co.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Variant {
    /// Default math: Latin and lowercase Greek italic.
    #[default]
    Normal,
    /// Upright (`\mathrm`, `\text`).
    Upright,
    /// Italic (`\mathit`).
    Italic,
    /// Bold (`\mathbf`).
    Bold,
    /// Bold italic (`\boldsymbol`, `\bm`).
    BoldItalic,
    /// Double-struck (`\mathbb`).
    DoubleStruck,
    /// Script (`\mathcal`, `\mathscr`).
    Script,
    /// Fraktur (`\mathfrak`).
    Fraktur,
    /// Sans-serif (`\mathsf`).
    SansSerif,
    /// Monospace (`\mathtt`).
    Monospace,
}

fn offset(base: u32, i: u32) -> char {
    char::from_u32(base + i).unwrap_or('?')
}

const GREEK_LOWER: &str = "αβγδεζηθικλμνξοπρςστυφχψω";

/// Maps a character to its styled Mathematical Alphanumeric Symbol.
pub fn styled(c: char, v: Variant) -> char {
    use Variant::*;
    let upper = c.is_ascii_uppercase();
    let lower = c.is_ascii_lowercase();
    let digit = c.is_ascii_digit();
    let li = if upper {
        c as u32 - 'A' as u32
    } else if lower {
        c as u32 - 'a' as u32 + 26
    } else {
        0
    };
    let di = if digit { c as u32 - '0' as u32 } else { 0 };
    match v {
        Upright => c,
        Normal | Italic => {
            if c == 'h' {
                'ℎ'
            } else if upper || lower {
                offset(0x1D434, li)
            } else if let Some(i) = GREEK_LOWER.chars().position(|g| g == c) {
                offset(0x1D6FC, i as u32)
            } else {
                match c {
                    'ϵ' => '𝜖',
                    'ϑ' => '𝜗',
                    'ϰ' => '𝜘',
                    'ϕ' => '𝜙',
                    'ϱ' => '𝜚',
                    'ϖ' => '𝜛',
                    'ı' => '𝚤',
                    'ȷ' => '𝚥',
                    _ if v == Italic && ('Α'..='Ω').contains(&c) => {
                        offset(0x1D6E2, c as u32 - 'Α' as u32)
                    }
                    _ => c,
                }
            }
        }
        Bold => {
            if upper || lower {
                offset(0x1D400, li)
            } else if digit {
                offset(0x1D7CE, di)
            } else if let Some(i) = GREEK_LOWER.chars().position(|g| g == c) {
                offset(0x1D6C2, i as u32)
            } else if ('Α'..='Ω').contains(&c) {
                offset(0x1D6A8, c as u32 - 'Α' as u32)
            } else {
                c
            }
        }
        BoldItalic => {
            if upper || lower {
                offset(0x1D468, li)
            } else if digit {
                offset(0x1D7CE, di)
            } else if let Some(i) = GREEK_LOWER.chars().position(|g| g == c) {
                offset(0x1D736, i as u32)
            } else if ('Α'..='Ω').contains(&c) {
                offset(0x1D71C, c as u32 - 'Α' as u32)
            } else {
                c
            }
        }
        DoubleStruck => match c {
            'C' => 'ℂ',
            'H' => 'ℍ',
            'N' => 'ℕ',
            'P' => 'ℙ',
            'Q' => 'ℚ',
            'R' => 'ℝ',
            'Z' => 'ℤ',
            _ if upper || lower => offset(0x1D538, li),
            _ if digit => offset(0x1D7D8, di),
            _ => c,
        },
        Script => match c {
            'B' => 'ℬ',
            'E' => 'ℰ',
            'F' => 'ℱ',
            'H' => 'ℋ',
            'I' => 'ℐ',
            'L' => 'ℒ',
            'M' => 'ℳ',
            'R' => 'ℛ',
            'e' => 'ℯ',
            'g' => 'ℊ',
            'o' => 'ℴ',
            _ if upper || lower => offset(0x1D49C, li),
            _ => c,
        },
        Fraktur => match c {
            'C' => 'ℭ',
            'H' => 'ℌ',
            'I' => 'ℑ',
            'R' => 'ℜ',
            'Z' => 'ℨ',
            _ if upper || lower => offset(0x1D504, li),
            _ => c,
        },
        SansSerif => {
            if upper || lower {
                offset(0x1D5A0, li)
            } else if digit {
                offset(0x1D7E2, di)
            } else {
                c
            }
        }
        Monospace => {
            if upper || lower {
                offset(0x1D670, li)
            } else if digit {
                offset(0x1D7F6, di)
            } else {
                c
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn styles_map_to_math_alphanumerics() {
        assert_eq!(styled('x', Variant::Normal), '𝑥');
        assert_eq!(styled('h', Variant::Normal), 'ℎ');
        assert_eq!(styled('A', Variant::Normal), '𝐴');
        assert_eq!(styled('α', Variant::Normal), '𝛼');
        assert_eq!(styled('R', Variant::DoubleStruck), 'ℝ');
        assert_eq!(styled('A', Variant::DoubleStruck), '𝔸');
        assert_eq!(styled('L', Variant::Script), 'ℒ');
        assert_eq!(styled('A', Variant::Script), '𝒜');
        assert_eq!(styled('x', Variant::Bold), '𝐱');
        assert_eq!(styled('2', Variant::Bold), '𝟐');
        assert_eq!(styled('g', Variant::Fraktur), '𝔤');
        assert_eq!(styled('1', Variant::Normal), '1');
        assert_eq!(char_class('='), Class::Rel);
        assert_eq!(char_class('≤'), Class::Rel);
        assert_eq!(color("red"), Some([255, 0, 0]));
        assert_eq!(color("#00ff00"), Some([0, 255, 0]));
        assert_eq!(color("FF8000"), Some([255, 128, 0]));
    }
}
