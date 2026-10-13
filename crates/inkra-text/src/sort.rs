//! Natural ordering (`file2` < `file10`), used for file lists and table sorting.

use std::cmp::Ordering;

/// Compares strings so that embedded numbers sort numerically; letters compare
/// case-insensitively first, then by case as a tie-breaker.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let o = natural_cmp_inner(&a.to_lowercase(), &b.to_lowercase());
    if o == Ordering::Equal { a.cmp(b) } else { o }
}

fn natural_cmp_inner(a: &str, b: &str) -> Ordering {
    let (mut ai, mut bi) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (ai.peek().copied(), bi.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let mut na = String::new();
                while let Some(c) = ai.peek().copied().filter(char::is_ascii_digit) {
                    na.push(c);
                    ai.next();
                }
                let mut nb = String::new();
                while let Some(c) = bi.peek().copied().filter(char::is_ascii_digit) {
                    nb.push(c);
                    bi.next();
                }
                let ta = na.trim_start_matches('0');
                let tb = nb.trim_start_matches('0');
                let o = ta
                    .len()
                    .cmp(&tb.len())
                    .then(ta.cmp(tb))
                    .then(na.len().cmp(&nb.len()));
                if o != Ordering::Equal {
                    return o;
                }
            }
            (Some(x), Some(y)) => {
                if x != y {
                    return x.cmp(&y);
                }
                ai.next();
                bi.next();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn natural() {
        let mut v = vec!["file10.md", "File2.md", "file1.md", "a.md", "file02.md"];
        v.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(
            v,
            ["a.md", "file1.md", "File2.md", "file02.md", "file10.md"]
        );
    }
}
