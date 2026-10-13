//! # inkra-text
//!
//! **Layer L0** (see SPEC 2.2).
//!
//! Text foundation: a rope wrapper addressed by **byte offsets** (the unit used by
//! every source range in the Markdown AST), offset conversion between bytes,
//! chars, lines and columns, Unicode grapheme/word segmentation, display width,
//! word counting, encoding detection (ED-106) and line endings.

pub mod count;
pub mod encoding;
pub mod eol;
pub mod segment;
pub mod sort;
mod text;

pub use encoding::{Decoded, Encoding};
pub use eol::LineEnding;
pub use text::Text;
