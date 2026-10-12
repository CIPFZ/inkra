//! Encoding detection and round-trip encoding (ED-106).
//!
//! Detection order: BOM (UTF-8, UTF-16 LE/BE) → valid UTF-8 → BOM-less UTF-16
//! heuristic → GBK → GB18030 → lossy UTF-8. Saving re-encodes with the detected
//! encoding (and BOM), so an unmodified file is written back byte-identically.

/// A supported text encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Encoding {
    /// UTF-8 (default for new files, PR-016).
    #[default]
    Utf8,
    /// UTF-16 little endian.
    Utf16Le,
    /// UTF-16 big endian.
    Utf16Be,
    /// GBK (Simplified Chinese, CP936).
    Gbk,
    /// GB18030.
    Gb18030,
}

impl Encoding {
    /// Display label.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Utf8 => "UTF-8",
            Self::Utf16Le => "UTF-16 LE",
            Self::Utf16Be => "UTF-16 BE",
            Self::Gbk => "GBK",
            Self::Gb18030 => "GB18030",
        }
    }

    /// Parses a label as used in preferences (`utf-8`, `gbk`, …).
    pub fn from_label(s: &str) -> Option<Self> {
        Some(
            match s.to_ascii_lowercase().replace(['-', '_', ' '], "").as_str() {
                "utf8" => Self::Utf8,
                "utf16le" | "utf16" => Self::Utf16Le,
                "utf16be" => Self::Utf16Be,
                "gbk" | "cp936" => Self::Gbk,
                "gb18030" => Self::Gb18030,
                _ => return None,
            },
        )
    }

    fn rs(self) -> &'static encoding_rs::Encoding {
        match self {
            Self::Utf8 => encoding_rs::UTF_8,
            Self::Utf16Le => encoding_rs::UTF_16LE,
            Self::Utf16Be => encoding_rs::UTF_16BE,
            Self::Gbk => encoding_rs::GBK,
            Self::Gb18030 => encoding_rs::GB18030,
        }
    }
}

/// Result of decoding a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decoded {
    /// Decoded text.
    pub text: String,
    /// Detected encoding.
    pub encoding: Encoding,
    /// Whether the file started with a byte order mark.
    pub bom: bool,
    /// Whether undecodable bytes were replaced with U+FFFD. Saving such a document
    /// would not reproduce the original bytes; the UI warns before saving.
    pub lossy: bool,
}

fn looks_like_utf16(bytes: &[u8]) -> Option<Encoding> {
    if bytes.len() < 4 || !bytes.len().is_multiple_of(2) {
        return None;
    }
    let sample = &bytes[..bytes.len().min(4096)];
    let pairs = sample.len() / 2;
    let even_zero = sample.iter().step_by(2).filter(|&&b| b == 0).count();
    let odd_zero = sample
        .iter()
        .skip(1)
        .step_by(2)
        .filter(|&&b| b == 0)
        .count();
    // Mostly-ASCII UTF-16 text has a zero in every other byte.
    if odd_zero * 10 > pairs * 7 && even_zero * 10 < pairs {
        Some(Encoding::Utf16Le)
    } else if even_zero * 10 > pairs * 7 && odd_zero * 10 < pairs {
        Some(Encoding::Utf16Be)
    } else {
        None
    }
}

/// Decodes file bytes. `fallback` is tried before GB18030 for non-UTF-8 input
/// (preference "default encoding"); pass `None` for the built-in order.
pub fn decode(bytes: &[u8], fallback: Option<Encoding>) -> Decoded {
    if let Some(rest) = bytes.strip_prefix(b"\xEF\xBB\xBF") {
        let (text, lossy) = match std::str::from_utf8(rest) {
            Ok(s) => (s.to_owned(), false),
            Err(_) => (String::from_utf8_lossy(rest).into_owned(), true),
        };
        return Decoded {
            text,
            encoding: Encoding::Utf8,
            bom: true,
            lossy,
        };
    }
    for (bom, enc) in [
        (&b"\xFF\xFE"[..], Encoding::Utf16Le),
        (&b"\xFE\xFF"[..], Encoding::Utf16Be),
    ] {
        if let Some(rest) = bytes.strip_prefix(bom) {
            let (text, had_errors) = enc.rs().decode_without_bom_handling(rest);
            return Decoded {
                text: text.into_owned(),
                encoding: enc,
                bom: true,
                lossy: had_errors,
            };
        }
    }
    // NUL bytes are valid UTF-8, so BOM-less UTF-16 must be recognized first.
    if let Some(enc) = looks_like_utf16(bytes) {
        let (text, had_errors) = enc.rs().decode_without_bom_handling(bytes);
        if !had_errors {
            return Decoded {
                text: text.into_owned(),
                encoding: enc,
                bom: false,
                lossy: false,
            };
        }
    }
    if let Ok(s) = std::str::from_utf8(bytes) {
        return Decoded {
            text: s.to_owned(),
            encoding: Encoding::Utf8,
            bom: false,
            lossy: false,
        };
    }
    let mut candidates = Vec::new();
    if let Some(f) = fallback.filter(|f| *f != Encoding::Utf8) {
        candidates.push(f);
    }
    candidates.extend([Encoding::Gbk, Encoding::Gb18030]);
    for enc in candidates {
        let (text, had_errors) = enc.rs().decode_without_bom_handling(bytes);
        if !had_errors && encode(&text, enc, false) == bytes {
            return Decoded {
                text: text.into_owned(),
                encoding: enc,
                bom: false,
                lossy: false,
            };
        }
    }
    Decoded {
        text: String::from_utf8_lossy(bytes).into_owned(),
        encoding: Encoding::Utf8,
        bom: false,
        lossy: true,
    }
}

/// Encodes `text` for saving.
pub fn encode(text: &str, encoding: Encoding, bom: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len() + 3);
    match encoding {
        Encoding::Utf8 => {
            if bom {
                out.extend_from_slice(b"\xEF\xBB\xBF");
            }
            out.extend_from_slice(text.as_bytes());
        }
        Encoding::Utf16Le | Encoding::Utf16Be => {
            let le = encoding == Encoding::Utf16Le;
            if bom {
                out.extend_from_slice(if le { b"\xFF\xFE" } else { b"\xFE\xFF" });
            }
            for u in text.encode_utf16() {
                out.extend_from_slice(&if le { u.to_le_bytes() } else { u.to_be_bytes() });
            }
        }
        Encoding::Gbk | Encoding::Gb18030 => {
            let (bytes, _, _) = encoding.rs().encode(text);
            out.extend_from_slice(&bytes);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(bytes: &[u8], expect: Encoding) {
        let d = decode(bytes, None);
        assert_eq!(d.encoding, expect, "{bytes:?}");
        assert!(!d.lossy);
        assert_eq!(encode(&d.text, d.encoding, d.bom), bytes);
    }

    #[test]
    fn detects_and_round_trips() {
        round_trip("# 标题\n".as_bytes(), Encoding::Utf8);
        round_trip(b"\xEF\xBB\xBF# x\r\n", Encoding::Utf8);
        let mut le = vec![0xFF, 0xFE];
        for u in "中文 text".encode_utf16() {
            le.extend_from_slice(&u.to_le_bytes());
        }
        round_trip(&le, Encoding::Utf16Le);
        let mut be = Vec::new();
        for u in "hello world".encode_utf16() {
            be.extend_from_slice(&u.to_be_bytes());
        }
        round_trip(&be, Encoding::Utf16Be);
        let (gbk, _, _) = encoding_rs::GBK.encode("中文标题，测试编码。");
        round_trip(&gbk, Encoding::Gbk);
    }

    #[test]
    fn invalid_input_is_lossy_not_lost() {
        let d = decode(&[0x80, 0x81, 0xFF, b'a'], None);
        assert!(d.lossy || d.encoding != Encoding::Utf8);
        assert!(!d.text.is_empty());
    }
}
