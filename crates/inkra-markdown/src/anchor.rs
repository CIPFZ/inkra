//! GitHub-style heading anchors (MD-002).

use std::collections::HashMap;

/// Converts heading text to a slug: lower-case, punctuation removed (except `-`
/// and `_`), spaces turned into `-`.
pub fn slugify(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.trim().chars() {
        if c.is_alphanumeric() || c == '_' || c == '-' {
            out.extend(c.to_lowercase());
        } else if c == ' ' {
            out.push('-');
        }
    }
    out
}

/// Produces unique slugs: duplicates get `-1`, `-2`, … appended.
#[derive(Debug, Default, Clone)]
pub struct Slugger {
    seen: HashMap<String, usize>,
}

impl Slugger {
    /// Unique slug for `text`.
    pub fn slug(&mut self, text: &str) -> String {
        let base = slugify(text);
        let mut candidate = base.clone();
        loop {
            match self.seen.get_mut(&candidate) {
                None => {
                    self.seen.insert(candidate.clone(), 0);
                    return candidate;
                }
                Some(_) => {
                    let n = self.seen.get(&base).copied().unwrap_or(0) + 1;
                    self.seen.insert(base.clone(), n);
                    candidate = format!("{base}-{n}");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn github_slugs() {
        assert_eq!(slugify("Hello, World!"), "hello-world");
        assert_eq!(slugify("API v2.0 — notes"), "api-v20--notes");
        assert_eq!(slugify("中文 标题"), "中文-标题");
        let mut s = Slugger::default();
        assert_eq!(s.slug("Intro"), "intro");
        assert_eq!(s.slug("Intro"), "intro-1");
        assert_eq!(s.slug("Intro"), "intro-2");
        assert_eq!(s.slug("intro-1"), "intro-1-1");
    }
}
