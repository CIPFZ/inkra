//! `:shortcode:` emoji table (MD-039, GitHub gemoji).

use crate::emoji_table::EMOJI;

/// Looks up a shortcode (without colons).
pub fn lookup(name: &str) -> Option<&'static str> {
    EMOJI
        .binary_search_by(|(k, _)| (*k).cmp(name))
        .ok()
        .map(|i| EMOJI[i].1)
}

/// Every `(shortcode, emoji)`, sorted by shortcode (the Emoji & Symbols picker).
pub fn all() -> &'static [(&'static str, &'static str)] {
    EMOJI
}

/// Shortcodes for completion (ED-103): those starting with `prefix` first,
/// then those containing it; at most `limit`.
pub fn complete(prefix: &str, limit: usize) -> Vec<(&'static str, &'static str)> {
    let p = prefix.to_ascii_lowercase();
    if p.is_empty() {
        return Vec::new();
    }
    let start = EMOJI.partition_point(|(k, _)| *k < p.as_str());
    let mut out: Vec<(&str, &str)> = EMOJI[start..]
        .iter()
        .take_while(|(k, _)| k.starts_with(p.as_str()))
        .take(limit)
        .copied()
        .collect();
    if out.len() < limit {
        for &(k, v) in EMOJI {
            if out.len() >= limit {
                break;
            }
            if !k.starts_with(p.as_str()) && k.contains(p.as_str()) {
                out.push((k, v));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table() {
        assert!(EMOJI.len() > 1800);
        assert!(EMOJI.windows(2).all(|w| w[0].0 < w[1].0), "sorted");
        assert_eq!(lookup("smile"), Some("😄"));
        assert_eq!(lookup("+1"), Some("👍"));
        assert_eq!(lookup("nope_nope"), None);
        let c = complete("smi", 10);
        assert!(c.iter().any(|(k, _)| *k == "smile"));
        assert!(c.len() <= 10);
    }
}
