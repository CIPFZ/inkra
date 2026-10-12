//! Line endings (ED-106).
//!
//! Inkra keeps the decoded text byte-for-byte (no normalization), so files with
//! CRLF or mixed endings round-trip exactly. New line breaks inserted by editing
//! use the document's dominant style.

/// A line terminator style.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum LineEnding {
    /// `\n` (default for new files on all platforms, PR-016).
    #[default]
    Lf,
    /// `\r\n`.
    Crlf,
    /// Lone `\r` (classic Mac).
    Cr,
}

impl LineEnding {
    /// The terminator string.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Lf => "\n",
            Self::Crlf => "\r\n",
            Self::Cr => "\r",
        }
    }

    /// Short label (`LF`, `CRLF`, `CR`).
    pub const fn label(self) -> &'static str {
        match self {
            Self::Lf => "LF",
            Self::Crlf => "CRLF",
            Self::Cr => "CR",
        }
    }
}

/// Counts of each terminator in a text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EolStats {
    /// `\n` not preceded by `\r`.
    pub lf: usize,
    /// `\r\n`.
    pub crlf: usize,
    /// `\r` not followed by `\n`.
    pub cr: usize,
}

impl EolStats {
    /// Counts terminators in `s`.
    pub fn of(s: &str) -> Self {
        let b = s.as_bytes();
        let mut st = Self::default();
        let mut i = 0;
        while i < b.len() {
            match b[i] {
                b'\r' if b.get(i + 1) == Some(&b'\n') => {
                    st.crlf += 1;
                    i += 1;
                }
                b'\r' => st.cr += 1,
                b'\n' => st.lf += 1,
                _ => {}
            }
            i += 1;
        }
        st
    }

    /// The most frequent style (`Lf` for text without line breaks).
    pub fn dominant(&self) -> LineEnding {
        if self.crlf > self.lf && self.crlf >= self.cr {
            LineEnding::Crlf
        } else if self.cr > self.lf && self.cr > self.crlf {
            LineEnding::Cr
        } else {
            LineEnding::Lf
        }
    }

    /// Whether more than one style occurs.
    pub fn is_mixed(&self) -> bool {
        [self.lf, self.crlf, self.cr]
            .iter()
            .filter(|&&n| n > 0)
            .count()
            > 1
    }
}

/// Dominant line ending of `s`.
pub fn detect(s: &str) -> LineEnding {
    EolStats::of(s).dominant()
}

/// Rewrites every line terminator in `s` to `eol`.
pub fn convert(s: &str, eol: LineEnding) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                out.push_str(eol.as_str());
            }
            '\n' => out.push_str(eol.as_str()),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects() {
        assert_eq!(detect("a\r\nb\r\nc\n"), LineEnding::Crlf);
        assert_eq!(detect("a\nb"), LineEnding::Lf);
        assert_eq!(detect("x"), LineEnding::Lf);
        assert!(EolStats::of("a\r\nb\n").is_mixed());
        assert_eq!(
            convert("a\r\nb\nc\rd", LineEnding::Crlf),
            "a\r\nb\r\nc\r\nd"
        );
    }
}
