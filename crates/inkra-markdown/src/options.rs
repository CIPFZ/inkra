//! Parser options (SPEC 4.3).

/// Syntax switches. [`ParseOptions::default`] is Inkra's default dialect (GFM plus
/// Inkra extensions with the defaults of SPEC 4.3); [`ParseOptions::commonmark`]
/// is strict CommonMark 0.31, used for conformance tests.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ParseOptions {
    /// Indented (4-space) code blocks. Off in Inkra (MD-007): indented text is a
    /// paragraph.
    pub indented_code: bool,
    /// Require a space after ATX `#` (strict mode, MD-016). Off: `#Heading` is a heading.
    pub strict_atx: bool,
    /// Relax emphasis flanking rules next to CJK characters (MD-030).
    pub cjk_emphasis: bool,
    /// GFM tables (MD-009).
    pub tables: bool,
    /// GFM task list items (MD-006).
    pub tasks: bool,
    /// GFM strikethrough `~~x~~` (MD-032).
    pub strikethrough: bool,
    /// GFM extended autolinks (`www.`, `http://`, e-mail) (MD-033).
    pub extended_autolinks: bool,
    /// Footnotes `[^x]` (MD-010).
    pub footnotes: bool,
    /// YAML front matter (MD-012).
    pub front_matter: bool,
    /// `[TOC]` blocks (MD-013).
    pub toc: bool,
    /// `$$` math blocks (MD-008).
    pub math_blocks: bool,
    /// `$x$` inline math (MD-060, default off).
    pub inline_math: bool,
    /// `\[..\]` and `\(..\)` math delimiters (MD-069, default off).
    pub latex_delimiters: bool,
    /// Relaxed inline math: allow spaces next to `$` (MD-070, default off).
    pub loose_inline_math: bool,
    /// `==x==` highlight (MD-063, default off).
    pub highlight: bool,
    /// `^x^` superscript (MD-062, default off).
    pub superscript: bool,
    /// `~x~` subscript (MD-061, default off). When on, single `~` is no longer
    /// strikethrough.
    pub subscript: bool,
    /// GitHub alerts `> [!NOTE]` (MD-004).
    pub alerts: bool,
    /// `:shortcode:` emoji (MD-039).
    pub emoji: bool,
}

impl Default for ParseOptions {
    fn default() -> Self {
        Self {
            indented_code: false,
            strict_atx: false,
            cjk_emphasis: true,
            tables: true,
            tasks: true,
            strikethrough: true,
            extended_autolinks: true,
            footnotes: true,
            front_matter: true,
            toc: true,
            math_blocks: true,
            inline_math: false,
            latex_delimiters: false,
            loose_inline_math: false,
            highlight: false,
            superscript: false,
            subscript: false,
            alerts: true,
            emoji: true,
        }
    }
}

impl ParseOptions {
    /// Strict CommonMark 0.31: no extensions.
    pub fn commonmark() -> Self {
        Self {
            indented_code: true,
            strict_atx: true,
            cjk_emphasis: false,
            tables: false,
            tasks: false,
            strikethrough: false,
            extended_autolinks: false,
            footnotes: false,
            front_matter: false,
            toc: false,
            math_blocks: false,
            inline_math: false,
            latex_delimiters: false,
            loose_inline_math: false,
            highlight: false,
            superscript: false,
            subscript: false,
            alerts: false,
            emoji: false,
        }
    }

    /// GitHub Flavored Markdown (CommonMark + tables, tasks, strikethrough,
    /// extended autolinks), used for the GFM conformance tests.
    pub fn gfm() -> Self {
        Self {
            tables: true,
            tasks: true,
            strikethrough: true,
            extended_autolinks: true,
            ..Self::commonmark()
        }
    }
}
