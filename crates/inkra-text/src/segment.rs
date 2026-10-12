//! Unicode segmentation helpers on `&str` (byte offsets).

use unicode_segmentation::UnicodeSegmentation;

/// Byte offset of the grapheme boundary after `at` (or `s.len()`).
pub fn next_grapheme(s: &str, at: usize) -> usize {
    let at = floor_char(s, at);
    s[at..]
        .graphemes(true)
        .next()
        .map_or(s.len(), |g| at + g.len())
}

/// Byte offset of the grapheme boundary before `at` (or 0).
pub fn prev_grapheme(s: &str, at: usize) -> usize {
    let at = floor_char(s, at);
    s[..at]
        .graphemes(true)
        .next_back()
        .map_or(0, |g| at - g.len())
}

/// Largest char boundary `<= at`.
pub fn floor_char(s: &str, at: usize) -> usize {
    let mut at = at.min(s.len());
    while !s.is_char_boundary(at) {
        at -= 1;
    }
    at
}

/// Whether `c` is a CJK ideograph, kana, hangul or CJK punctuation/fullwidth form.
pub fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x1100..=0x11FF       // Hangul Jamo
        | 0x2E80..=0x2FDF     // CJK radicals, Kangxi
        | 0x2FF0..=0x303F     // ideographic description, CJK symbols & punctuation
        | 0x3040..=0x30FF     // Hiragana, Katakana
        | 0x3100..=0x312F     // Bopomofo
        | 0x3130..=0x318F     // Hangul compatibility Jamo
        | 0x3190..=0x31FF     // Kanbun, Bopomofo ext, CJK strokes, Katakana ext
        | 0x3200..=0x33FF     // enclosed CJK, compatibility
        | 0x3400..=0x4DBF     // CJK ext A
        | 0x4E00..=0x9FFF     // CJK unified
        | 0xA960..=0xA97F     // Hangul Jamo ext A
        | 0xAC00..=0xD7FF     // Hangul syllables + Jamo ext B
        | 0xF900..=0xFAFF     // CJK compatibility ideographs
        | 0xFE30..=0xFE4F     // CJK compatibility forms
        | 0xFF00..=0xFFEF     // halfwidth and fullwidth forms
        | 0x1B000..=0x1B16F   // kana supplement/ext
        | 0x20000..=0x3FFFF   // CJK ext B..H
    )
}

/// Whether `c` is a CJK ideograph (Han), used for per-character word counting.
pub fn is_cjk_word_char(c: char) -> bool {
    is_cjk(c) && !c.is_ascii() && !is_cjk_punctuation(c)
}

fn is_cjk_punctuation(c: char) -> bool {
    matches!(c as u32, 0x3000..=0x303F | 0xFF00..=0xFF0F | 0xFF1A..=0xFF20 | 0xFF3B..=0xFF40 | 0xFF5B..=0xFF65 | 0xFE30..=0xFE4F)
}

/// Kind of character for word-wise movement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CharClass {
    Space,
    Word,
    Cjk,
    Punct,
}

fn class(c: char) -> CharClass {
    if c.is_whitespace() {
        CharClass::Space
    } else if is_cjk_word_char(c) {
        CharClass::Cjk
    } else if c.is_alphanumeric() || c == '_' {
        CharClass::Word
    } else {
        CharClass::Punct
    }
}

/// Word range around `at` for double-click selection (ED-011): a run of word
/// characters, a run of whitespace, or a single punctuation/CJK char (CJK text is
/// selected per character).
pub fn word_at(s: &str, at: usize) -> std::ops::Range<usize> {
    let at = floor_char(s, at);
    let Some(c) = s[at..]
        .chars()
        .next()
        .or_else(|| s[..at].chars().next_back())
    else {
        return at..at;
    };
    let at = if s[at..].chars().next().is_none() {
        at - c.len_utf8()
    } else {
        at
    };
    let k = class(c);
    if matches!(k, CharClass::Punct | CharClass::Cjk) {
        return at..at + c.len_utf8();
    }
    let mut start = at;
    for ch in s[..at].chars().rev() {
        if class(ch) != k {
            break;
        }
        start -= ch.len_utf8();
    }
    let mut end = at;
    for ch in s[at..].chars() {
        if class(ch) != k {
            break;
        }
        end += ch.len_utf8();
    }
    start..end
}

/// Start of the previous word (for `Ctrl+Left` / `Ctrl+Backspace`): skips
/// whitespace, then one run of same-class chars (one char for CJK/punctuation runs
/// are taken as a whole run of punctuation).
pub fn prev_word(s: &str, at: usize) -> usize {
    let mut i = floor_char(s, at);
    let mut it = s[..i].chars().rev().peekable();
    while let Some(&c) = it.peek() {
        if !c.is_whitespace() {
            break;
        }
        i -= c.len_utf8();
        it.next();
    }
    let Some(&first) = it.peek() else { return i };
    let k = class(first);
    for c in it {
        if class(c) != k {
            break;
        }
        i -= c.len_utf8();
    }
    i
}

/// End of the next word (for `Ctrl+Right` / `Ctrl+Delete`).
pub fn next_word(s: &str, at: usize) -> usize {
    let mut i = floor_char(s, at);
    let mut it = s[i..].chars().peekable();
    while let Some(&c) = it.peek() {
        if !c.is_whitespace() {
            break;
        }
        i += c.len_utf8();
        it.next();
    }
    let Some(&first) = it.peek() else { return i };
    let k = class(first);
    for c in it {
        if class(c) != k {
            break;
        }
        i += c.len_utf8();
    }
    i
}

/// Display width in terminal columns: CJK wide chars count 2 (ED-085).
pub fn display_width(s: &str) -> usize {
    unicode_width::UnicodeWidthStr::width(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graphemes() {
        let s = "e\u{301}x👍🏽";
        assert_eq!(next_grapheme(s, 0), 3);
        assert_eq!(next_grapheme(s, 3), 4);
        assert_eq!(next_grapheme(s, 4), s.len());
        assert_eq!(prev_grapheme(s, s.len()), 4);
        assert_eq!(prev_grapheme(s, 3), 0);
    }

    #[test]
    fn words() {
        let s = "hello, 世界 foo_bar";
        assert_eq!(&s[word_at(s, 1)], "hello");
        assert_eq!(&s[word_at(s, 5)], ",");
        assert_eq!(&s[word_at(s, 7)], "世");
        assert_eq!(&s[word_at(s, s.len())], "foo_bar");
        assert_eq!(prev_word(s, s.len()), s.len() - 7);
        assert_eq!(prev_word(s, 5), 0);
        assert_eq!(next_word(s, 0), 5);
        assert_eq!(next_word(s, 5), 6);
    }

    #[test]
    fn widths() {
        assert_eq!(display_width("ab"), 2);
        assert_eq!(display_width("中文"), 4);
        assert!(is_cjk('中') && is_cjk('の') && is_cjk('한') && !is_cjk('a'));
    }
}
