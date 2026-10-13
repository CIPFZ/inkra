//! Fuzzy path matching for quick open (FS-030) and the outline filter (FS-020).

use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};

/// A scored match with the matched character indices (for highlighting).
#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    /// Index into the candidate list.
    pub index: usize,
    /// Score (higher is better).
    pub score: u32,
    /// Matched char indices in the candidate.
    pub indices: Vec<u32>,
}

/// A reusable matcher.
pub struct Fuzzy {
    matcher: Matcher,
}

impl Default for Fuzzy {
    fn default() -> Self {
        Self::paths()
    }
}

impl Fuzzy {
    /// Tuned for file paths (bonus for path separators).
    pub fn paths() -> Self {
        Self {
            matcher: Matcher::new(Config::DEFAULT.match_paths()),
        }
    }

    /// Tuned for plain text (headings).
    pub fn text() -> Self {
        Self {
            matcher: Matcher::new(Config::DEFAULT),
        }
    }

    /// Matches `query` against `candidates`; best first, at most `limit`.
    pub fn rank<S: AsRef<str>>(&mut self, query: &str, candidates: &[S], limit: usize) -> Vec<Hit> {
        if query.trim().is_empty() {
            return (0..candidates.len().min(limit))
                .map(|index| Hit {
                    index,
                    score: 0,
                    indices: Vec::new(),
                })
                .collect();
        }
        let pattern = Pattern::parse(query, CaseMatching::Smart, Normalization::Smart);
        let mut buf = Vec::new();
        let mut hits: Vec<Hit> = candidates
            .iter()
            .enumerate()
            .filter_map(|(index, c)| {
                let h = Utf32Str::new(c.as_ref(), &mut buf);
                pattern.score(h, &mut self.matcher).map(|score| Hit {
                    index,
                    score,
                    indices: Vec::new(),
                })
            })
            .collect();
        hits.sort_by(|a, b| b.score.cmp(&a.score).then(a.index.cmp(&b.index)));
        hits.truncate(limit);
        for h in &mut hits {
            let mut idx = Vec::new();
            let s = Utf32Str::new(candidates[h.index].as_ref(), &mut buf);
            pattern.indices(s, &mut self.matcher, &mut idx);
            idx.sort_unstable();
            idx.dedup();
            h.indices = idx;
        }
        hits
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranks_paths() {
        let c = [
            "docs/readme.md",
            "src/main.rs",
            "notes/rust-notes.md",
            "rdm.md",
        ];
        let mut f = Fuzzy::paths();
        let h = f.rank("rdm", &c, 10);
        assert!(!h.is_empty());
        assert!(h.iter().any(|x| x.index == 3));
        let h = f.rank("notes", &c, 10);
        assert_eq!(h[0].index, 2);
        assert!(!h[0].indices.is_empty());
        assert_eq!(f.rank("", &c, 2).len(), 2);
    }

    #[test]
    fn ten_thousand_paths_under_50ms() {
        let c: Vec<String> = (0..10_000)
            .map(|i| format!("project/area{}/topic-{}/note {i}.md", i % 37, i % 101))
            .collect();
        let mut f = Fuzzy::paths();
        let t = std::time::Instant::now();
        let h = f.rank("topic note 77", &c, 50);
        let ms = t.elapsed().as_secs_f64() * 1e3;
        assert!(!h.is_empty());
        if !cfg!(debug_assertions) {
            assert!(ms < 50.0, "{ms} ms");
        }
        eprintln!("fuzzy 10k: {ms:.1} ms");
    }
}
