//! # inkra-html
//!
//! **Layer L2** (see SPEC 2.2).
//!
//! HTML ↔ Markdown. HTML is only a file/clipboard format here, parsed with
//! html5ever; it is never executed or rendered by a browser engine (SPEC 1.6).
//! This crate converts pasted or imported HTML to Markdown (ED-016, EX-020).

mod to_markdown;

pub use to_markdown::html_to_markdown;
