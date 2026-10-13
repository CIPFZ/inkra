# Custom Themes

A theme is a folder containing a `theme.toml`. Put the folder into the themes directory (Themes → Open Theme Folder) and it appears in the Themes menu; saving `theme.toml` applies the change immediately.

## Start from an existing theme

The easiest way is to inherit a built-in theme and change only what you need:

```toml
[meta]
name = "My Theme"
author = "Me"
version = "1.0"
dark = false
base = "Paper"

[colors]
accent = "#0b6e4f"
cursor = "#e4572e"
```

Built-in themes: Paper (default), Ink (dark), Journal, Mono, Slate, Serif.

## Sections

| Section | Keys | Meaning |
|---|---|---|
| `[meta]` | `name` `author` `version` `dark` `base` | `dark` selects dark UI chrome; `base` is the theme to inherit |
| `[fonts]` | `body` `heading` `mono` | Font family lists, tried in order; Inter, Source Serif 4 and JetBrains Mono are bundled |
|  | `size` `line_height` `paragraph_spacing` | Body size (pt), line height factor, paragraph spacing (em) |
| `[colors]` | `bg` `fg` `fg_muted` `accent` `selection` `cursor` | Background, text, secondary text, accent (links…), selection, caret |
|  | `code_bg` `code_border` `quote_border` `quote_fg` `hr` | Code blocks, quotes, rules |
|  | `table_border` `table_header_bg` `table_row_alt_bg` | Tables |
|  | `highlight_bg` `mark_fg` `search_match_bg` `search_current_bg` `spell_error` | Highlights, revealed syntax, search, spelling |
|  | `sidebar_bg` `sidebar_fg` `sidebar_active_bg` `toolbar_bg` `statusbar_fg` | Sidebar and status bar |
|  | `alert_note` `alert_tip` `alert_important` `alert_warning` `alert_caution` | Alert colors |
| `[headings]` | `h1`…`h6` | `{ size, weight, margin_top, margin_bottom, border_bottom, color }`; `color` is a color or a color key |
| `[code]` | `syntax_theme` `radius` `padding` `line_numbers_fg` | Highlighting theme (syntect built-ins such as InspiredGitHub, base16-ocean.dark), corner radius, padding |
| `[layout]` | `max_width` `padding_x` `padding_y` `list_indent` `quote_indent` `focus_dim_opacity` | Text column width, margins, indents, focus mode dimming |

Colors are `#rrggbb` or `#rrggbbaa` with alpha. If a theme has an error, Inkra keeps the current theme and reports the line.

## Export

HTML and PDF exports use the current theme (or the one chosen under Preferences → Export): colors, fonts and heading styles carry over.
