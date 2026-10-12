//! # inkra-highlight
//!
//! **Layer L2** (see SPEC 2.2).
//!
//! Code highlighting with syntect (MD-007): a language alias table, syntax
//! definitions loaded lazily on a background thread (AD-5), and a cache of
//! highlighted blocks. Until the definitions are loaded, callers render plain
//! text and repaint when [`generation`] changes.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use syntect::easy::HighlightLines;
use syntect::highlighting::{FontStyle, Theme, ThemeSet};
use syntect::parsing::SyntaxSet;

/// An RGBA color.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rgba(pub [u8; 4]);

/// A highlighted range of a code block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    /// Byte range in the code.
    pub range: Range<usize>,
    /// Foreground color.
    pub color: Rgba,
    /// Bold.
    pub bold: bool,
    /// Italic.
    pub italic: bool,
}

/// Highlighted code: spans for every line, in order.
pub type Highlighted = Arc<Vec<Span>>;

struct Assets {
    syntaxes: SyntaxSet,
    themes: ThemeSet,
}

static ASSETS: OnceLock<Assets> = OnceLock::new();
static LOADING: AtomicBool = AtomicBool::new(false);
static GENERATION: AtomicU64 = AtomicU64::new(0);
static CACHE: OnceLock<Mutex<HashMap<u64, Highlighted>>> = OnceLock::new();
type Waker = Box<dyn Fn() + Send + Sync>;
static WAKE: OnceLock<Mutex<Option<Waker>>> = OnceLock::new();

/// Incremented when the syntax definitions finish loading.
pub fn generation() -> u64 {
    GENERATION.load(Ordering::Acquire)
}

/// Registers a callback run (on the loader thread) when definitions are ready,
/// e.g. `ctx.request_repaint()`.
pub fn on_ready(f: impl Fn() + Send + Sync + 'static) {
    *WAKE
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = Some(Box::new(f));
}

/// Starts loading syntax definitions in the background (idempotent).
pub fn preload() {
    if ASSETS.get().is_some() || LOADING.swap(true, Ordering::AcqRel) {
        return;
    }
    let spawned = std::thread::Builder::new()
        .name("inkra-syntect".into())
        .spawn(|| {
            let t = std::time::Instant::now();
            let assets = Assets {
                syntaxes: SyntaxSet::load_defaults_newlines(),
                themes: ThemeSet::load_defaults(),
            };
            let _ = ASSETS.set(assets);
            GENERATION.fetch_add(1, Ordering::AcqRel);
            log::info!(
                "syntax definitions loaded in {:.0} ms",
                t.elapsed().as_secs_f64() * 1e3
            );
            if let Some(w) = WAKE.get().and_then(|m| m.lock().ok())
                && let Some(f) = w.as_ref()
            {
                f();
            }
        });
    if let Err(e) = spawned {
        log::warn!("cannot start syntax loader: {e}");
    }
}

/// Loads definitions synchronously (tests, CLI export).
pub fn load_now() {
    if ASSETS.get().is_none() {
        let _ = ASSETS.set(Assets {
            syntaxes: SyntaxSet::load_defaults_newlines(),
            themes: ThemeSet::load_defaults(),
        });
        GENERATION.fetch_add(1, Ordering::AcqRel);
    }
}

/// Language alias table (MD-007): info-string words → syntect syntax names.
const ALIASES: &[(&str, &str)] = &[
    ("js", "JavaScript"),
    ("javascript", "JavaScript"),
    ("jsx", "JavaScript"),
    ("mjs", "JavaScript"),
    ("ts", "JavaScript"),
    ("typescript", "JavaScript"),
    ("tsx", "JavaScript"),
    ("py", "Python"),
    ("python", "Python"),
    ("python3", "Python"),
    ("rb", "Ruby"),
    ("ruby", "Ruby"),
    ("sh", "Bourne Again Shell (bash)"),
    ("bash", "Bourne Again Shell (bash)"),
    ("shell", "Bourne Again Shell (bash)"),
    ("zsh", "Bourne Again Shell (bash)"),
    ("console", "Bourne Again Shell (bash)"),
    ("c", "C"),
    ("h", "C"),
    ("cpp", "C++"),
    ("c++", "C++"),
    ("cc", "C++"),
    ("hpp", "C++"),
    ("cxx", "C++"),
    ("cs", "C#"),
    ("csharp", "C#"),
    ("c#", "C#"),
    ("java", "Java"),
    ("kt", "Java"),
    ("kotlin", "Java"),
    ("go", "Go"),
    ("golang", "Go"),
    ("rs", "Rust"),
    ("rust", "Rust"),
    ("swift", "Objective-C"),
    ("objc", "Objective-C"),
    ("objective-c", "Objective-C"),
    ("php", "PHP"),
    ("html", "HTML"),
    ("htm", "HTML"),
    ("xml", "XML"),
    ("svg", "XML"),
    ("xhtml", "HTML"),
    ("vue", "HTML"),
    ("css", "CSS"),
    ("scss", "CSS"),
    ("less", "CSS"),
    ("sass", "CSS"),
    ("json", "JSON"),
    ("jsonc", "JSON"),
    ("json5", "JSON"),
    ("yaml", "YAML"),
    ("yml", "YAML"),
    ("toml", "YAML"),
    ("sql", "SQL"),
    ("mysql", "SQL"),
    ("postgresql", "SQL"),
    ("sqlite", "SQL"),
    ("md", "Markdown"),
    ("markdown", "Markdown"),
    ("diff", "Diff"),
    ("patch", "Diff"),
    ("dockerfile", "Bourne Again Shell (bash)"),
    ("docker", "Bourne Again Shell (bash)"),
    ("makefile", "Makefile"),
    ("make", "Makefile"),
    ("mk", "Makefile"),
    ("latex", "LaTeX"),
    ("tex", "LaTeX"),
    ("r", "R"),
    ("matlab", "MATLAB"),
    ("octave", "MATLAB"),
    ("lua", "Lua"),
    ("perl", "Perl"),
    ("pl", "Perl"),
    ("dart", "Java"),
    ("scala", "Scala"),
    ("sbt", "Scala"),
    ("haskell", "Haskell"),
    ("hs", "Haskell"),
    ("elixir", "Ruby"),
    ("ex", "Ruby"),
    ("erlang", "Erlang"),
    ("erl", "Erlang"),
    ("clojure", "Clojure"),
    ("clj", "Clojure"),
    ("lisp", "Lisp"),
    ("elisp", "Lisp"),
    ("scheme", "Lisp"),
    ("ocaml", "OCaml"),
    ("ml", "OCaml"),
    ("fsharp", "OCaml"),
    ("fs", "OCaml"),
    ("powershell", "Bourne Again Shell (bash)"),
    ("ps1", "Bourne Again Shell (bash)"),
    ("pwsh", "Bourne Again Shell (bash)"),
    ("bat", "Batch File"),
    ("batch", "Batch File"),
    ("cmd", "Batch File"),
    ("ini", "YAML"),
    ("conf", "YAML"),
    ("cfg", "YAML"),
    ("nginx", "YAML"),
    ("graphql", "JavaScript"),
    ("gql", "JavaScript"),
    ("proto", "C++"),
    ("protobuf", "C++"),
    ("gas", "Plain Text"),
    ("asm", "Plain Text"),
    ("nasm", "Plain Text"),
    ("url", "Plain Text"),
    ("text", "Plain Text"),
    ("txt", "Plain Text"),
    ("plain", "Plain Text"),
    ("groovy", "Groovy"),
    ("gradle", "Groovy"),
    ("d", "D"),
    ("pascal", "Pascal"),
    ("tcl", "Tcl"),
    ("jsp", "Java Server Page (JSP)"),
    ("asp", "ASP"),
    ("erb", "HTML (Rails)"),
    ("rd", "Rd (R Documentation)"),
    ("dot", "Graphviz (DOT)"),
    ("graphviz", "Graphviz (DOT)"),
    ("bibtex", "BibTeX"),
    ("regex", "Regular Expression"),
    ("textile", "Textile"),
    ("rst", "reStructuredText"),
    ("restructuredtext", "reStructuredText"),
];

/// The syntax name for an info-string language word, if known.
pub fn resolve_lang(lang: &str) -> Option<&'static str> {
    let l = lang.trim().to_ascii_lowercase();
    ALIASES.iter().find(|(a, _)| *a == l).map(|(_, n)| *n)
}

/// All known language words (for the code block language picker, ED-105).
pub fn languages() -> Vec<&'static str> {
    let mut v: Vec<&'static str> = ALIASES.iter().map(|(a, _)| *a).collect();
    v.sort_unstable();
    v.dedup();
    v
}

fn find_syntax<'a>(ss: &'a SyntaxSet, lang: &str) -> Option<&'a syntect::parsing::SyntaxReference> {
    let name = resolve_lang(lang);
    name.and_then(|n| ss.find_syntax_by_name(n))
        .or_else(|| ss.find_syntax_by_token(lang))
        .or_else(|| ss.find_syntax_by_extension(lang))
}

fn theme<'a>(ts: &'a ThemeSet, name: &str) -> Option<&'a Theme> {
    ts.themes
        .get(name)
        .or_else(|| ts.themes.get("InspiredGitHub"))
}

/// Names of the built-in syntect themes.
pub fn theme_names() -> Vec<String> {
    ASSETS
        .get()
        .map(|a| a.themes.themes.keys().cloned().collect())
        .unwrap_or_default()
}

/// Highlights `code` written in `lang` with syntect theme `theme_name`. Returns
/// `None` if the language is unknown (render plain) or the definitions are not
/// loaded yet (call [`preload`] and retry after [`generation`] changes).
pub fn highlight(code: &str, lang: &str, theme_name: &str) -> Option<Highlighted> {
    if lang.trim().is_empty() {
        return None;
    }
    let Some(assets) = ASSETS.get() else {
        preload();
        return None;
    };
    let mut h = std::collections::hash_map::DefaultHasher::new();
    code.hash(&mut h);
    lang.hash(&mut h);
    theme_name.hash(&mut h);
    let key = h.finish();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(hit) = cache.lock().ok().and_then(|c| c.get(&key).cloned()) {
        return Some(hit);
    }
    let syntax = find_syntax(&assets.syntaxes, lang)?;
    let theme = theme(&assets.themes, theme_name)?;
    let mut hl = HighlightLines::new(syntax, theme);
    let mut spans = Vec::new();
    let mut offset = 0usize;
    for line in syntect::util::LinesWithEndings::from(code) {
        match hl.highlight_line(line, &assets.syntaxes) {
            Ok(regions) => {
                for (style, text) in regions {
                    let len = text.len();
                    let c = style.foreground;
                    spans.push(Span {
                        range: offset..offset + len,
                        color: Rgba([c.r, c.g, c.b, c.a]),
                        bold: style.font_style.contains(FontStyle::BOLD),
                        italic: style.font_style.contains(FontStyle::ITALIC),
                    });
                    offset += len;
                }
            }
            Err(e) => {
                log::debug!("highlight error: {e}");
                return None;
            }
        }
    }
    let out = Arc::new(spans);
    if let Ok(mut c) = cache.lock() {
        if c.len() > 4096 {
            c.clear();
        }
        c.insert(key, out.clone());
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlights_rust() {
        load_now();
        let h = highlight("fn main() {}\n", "rust", "InspiredGitHub").expect("rust highlighted");
        assert!(h.len() > 2);
        assert_eq!(h.last().unwrap().range.end, 13);
        let colors: std::collections::HashSet<_> = h.iter().map(|s| s.color).collect();
        assert!(colors.len() > 1);
        assert!(highlight("x", "no-such-lang", "InspiredGitHub").is_none());
        assert!(highlight("x", "", "InspiredGitHub").is_none());
    }

    #[test]
    fn aliases_resolve_to_existing_syntaxes() {
        load_now();
        let ss = &ASSETS.get().unwrap().syntaxes;
        for (alias, name) in ALIASES {
            assert!(
                ss.find_syntax_by_name(name).is_some(),
                "{alias} -> {name} missing"
            );
        }
    }
}
