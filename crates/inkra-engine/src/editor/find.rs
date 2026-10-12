//! Find and replace (ED-110, ED-111).

use std::ops::Range;

use inkra_doc::{Doc, Edit, Selection, Transaction};

/// Search options.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FindOptions {
    /// Match case.
    pub case_sensitive: bool,
    /// Whole words only.
    pub whole_word: bool,
    /// Regular expression (`$1` group references in replacements).
    pub regex: bool,
}

/// Find bar state of an editor.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FindState {
    /// Find bar visible.
    pub open: bool,
    /// Replace row visible.
    pub replace_open: bool,
    /// Query.
    pub query: String,
    /// Replacement.
    pub replacement: String,
    /// Options.
    pub options: FindOptions,
    /// Matches (absolute ranges), valid for `version`.
    pub matches: Vec<Range<usize>>,
    /// Index of the current match.
    pub current: Option<usize>,
    /// Regex error, if any.
    pub error: Option<String>,
    /// Document version the matches were computed for.
    pub version: Option<u64>,
}

fn build(query: &str, o: FindOptions) -> Result<regex::Regex, String> {
    let pat = if o.regex {
        query.to_owned()
    } else {
        regex::escape(query)
    };
    let pat = if o.whole_word {
        format!(r"\b(?:{pat})\b")
    } else {
        pat
    };
    regex::RegexBuilder::new(&pat)
        .case_insensitive(!o.case_sensitive)
        .multi_line(true)
        .build()
        .map_err(|e| e.to_string())
}

/// All non-empty matches of `query` in `text`.
pub fn find_all(text: &str, query: &str, o: FindOptions) -> Result<Vec<Range<usize>>, String> {
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let re = build(query, o)?;
    Ok(re
        .find_iter(text)
        .filter(|m| !m.as_str().is_empty())
        .map(|m| m.range())
        .take(100_000)
        .collect())
}

impl FindState {
    /// Recomputes matches if the query, options or document changed.
    pub fn refresh(&mut self, doc: &Doc, caret: usize) {
        if self.version == Some(doc.version()) {
            return;
        }
        self.version = Some(doc.version());
        match find_all(&doc.text().to_string(), &self.query, self.options) {
            Ok(m) => {
                self.matches = m;
                self.error = None;
            }
            Err(e) => {
                self.matches.clear();
                self.error = Some(e);
            }
        }
        self.current = if self.matches.is_empty() {
            None
        } else {
            Some(
                self.matches
                    .iter()
                    .position(|r| r.start >= caret)
                    .unwrap_or(0),
            )
        };
    }

    /// Forces recomputation on the next refresh.
    pub fn invalidate(&mut self) {
        self.version = None;
    }

    /// Moves to the next (or previous) match after `caret`; returns its range.
    pub fn step(&mut self, caret: usize, forward: bool) -> Option<Range<usize>> {
        if self.matches.is_empty() {
            return None;
        }
        let n = self.matches.len();
        let i = if forward {
            self.matches
                .iter()
                .position(|r| r.start > caret)
                .unwrap_or(0)
        } else {
            self.matches
                .iter()
                .rposition(|r| r.start < caret)
                .unwrap_or(n - 1)
        };
        self.current = Some(i);
        Some(self.matches[i].clone())
    }

    /// `"3/17"` (1-based), or `"0/0"`.
    pub fn counter(&self) -> String {
        match self.current {
            Some(i) => format!("{}/{}", i + 1, self.matches.len()),
            None => format!("0/{}", self.matches.len()),
        }
    }

    fn expand(&self, text: &str, m: &Range<usize>) -> String {
        if !self.options.regex {
            return self.replacement.clone();
        }
        match build(&self.query, self.options) {
            Ok(re) => match re.captures(&text[m.start..]) {
                Some(c) if c.get(0).is_some_and(|z| z.start() == 0) => {
                    let mut out = String::new();
                    c.expand(&self.replacement, &mut out);
                    out
                }
                _ => self.replacement.clone(),
            },
            Err(_) => self.replacement.clone(),
        }
    }

    /// Replaces the current match.
    pub fn replace_current(&self, doc: &Doc, sel: Selection) -> Option<Transaction> {
        let i = self.current?;
        let m = self.matches.get(i)?.clone();
        let text = doc.text().to_string();
        let rep = self.expand(&text, &m);
        let end = m.start + rep.len();
        Some(
            Transaction::many(vec![Edit {
                range: m.clone(),
                text: rep,
            }])
            .with_selection_before(sel)
            .with_selection_after(Selection::caret(end)),
        )
    }

    /// Replaces every match as one transaction (one undo group).
    pub fn replace_all(&self, doc: &Doc, sel: Selection) -> Option<Transaction> {
        if self.matches.is_empty() {
            return None;
        }
        let text = doc.text().to_string();
        let edits = self
            .matches
            .iter()
            .map(|m| Edit {
                range: m.clone(),
                text: self.expand(&text, m),
            })
            .collect();
        Some(Transaction::many(edits).with_selection_before(sel))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_with_options() {
        let t = "Foo foo food\nFOO";
        let o = FindOptions::default();
        assert_eq!(find_all(t, "foo", o).unwrap().len(), 4);
        assert_eq!(
            find_all(
                t,
                "foo",
                FindOptions {
                    whole_word: true,
                    ..o
                }
            )
            .unwrap()
            .len(),
            3
        );
        assert_eq!(
            find_all(
                t,
                "foo",
                FindOptions {
                    case_sensitive: true,
                    ..o
                }
            )
            .unwrap()
            .len(),
            2
        );
        assert_eq!(
            find_all(t, "f(o+)d", FindOptions { regex: true, ..o }).unwrap(),
            vec![8..12]
        );
        assert!(find_all(t, "(", FindOptions { regex: true, ..o }).is_err());
        assert_eq!(find_all(t, "a.b", o).unwrap().len(), 0);
    }

    #[test]
    fn replace_all_with_groups_is_one_transaction() {
        let mut doc = Doc::from_str("a1 b2 c3", Default::default());
        let mut st = FindState {
            query: r"([a-z])(\d)".into(),
            replacement: "$2$1".into(),
            options: FindOptions {
                regex: true,
                ..Default::default()
            },
            ..Default::default()
        };
        st.refresh(&doc, 0);
        assert_eq!(st.counter(), "1/3");
        let tx = st.replace_all(&doc, Selection::caret(0)).unwrap();
        doc.apply(tx, inkra_doc::UndoPolicy::NewGroup);
        assert_eq!(doc.text().to_string(), "1a 2b 3c");
        doc.undo();
        assert_eq!(doc.text().to_string(), "a1 b2 c3");
    }
}
