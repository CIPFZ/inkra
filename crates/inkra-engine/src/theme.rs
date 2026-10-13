//! Themes (TH-001 … TH-003): declarative TOML files resolved into a layout
//! [`Style`]. Built-in themes are compiled in; user themes live in
//! `<config_dir>/themes/<name>/theme.toml` (or `<name>.toml`) and may inherit a
//! built-in one with `base`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use inkra_layout::Style;
use inkra_layout::epaint::Color32;
use inkra_layout::fonts::FontKind;
use toml::Value;

/// Built-in themes: `(name, TOML source)`. `Paper` is the default.
pub const BUILTIN: &[(&str, &str)] = &[
    ("Paper", include_str!("../../../themes/Paper/theme.toml")),
    ("Ink", include_str!("../../../themes/Ink/theme.toml")),
    (
        "Journal",
        include_str!("../../../themes/Journal/theme.toml"),
    ),
    ("Mono", include_str!("../../../themes/Mono/theme.toml")),
    ("Slate", include_str!("../../../themes/Slate/theme.toml")),
    ("Serif", include_str!("../../../themes/Serif/theme.toml")),
];

/// Where a theme comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    /// Compiled in.
    Builtin,
    /// A user file.
    File(PathBuf),
}

/// An installed theme.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemeEntry {
    /// Name (from `[meta] name`, else the folder/file name).
    pub name: String,
    /// Dark theme.
    pub dark: bool,
    /// Origin.
    pub origin: Origin,
}

fn parse(src: &str) -> Result<toml::Table, String> {
    src.parse::<toml::Table>().map_err(|e| e.to_string())
}

fn meta_name(t: &toml::Table) -> Option<String> {
    t.get("meta")?.get("name")?.as_str().map(str::to_owned)
}

fn meta_dark(t: &toml::Table) -> bool {
    t.get("meta")
        .and_then(|m| m.get("dark"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn user_theme_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            let f = p.join("theme.toml");
            if f.is_file() {
                out.push(f);
            }
        } else if p.extension().is_some_and(|x| x == "toml") {
            out.push(p);
        }
    }
    out.sort();
    out
}

/// All installed themes (built-in first, then user themes).
pub fn list(user_dir: Option<&Path>) -> Vec<ThemeEntry> {
    let mut out: Vec<ThemeEntry> = BUILTIN
        .iter()
        .map(|(n, src)| ThemeEntry {
            name: (*n).to_owned(),
            dark: parse(src).map(|t| meta_dark(&t)).unwrap_or(false),
            origin: Origin::Builtin,
        })
        .collect();
    if let Some(dir) = user_dir {
        for f in user_theme_files(dir) {
            let Ok(src) = std::fs::read_to_string(&f) else {
                continue;
            };
            let Ok(t) = parse(&src) else { continue };
            let fallback = if f.file_name().is_some_and(|n| n == "theme.toml") {
                f.parent().and_then(|p| p.file_name())
            } else {
                f.file_stem()
            };
            let name = meta_name(&t)
                .or_else(|| fallback.map(|n| n.to_string_lossy().into_owned()))
                .unwrap_or_default();
            if name.is_empty() {
                continue;
            }
            out.retain(|e| e.name != name || e.origin != Origin::Builtin);
            out.push(ThemeEntry {
                name,
                dark: meta_dark(&t),
                origin: Origin::File(f),
            });
        }
    }
    out
}

fn source_of(name: &str, user_dir: Option<&Path>) -> Option<(String, Origin)> {
    if let Some(dir) = user_dir {
        for e in list(Some(dir)) {
            if e.name == name
                && let Origin::File(p) = &e.origin
            {
                return std::fs::read_to_string(p)
                    .ok()
                    .map(|s| (s, e.origin.clone()));
            }
        }
    }
    BUILTIN
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, s)| ((*s).to_owned(), Origin::Builtin))
}

/// Deep-merges `over` into `base`.
fn merge(base: &mut toml::Table, over: toml::Table) {
    for (k, v) in over {
        match (base.get_mut(&k), v) {
            (Some(Value::Table(b)), Value::Table(o)) => merge(b, o),
            (_, v) => {
                base.insert(k, v);
            }
        }
    }
}

/// The merged TOML of `name` with its `base` chain resolved (Paper is always the
/// root, so every field has a value).
fn resolved_table(
    name: &str,
    user_dir: Option<&Path>,
    depth: usize,
) -> Result<toml::Table, String> {
    let (src, _) = source_of(name, user_dir).ok_or_else(|| format!("theme `{name}` not found"))?;
    let t = parse(&src)?;
    let base = t
        .get("meta")
        .and_then(|m| m.get("base"))
        .and_then(Value::as_str)
        .filter(|b| !b.is_empty())
        .map(str::to_owned);
    let mut out = match (base, depth) {
        (_, d) if d > 8 => return Err("theme inheritance too deep".into()),
        (Some(b), d) if !b.eq_ignore_ascii_case(name) => resolved_table(&b, user_dir, d + 1)?,
        _ if !name.eq_ignore_ascii_case("Paper") => resolved_table("Paper", None, depth + 1)?,
        _ => toml::Table::new(),
    };
    // `dark` and `base` are not inherited blindly: an explicit value wins.
    merge(&mut out, t);
    Ok(out)
}

/// Parses `#rgb`, `#rrggbb`, `#rrggbbaa`.
pub fn parse_color(s: &str) -> Option<Color32> {
    let h = s.trim().strip_prefix('#')?;
    let v = |i: usize, n: usize| u8::from_str_radix(&h[i..i + n], 16).ok();
    match h.len() {
        3 => {
            let d = |i: usize| v(i, 1).map(|x| x * 17);
            Some(Color32::from_rgb(d(0)?, d(1)?, d(2)?))
        }
        6 => Some(Color32::from_rgb(v(0, 2)?, v(2, 2)?, v(4, 2)?)),
        8 => Some(Color32::from_rgba_unmultiplied(
            v(0, 2)?,
            v(2, 2)?,
            v(4, 2)?,
            v(6, 2)?,
        )),
        _ => None,
    }
}

struct Colors<'a> {
    table: Option<&'a toml::Table>,
}

impl Colors<'_> {
    /// A color by key; values may reference other keys of `[colors]`.
    fn get(&self, key: &str) -> Option<Color32> {
        let mut k = key.to_owned();
        for _ in 0..8 {
            let v = self.table?.get(&k)?.as_str()?;
            if let Some(c) = parse_color(v) {
                return Some(c);
            }
            k = v.trim().to_owned();
        }
        None
    }

    fn resolve(&self, v: &str) -> Option<Color32> {
        parse_color(v).or_else(|| self.get(v))
    }
}

fn f32_of(t: &toml::Table, section: &str, key: &str) -> Option<f32> {
    let v = t.get(section)?.get(key)?;
    v.as_float()
        .map(|f| f as f32)
        .or_else(|| v.as_integer().map(|i| i as f32))
}

/// The first usable face of a font chain (TH-006): a bundled or generic name,
/// or an installed family; failing that, the bundled face closest to the first
/// name. Also returns the installed family's name, if one was chosen.
fn font_choice(t: &toml::Table, key: &str, fallback: FontKind) -> (FontKind, Option<String>) {
    let names: Vec<String> = t
        .get("fonts")
        .and_then(|f| f.get(key))
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    for n in &names {
        if let Some(k) = inkra_layout::fonts::resolve_name(n) {
            let name = matches!(k, FontKind::System(_)).then(|| n.clone());
            return (k, name);
        }
    }
    (
        names
            .iter()
            .find_map(|n| FontKind::from_name(n))
            .unwrap_or(fallback),
        None,
    )
}

/// Loads theme `name` into a [`Style`]. Unknown fields are ignored; invalid
/// values keep the inherited ones.
pub fn load(name: &str, user_dir: Option<&Path>) -> Result<Style, String> {
    let t = resolved_table(name, user_dir, 0)?;
    let mut s = style_of(name, &t)?;
    // TH-001: `syntax_theme` may name a TextMate scheme beside the theme file
    // (`syntax.tmTheme` by default).
    if let Some((_, Origin::File(p))) = source_of(name, user_dir)
        && let Some(dir) = p.parent()
    {
        let named = t
            .get("code")
            .and_then(|c| c.get("syntax_theme"))
            .and_then(Value::as_str)
            .filter(|n| n.ends_with(".tmTheme"));
        let file = dir.join(named.unwrap_or("syntax.tmTheme"));
        if file.is_file() {
            match inkra_highlight::load_theme_file(&file) {
                Ok(n) => {
                    use std::hash::{Hash, Hasher};
                    let mut h = std::collections::hash_map::DefaultHasher::new();
                    s.version.hash(&mut h);
                    n.hash(&mut h);
                    s.version = h.finish() | 1;
                    s.syntax_theme = n;
                }
                Err(e) => log::warn!("syntax theme: {e}"),
            }
        }
    }
    Ok(s)
}

fn style_of(name: &str, t: &toml::Table) -> Result<Style, String> {
    let t = t.clone();
    let (body, body_name) = font_choice(&t, "body", FontKind::Sans);
    let (mono, mono_name) = font_choice(&t, "mono", FontKind::Mono);
    let mut s = Style {
        name: name.to_owned(),
        dark: meta_dark(&t),
        body_font: body,
        mono_font: mono,
        font_names: [body_name, None, mono_name],
        ..Style::default()
    };
    if let Some(v) = t
        .get("fonts")
        .and_then(|f| f.get("cjk_latin_spacing"))
        .and_then(Value::as_bool)
    {
        s.cjk_latin_spacing = v;
    }
    s.heading_font = {
        let empty = t
            .get("fonts")
            .and_then(|f| f.get("heading"))
            .and_then(Value::as_array)
            .is_none_or(|a| a.is_empty());
        if empty {
            s.font_names[1] = s.font_names[0].clone();
            s.body_font
        } else {
            let (k, n) = font_choice(&t, "heading", s.body_font);
            s.font_names[1] = n;
            k
        }
    };
    if let Some(v) = f32_of(&t, "fonts", "size") {
        s.size = v.clamp(8.0, 40.0);
        s.mono_size = (s.size * 0.875).round();
    }
    if let Some(v) = f32_of(&t, "fonts", "line_height") {
        s.line_height = v.clamp(1.0, 3.0);
    }
    if let Some(v) = f32_of(&t, "fonts", "paragraph_spacing") {
        s.paragraph_spacing = v.clamp(0.0, 4.0);
    }
    let c = Colors {
        table: t.get("colors").and_then(Value::as_table),
    };
    macro_rules! color {
        ($($field:ident),*) => { $( if let Some(v) = c.get(stringify!($field)) { s.$field = v; } )* };
    }
    color!(
        bg,
        fg,
        fg_muted,
        accent,
        selection,
        cursor,
        code_bg,
        code_border,
        quote_border,
        quote_fg,
        hr,
        table_border,
        table_header_bg,
        table_row_alt_bg,
        highlight_bg,
        mark_fg,
        search_match_bg,
        search_current_bg,
        spell_error,
        sidebar_bg,
        sidebar_fg,
        sidebar_active_bg,
        statusbar_fg,
        toolbar_bg
    );
    if c.get("toolbar_bg").is_none() {
        s.toolbar_bg = s.sidebar_bg;
    }
    for (i, k) in [
        "alert_note",
        "alert_tip",
        "alert_important",
        "alert_warning",
        "alert_caution",
    ]
    .iter()
    .enumerate()
    {
        if let Some(v) = c.get(k) {
            s.alerts[i] = v;
        }
    }
    if let Some(h) = t.get("headings").and_then(Value::as_table) {
        for level in 0..6 {
            let Some(e) = h.get(&format!("h{}", level + 1)).and_then(Value::as_table) else {
                continue;
            };
            let hs = &mut s.headings[level];
            let num = |k: &str| {
                e.get(k).and_then(|v| {
                    v.as_float()
                        .map(|f| f as f32)
                        .or_else(|| v.as_integer().map(|i| i as f32))
                })
            };
            if let Some(v) = num("size") {
                hs.size = v.clamp(0.5, 4.0);
            }
            if let Some(v) = num("margin_top") {
                hs.margin_top = v.clamp(0.0, 4.0);
            }
            if let Some(v) = num("margin_bottom") {
                hs.margin_bottom = v.clamp(0.0, 4.0);
            }
            if let Some(v) = e.get("border_bottom").and_then(Value::as_bool) {
                hs.border_bottom = v;
            }
            if let Some(w) = e.get("weight").and_then(Value::as_str) {
                hs.bold = w != "normal";
            }
            if let Some(col) = e.get("color").and_then(Value::as_str) {
                hs.color = c.resolve(col);
            }
        }
        if let Some(v) = h.get("auto_number").and_then(Value::as_bool) {
            s.heading_numbers = v;
        }
    }
    if let Some(code) = t.get("code").and_then(Value::as_table) {
        if let Some(st) = code.get("syntax_theme").and_then(Value::as_str) {
            s.syntax_theme = st.to_owned();
        }
        if let Some(v) = code
            .get("line_numbers_fg")
            .and_then(Value::as_str)
            .and_then(|v| c.resolve(v))
        {
            s.line_numbers_fg = v;
        }
    }
    if let Some(v) = f32_of(&t, "code", "radius") {
        s.code_radius = v.clamp(0.0, 20.0);
    }
    if let Some(v) = f32_of(&t, "code", "padding") {
        s.code_padding = v.clamp(0.0, 40.0);
    }
    if let Some(v) = f32_of(&t, "layout", "max_width") {
        s.max_width = v.clamp(300.0, 4000.0);
    }
    if let Some(v) = f32_of(&t, "layout", "padding_x") {
        s.padding_x = v.clamp(0.0, 200.0);
    }
    if let Some(v) = f32_of(&t, "layout", "padding_y") {
        s.padding_y = v.clamp(0.0, 200.0);
    }
    if let Some(v) = f32_of(&t, "layout", "list_indent") {
        s.list_indent = v.clamp(0.5, 6.0);
    }
    if let Some(v) = f32_of(&t, "layout", "quote_indent") {
        s.quote_indent = v.clamp(0.2, 6.0);
    }
    if let Some(v) = f32_of(&t, "layout", "focus_dim_opacity") {
        s.focus_dim_opacity = v.clamp(0.0, 1.0);
    }
    if let Some(v) = f32_of(&t, "layout", "radius") {
        s.radius = v.clamp(0.0, 20.0);
    }
    if let Some(v) = f32_of(&t, "layout", "image_radius") {
        s.image_radius = v.clamp(0.0, 40.0);
    }
    if let Some(v) = t
        .get("layout")
        .and_then(|l| l.get("task_done"))
        .and_then(Value::as_str)
    {
        s.task_done = match v {
            "strike" => 1,
            "both" => 2,
            _ => 0,
        };
    }
    // MD-102: diagram colors (`[diagram]`).
    if let Some(d) = t.get("diagram").and_then(Value::as_table) {
        for (k, key) in ["primary", "primary_border", "line", "text"]
            .iter()
            .enumerate()
        {
            if let Some(v) = d
                .get(*key)
                .and_then(Value::as_str)
                .and_then(|v| c.resolve(v))
            {
                s.diagram_colors[k] = Some(v);
            }
        }
    }
    // Source-mode highlighting follows the theme's palette.
    s.source.heading = s.accent;
    s.source.link = s.accent;
    s.source.marker = s.mark_fg;
    s.source.meta = s.fg_muted;
    if s.dark {
        s.source.code = Color32::from_rgb(0xff, 0x9e, 0x80);
        s.source.url = Color32::from_rgb(0x6c, 0xd4, 0xc0);
        s.source.html = Color32::from_rgb(0xd0, 0x9c, 0xf0);
        s.source.math = Color32::from_rgb(0x7c, 0xb7, 0xff);
    }
    s.version = style_version(name, &t);
    Ok(s)
}

fn style_version(name: &str, t: &toml::Table) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    name.hash(&mut h);
    format!("{t:?}").hash(&mut h);
    h.finish() | 1
}

/// The file of a user theme (for hot reload), if `name` is one.
pub fn file_of(name: &str, user_dir: Option<&Path>) -> Option<PathBuf> {
    list(user_dir)
        .into_iter()
        .find(|e| e.name == name)
        .and_then(|e| match e.origin {
            Origin::File(p) => Some(p),
            Origin::Builtin => None,
        })
}

/// Names of built-in themes grouped by brightness.
pub fn builtin_names() -> BTreeMap<bool, Vec<&'static str>> {
    let mut m: BTreeMap<bool, Vec<&'static str>> = BTreeMap::new();
    for (n, src) in BUILTIN {
        let dark = parse(src).map(|t| meta_dark(&t)).unwrap_or(false);
        m.entry(dark).or_default().push(n);
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_load() {
        for (name, _) in BUILTIN {
            let s = load(name, None).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(&s.name, name);
        }
        let paper = load("Paper", None).unwrap();
        assert_eq!(paper.bg, Color32::from_rgb(0xFA, 0xF8, 0xF3), "brand Paper");
        assert_eq!(
            paper.accent,
            Color32::from_rgb(0x2E, 0x2A, 0x5A),
            "brand Ink"
        );
        assert_eq!(
            paper.cursor,
            Color32::from_rgb(0xFF, 0x6B, 0x57),
            "brand Coral"
        );
        let ink = load("Ink", None).unwrap();
        assert!(ink.dark);
        assert_eq!(
            ink.bg,
            Color32::from_rgb(0x1E, 0x1B, 0x33),
            "brand Paper Dark"
        );
        assert_eq!(load("Journal", None).unwrap().body_font, FontKind::Serif);
        assert_eq!(load("Mono", None).unwrap().body_font, FontKind::Mono);
        let slate = load("Slate", None).unwrap();
        assert!(slate.dark, "inherits dark from Ink");
        assert_eq!(builtin_names()[&true].len(), 2);
    }

    #[test]
    fn colors_and_user_themes() {
        assert_eq!(
            parse_color("#abc"),
            Some(Color32::from_rgb(0xaa, 0xbb, 0xcc))
        );
        assert_eq!(
            parse_color("#11223344"),
            Some(Color32::from_rgba_unmultiplied(0x11, 0x22, 0x33, 0x44))
        );
        assert_eq!(parse_color("red"), None);
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("Mine")).unwrap();
        std::fs::write(
            dir.path().join("Mine/theme.toml"),
            "[meta]\nname = \"Mine\"\nbase = \"Ink\"\n[colors]\naccent = \"#ff0000\"\nfg = \"accent\"\n",
        )
        .unwrap();
        let names: Vec<String> = list(Some(dir.path())).into_iter().map(|e| e.name).collect();
        assert!(names.contains(&"Mine".to_owned()));
        let s = load("Mine", Some(dir.path())).unwrap();
        assert_eq!(s.fg, Color32::from_rgb(255, 0, 0), "color reference");
        assert!(s.dark, "dark inherited from Ink");
        assert!(load("Nope", None).is_err());
        std::fs::write(dir.path().join("Bad.toml"), "not = [toml").unwrap();
        assert!(list(Some(dir.path())).iter().all(|e| e.name != "Bad"));
    }

    #[test]
    fn every_documented_field_is_read() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("Full")).unwrap();
        std::fs::write(
            dir.path().join("Full/theme.toml"),
            "[fonts]\nmono = [\"Nonexistent Mono\", \"monospace\"]\nbody = [\"serif\"]\ncjk_latin_spacing = true\n\
             [colors]\ntoolbar_bg = \"#123456\"\n[headings]\nauto_number = true\n\
             [layout]\nradius = 8.0\nimage_radius = 6.0\ntask_done = \"strike\"\n\
             [diagram]\nprimary = \"#ececff\"\nline = \"fg\"\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("Full/syntax.tmTheme"),
            r#"<?xml version="1.0" encoding="UTF-8"?><plist version="1.0"><dict><key>name</key><string>T</string><key>settings</key><array><dict><key>settings</key><dict><key>foreground</key><string>#112233</string><key>background</key><string>#ffffff</string></dict></dict></array></dict></plist>"#,
        )
        .unwrap();
        let s = load("Full", Some(dir.path())).unwrap();
        assert_eq!(s.body_font, FontKind::Serif);
        assert_eq!(s.mono_font, FontKind::Mono);
        assert!(s.cjk_latin_spacing && s.heading_numbers);
        assert_eq!(s.toolbar_bg, Color32::from_rgb(0x12, 0x34, 0x56));
        assert_eq!((s.radius, s.image_radius, s.task_done), (8.0, 6.0, 1));
        assert_eq!(
            s.diagram_colors[0],
            Some(Color32::from_rgb(0xec, 0xec, 0xff))
        );
        assert_eq!(s.diagram_colors[2], Some(s.fg));
        assert!(s.syntax_theme.starts_with("file:"), "{}", s.syntax_theme);
        // Without `toolbar_bg`, the toolbar matches the sidebar.
        let p = load("Paper", None).unwrap();
        assert_eq!(p.toolbar_bg, p.sidebar_bg);
    }
}
