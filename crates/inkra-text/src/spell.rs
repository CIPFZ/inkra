//! Spell checking (ED-104): Hunspell dictionaries through `spellbook`. The
//! bundled en_US dictionary loads on a background thread; until it is ready
//! every word counts as correct. A personal word list and session ignores
//! apply on top.

use std::collections::HashSet;
use std::ops::Range;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{OnceLock, RwLock};

/// The bundled American English dictionary (SCOWL, see its license file).
static EN_US_AFF: &str = include_str!("../../../assets/dictionaries/en_US.aff");
static EN_US_DIC: &str = include_str!("../../../assets/dictionaries/en_US.dic");

#[derive(Default)]
struct State {
    dict: Option<spellbook::Dictionary>,
    personal: HashSet<String>,
    ignored: HashSet<String>,
    personal_file: Option<PathBuf>,
}

static STATE: OnceLock<RwLock<State>> = OnceLock::new();
static GENERATION: AtomicU64 = AtomicU64::new(0);

fn state() -> &'static RwLock<State> {
    STATE.get_or_init(Default::default)
}

/// Changes whenever results may change (dictionary loaded, words added).
pub fn generation() -> u64 {
    GENERATION.load(Ordering::SeqCst)
}

/// Starts loading the dictionary in the background; `personal` is the user's
/// word list (one word per line). `ready` runs once loaded (e.g. repaint).
pub fn load(personal: Option<PathBuf>, ready: impl FnOnce() + Send + 'static) {
    static STARTED: OnceLock<()> = OnceLock::new();
    if STARTED.set(()).is_err() {
        return;
    }
    std::thread::Builder::new()
        .name("inkra-spell".into())
        .spawn(move || {
            let t0 = std::time::Instant::now();
            let dict = match spellbook::Dictionary::new(EN_US_AFF, EN_US_DIC) {
                Ok(d) => Some(d),
                Err(e) => {
                    log::warn!("spell dictionary: {e}");
                    None
                }
            };
            let words: HashSet<String> = personal
                .as_ref()
                .and_then(|p| std::fs::read_to_string(p).ok())
                .map(|s| {
                    s.lines()
                        .map(str::trim)
                        .filter(|w| !w.is_empty())
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default();
            if let Ok(mut s) = state().write() {
                s.dict = dict;
                s.personal = words;
                s.personal_file = personal;
            }
            log::info!(
                "spell checker ready in {:.0} ms",
                t0.elapsed().as_secs_f64() * 1e3
            );
            GENERATION.fetch_add(1, Ordering::SeqCst);
            ready();
        })
        .ok();
}

/// Loads synchronously (tests).
pub fn load_now() {
    if state().read().is_ok_and(|s| s.dict.is_some()) {
        return;
    }
    if let Ok(mut s) = state().write() {
        s.dict = spellbook::Dictionary::new(EN_US_AFF, EN_US_DIC).ok();
    }
    GENERATION.fetch_add(1, Ordering::SeqCst);
}

/// Whether the checker is ready.
pub fn ready() -> bool {
    state().read().is_ok_and(|s| s.dict.is_some())
}

/// Words that are not checked: too short, containing digits or underscores,
/// all-uppercase acronyms, CamelCase identifiers, and non-Latin scripts.
fn checkable(w: &str) -> bool {
    let n = w.chars().count();
    if n < 2 || w.chars().any(|c| c.is_ascii_digit() || c == '_') {
        return false;
    }
    if !w.chars().all(|c| {
        c.is_ascii_alphabetic() || c == '\'' || c == '’' || ('\u{c0}'..='\u{24f}').contains(&c)
    }) {
        return false;
    }
    let upper = w.chars().filter(|c| c.is_uppercase()).count();
    if upper == n || (upper > 1 && w.chars().skip(1).any(char::is_uppercase)) {
        return false;
    }
    true
}

/// Whether `word` is spelled correctly (or not checked).
pub fn check(word: &str) -> bool {
    if !checkable(word) {
        return true;
    }
    let Ok(s) = state().read() else { return true };
    let Some(d) = &s.dict else { return true };
    let w = word.replace('’', "'");
    let w = w.trim_matches('\'');
    if w.is_empty() || s.personal.contains(w) || s.ignored.contains(w) {
        return true;
    }
    d.check(w) || {
        let lower = w.to_lowercase();
        s.personal.contains(&lower) || d.check(&lower)
    }
}

/// Spelling suggestions (at most `n`).
pub fn suggest(word: &str, n: usize) -> Vec<String> {
    let Ok(s) = state().read() else {
        return Vec::new();
    };
    let Some(d) = &s.dict else { return Vec::new() };
    let mut out = Vec::new();
    d.suggest(&word.replace('’', "'"), &mut out);
    out.truncate(n);
    out
}

/// Adds a word to the personal dictionary (saved to its file).
pub fn add_word(word: &str) {
    if let Ok(mut s) = state().write() {
        s.personal.insert(word.to_owned());
        if let Some(p) = s.personal_file.clone() {
            let mut words: Vec<&String> = s.personal.iter().collect();
            words.sort();
            let body: String = words.iter().map(|w| format!("{w}\n")).collect();
            if let Some(dir) = p.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            if let Err(e) = std::fs::write(&p, body) {
                log::warn!("cannot save {}: {e}", p.display());
            }
        }
    }
    GENERATION.fetch_add(1, Ordering::SeqCst);
}

/// Ignores a word for this session.
pub fn ignore(word: &str) {
    if let Ok(mut s) = state().write() {
        s.ignored.insert(word.to_owned());
    }
    GENERATION.fetch_add(1, Ordering::SeqCst);
}

/// Byte ranges of misspelled words in `text`.
pub fn misspelled(text: &str) -> Vec<Range<usize>> {
    if !ready() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    let is_word = |c: char| c.is_alphanumeric() || c == '\'' || c == '’' || c == '_';
    for (i, c) in text
        .char_indices()
        .chain(std::iter::once((text.len(), ' ')))
    {
        if is_word(c) {
            start.get_or_insert(i);
        } else if let Some(s) = start.take() {
            let w = text[s..i].trim_end_matches(['\'', '’']);
            let w = w.trim_start_matches(['\'', '’']);
            if !w.is_empty() && !check(w) {
                let off = text[s..i].find(w).unwrap_or(0);
                out.push(s + off..s + off + w.len());
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checks_and_suggests() {
        load_now();
        assert!(check("hello"));
        assert!(check("Hello"));
        assert!(check("don't"));
        assert!(!check("helo"));
        assert!(check("NASA"), "acronyms are skipped");
        assert!(check("iPhone"), "mixed case is skipped");
        assert!(check("abc123"));
        assert!(check("中文"));
        assert!(suggest("helo", 5).iter().any(|s| s == "hello"));
        assert_eq!(
            misspelled("This is a tset of speling."),
            vec![10..14, 18..25]
        );
        ignore("tset");
        assert_eq!(misspelled("a tset"), Vec::<Range<usize>>::new());
    }
}

#[cfg(test)]
mod more_tests {
    use super::*;

    #[test]
    fn sentence() {
        load_now();
        let s = "This sentense has a speling mistake, but ";
        let r = misspelled(s);
        let words: Vec<&str> = r.iter().map(|r| &s[r.clone()]).collect();
        assert_eq!(words, ["sentense", "speling"]);
    }
}
