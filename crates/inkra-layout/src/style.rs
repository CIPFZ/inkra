//! Visual style consumed by layout and rendering. Built from a theme (TH-001);
//! the default is the `Paper` theme.

use epaint::Color32;

/// Per-heading-level style.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HeadingStyle {
    /// Size relative to the body size.
    pub size: f32,
    /// Bold weight.
    pub bold: bool,
    /// Space above, in em of the heading size.
    pub margin_top: f32,
    /// Space below, in em.
    pub margin_bottom: f32,
    /// Rule under the heading.
    pub border_bottom: bool,
    /// Text color override.
    pub color: Option<Color32>,
}

/// Colors of source-mode syntax highlighting.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SourceColors {
    /// Headings.
    pub heading: Color32,
    /// Markers (`#`, `*`, `>`, list bullets…).
    pub marker: Color32,
    /// Inline code and code blocks.
    pub code: Color32,
    /// Links.
    pub link: Color32,
    /// URLs.
    pub url: Color32,
    /// Raw HTML.
    pub html: Color32,
    /// Math.
    pub math: Color32,
    /// Front matter and definitions.
    pub meta: Color32,
}

/// All visual parameters of the document area.
#[derive(Debug, Clone, PartialEq)]
pub struct Style {
    /// Theme name.
    pub name: String,
    /// Dark theme.
    pub dark: bool,
    /// Body typeface.
    pub body_font: crate::fonts::FontKind,
    /// Heading typeface.
    pub heading_font: crate::fonts::FontKind,
    /// syntect theme for code blocks.
    pub syntax_theme: String,
    /// UI chrome: sidebar background.
    pub sidebar_bg: Color32,
    /// UI chrome: sidebar text.
    pub sidebar_fg: Color32,
    /// UI chrome: sidebar selection.
    pub sidebar_active_bg: Color32,
    /// UI chrome: status bar text.
    pub statusbar_fg: Color32,
    /// Alert (`> [!NOTE]`) backgrounds use the alert colors at this opacity.
    pub alert_bg_alpha: f32,
    /// Equation numbering (MD-068): 0 off, 1 AMS environments, 2 all blocks.
    /// A rendering option set from preferences, not by themes.
    pub math_numbering: u8,
    /// Render ```` ```mermaid ```` blocks as diagrams (MD-064); a preference.
    pub diagrams: bool,
    /// Body font size in points.
    pub size: f32,
    /// Monospace font size in points.
    pub mono_size: f32,
    /// Line height factor.
    pub line_height: f32,
    /// Paragraph spacing in em.
    pub paragraph_spacing: f32,
    /// Background.
    pub bg: Color32,
    /// Text.
    pub fg: Color32,
    /// Secondary text.
    pub fg_muted: Color32,
    /// Accent (links, selection base).
    pub accent: Color32,
    /// Selection fill.
    pub selection: Color32,
    /// Caret.
    pub cursor: Color32,
    /// Code background.
    pub code_bg: Color32,
    /// Code border.
    pub code_border: Color32,
    /// Quote bar.
    pub quote_border: Color32,
    /// Quote text.
    pub quote_fg: Color32,
    /// Horizontal rule.
    pub hr: Color32,
    /// Table borders.
    pub table_border: Color32,
    /// Table header background.
    pub table_header_bg: Color32,
    /// Background of every other body row.
    pub table_row_alt_bg: Color32,
    /// Highlight (`==x==`) background.
    pub highlight_bg: Color32,
    /// Visible Markdown markers.
    pub mark_fg: Color32,
    /// Search match background.
    pub search_match_bg: Color32,
    /// Current search match background.
    pub search_current_bg: Color32,
    /// Spelling error underline.
    pub spell_error: Color32,
    /// Alert colors: note, tip, important, warning, caution.
    pub alerts: [Color32; 5],
    /// Heading styles h1..h6.
    pub headings: [HeadingStyle; 6],
    /// Maximum text column width.
    pub max_width: f32,
    /// Horizontal padding of the text column.
    pub padding_x: f32,
    /// Vertical padding at top and bottom.
    pub padding_y: f32,
    /// Corner radius of code blocks.
    pub code_radius: f32,
    /// Inner padding of code blocks.
    pub code_padding: f32,
    /// List indentation in em.
    pub list_indent: f32,
    /// Quote indentation in em.
    pub quote_indent: f32,
    /// Opacity of non-active blocks in focus mode.
    pub focus_dim_opacity: f32,
    /// Line number color.
    pub line_numbers_fg: Color32,
    /// Source-mode highlighting.
    pub source: SourceColors,
    /// Incremented whenever the style changes (cache key).
    pub version: u64,
}

const fn rgb(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

impl Default for Style {
    /// The `Paper` theme: brand Paper background, Ink accent, Coral caret
    /// (SPEC appendix C.3).
    fn default() -> Self {
        let h = |size, margin_top, margin_bottom, border_bottom| HeadingStyle {
            size,
            bold: true,
            margin_top,
            margin_bottom,
            border_bottom,
            color: None,
        };
        Self {
            name: "Paper".to_owned(),
            dark: false,
            body_font: crate::fonts::FontKind::Sans,
            heading_font: crate::fonts::FontKind::Sans,
            syntax_theme: "InspiredGitHub".to_owned(),
            sidebar_bg: rgb(0xf3, 0xf1, 0xec),
            sidebar_fg: rgb(0x33, 0x33, 0x33),
            sidebar_active_bg: rgb(0xe4, 0xe1, 0xd8),
            statusbar_fg: rgb(0x99, 0x99, 0x99),
            alert_bg_alpha: 0.08,
            math_numbering: 0,
            diagrams: true,
            size: 16.0,
            mono_size: 14.0,
            line_height: 1.6,
            paragraph_spacing: 0.8,
            bg: rgb(0xFA, 0xF8, 0xF3),
            fg: rgb(0x24, 0x29, 0x2e),
            fg_muted: rgb(0x6a, 0x73, 0x7d),
            accent: rgb(0x2E, 0x2A, 0x5A),
            selection: Color32::from_rgba_unmultiplied(0x2E, 0x2A, 0x5A, 0x33),
            cursor: rgb(0xFF, 0x6B, 0x57),
            code_bg: rgb(0xf3, 0xf1, 0xec),
            code_border: rgb(0xe1, 0xe4, 0xe8),
            quote_border: rgb(0xdf, 0xe2, 0xe5),
            quote_fg: rgb(0x6a, 0x73, 0x7d),
            hr: rgb(0xe1, 0xe4, 0xe8),
            table_border: rgb(0xdf, 0xe2, 0xe5),
            table_header_bg: rgb(0xf3, 0xf1, 0xec),
            table_row_alt_bg: rgb(0xf7, 0xf5, 0xef),
            highlight_bg: rgb(0xff, 0xf5, 0xb1),
            mark_fg: rgb(0x9a, 0xa0, 0xa6),
            search_match_bg: Color32::from_rgba_unmultiplied(0xff, 0xeb, 0x3b, 0x80),
            search_current_bg: Color32::from_rgba_unmultiplied(0xff, 0x98, 0x00, 0x80),
            spell_error: rgb(0xd7, 0x3a, 0x49),
            alerts: [
                rgb(0x09, 0x69, 0xda),
                rgb(0x1a, 0x7f, 0x37),
                rgb(0x82, 0x50, 0xdf),
                rgb(0x9a, 0x67, 0x00),
                rgb(0xcf, 0x22, 0x2e),
            ],
            headings: [
                h(2.0, 1.2, 0.6, true),
                h(1.5, 1.0, 0.5, true),
                h(1.25, 0.9, 0.4, false),
                h(1.0, 0.8, 0.4, false),
                h(0.875, 0.8, 0.4, false),
                HeadingStyle {
                    color: Some(rgb(0x6a, 0x73, 0x7d)),
                    ..h(0.85, 0.8, 0.4, false)
                },
            ],
            max_width: 860.0,
            padding_x: 30.0,
            padding_y: 40.0,
            code_radius: 6.0,
            code_padding: 12.0,
            list_indent: 2.0,
            quote_indent: 1.0,
            focus_dim_opacity: 0.3,
            line_numbers_fg: rgb(0x99, 0x99, 0x99),
            source: SourceColors {
                heading: rgb(0x2E, 0x2A, 0x5A),
                marker: rgb(0x9a, 0xa0, 0xa6),
                code: rgb(0xb3, 0x45, 0x2f),
                link: rgb(0x2E, 0x2A, 0x5A),
                url: rgb(0x0a, 0x7a, 0x6b),
                html: rgb(0x8a, 0x4b, 0xa8),
                math: rgb(0x2a, 0x6e, 0xbb),
                meta: rgb(0x6a, 0x73, 0x7d),
            },
            version: 1,
        }
    }
}

impl Style {
    /// This style with text and spacing scaled by `zoom` (FS-040 editor zoom).
    pub fn scaled(&self, zoom: f32) -> Style {
        if (zoom - 1.0).abs() < 1e-3 {
            return self.clone();
        }
        let mut s = self.clone();
        s.size = (self.size * zoom).round();
        s.mono_size = (self.mono_size * zoom).round();
        s.max_width = self.max_width * zoom;
        s.code_padding = self.code_padding * zoom;
        s.code_radius = self.code_radius * zoom;
        s.padding_y = self.padding_y * zoom;
        s.version = self
            .version
            .wrapping_mul(31)
            .wrapping_add((zoom * 100.0).round() as u64);
        s
    }
}
