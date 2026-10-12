//! `:shortcode:` emoji table (MD-039).

/// `(shortcode, emoji)` sorted by shortcode.
// TODO(MD-039): replace with the full GitHub gemoji table in M5.
const EMOJI: &[(&str, &str)] = &[];

/// Looks up a shortcode (without colons).
pub fn lookup(name: &str) -> Option<&'static str> {
    EMOJI
        .binary_search_by(|(k, _)| (*k).cmp(name))
        .ok()
        .map(|i| EMOJI[i].1)
}
