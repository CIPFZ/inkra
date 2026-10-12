//! Word and character statistics (FS-025).

use crate::segment::is_cjk_word_char;

/// Statistics of a text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Counts {
    /// Words: every CJK ideograph counts as one word, other words are separated by
    /// whitespace and CJK characters.
    pub words: usize,
    /// Characters including whitespace.
    pub chars: usize,
    /// Characters excluding whitespace.
    pub chars_no_space: usize,
    /// Lines.
    pub lines: usize,
    /// Paragraphs (runs of non-blank lines).
    pub paragraphs: usize,
}

impl Counts {
    /// Counts `s`.
    pub fn of(s: &str) -> Self {
        let mut c = Self::default();
        let mut in_word = false;
        for ch in s.chars() {
            c.chars += 1;
            if ch.is_whitespace() {
                in_word = false;
                continue;
            }
            c.chars_no_space += 1;
            if is_cjk_word_char(ch) {
                c.words += 1;
                in_word = false;
            } else if ch.is_alphanumeric() {
                if !in_word {
                    c.words += 1;
                    in_word = true;
                }
            } else {
                // Punctuation neither starts nor breaks a word ("don't" is one word).
            }
        }
        let mut in_para = false;
        for line in s.lines() {
            c.lines += 1;
            let blank = line.trim().is_empty();
            if !blank && !in_para {
                c.paragraphs += 1;
            }
            in_para = !blank;
        }
        c
    }

    /// Reading time in seconds: 275 words per minute plus 12 s per image.
    pub fn reading_seconds(&self, images: usize) -> u64 {
        (self.words as f64 / 275.0 * 60.0).round() as u64 + 12 * images as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_mixed_text() {
        let c = Counts::of("Hello world, 你好世界！\n\nSecond para don't");
        assert_eq!(c.words, 2 + 4 + 3);
        assert_eq!(c.paragraphs, 2);
        assert_eq!(c.lines, 3);
        assert_eq!(
            c.chars_no_space,
            "Helloworld,你好世界！Secondparadon't".chars().count()
        );
        assert_eq!(Counts::of("").words, 0);
        assert_eq!(c.reading_seconds(1), 2 + 12);
    }
}
