# Changelog

All notable changes to Inkra are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow SemVer.

## [Unreleased]

## [1.0.0-beta.1] — 2026-10-15

The first public build: every milestone M0–M7 and the SPEC audit below, plus
the 1.0 wrap-up.

### 1.0 wrap-up
- Releases: GitHub Actions builds every package on every platform (Windows
  x64 and arm64 MSI and zip, a universal macOS dmg, Linux x86_64 and arm64
  AppImage, deb, rpm, Flatpak and tar.gz), installs and starts each on a fresh
  runner, and publishes them with `SHA256SUMS.txt`. Pre-release versions are
  written `1.0.0~beta.1` inside deb and rpm so that 1.0.0 upgrades them.
- Update check: against this repository; beta builds also see newer betas,
  and versions compare in SemVer order.
- Startup (DX12): the adapter that presented the first frame is remembered
  and preferred; a machine whose DX12 offers only the WARP software adapter
  switches to GL at the next start (and back if GL never presents); wgpu's
  indirect-draw validation shaders are skipped (−36 ms on the test machine).
- Fixes: inline formulas taller than the line sit on the text baseline;
  external changes are detected under symlinked folders (macOS `/var`,
  `/tmp`); Mermaid subgraphs sharing an id and hex colors with non-ASCII
  characters (diagrams and themes) no longer crash, nested subgraphs no
  longer push each other apart without bound, and nested `\left…\middle`
  groups are laid out in linear instead of exponential time — all found by
  fuzzing;
  a formula or diagram that fails to lay out shows its source instead of
  failing every frame; the native macOS menu is built only on the main thread.
- Security (NF-005): links never open `javascript:`, `vbscript:`, `data:`,
  program-starting Windows protocols, or programs (also through `file:` URLs);
  configured programs (Pandoc, mmdc, custom export commands) must be absolute
  paths or names on PATH; printing on Windows no longer puts the file name into
  a PowerShell script; Pandoc reads a private temporary file.
- Licenses (NF-011): egui's Ubuntu and Hack fonts are no longer shipped; the
  two emoji fonts Inkra uses are bundled with their licenses. `cargo deny`
  and `cargo audit` run in CI.
- Quality: cargo-fuzz targets for Markdown (full and incremental parsing),
  HTML → Markdown, math and diagrams; same-runner A/B benchmarks in CI.

### SPEC audit follow-ups
- Themes: installed font families by name (`fonts.body/heading/mono`),
  `fonts.cjk_latin_spacing`, `colors.toolbar_bg`, `headings.auto_number`,
  `layout.radius/image_radius/task_done`, `[diagram]` colors and a
  `syntax.tmTheme` beside the theme; the CJK fallback follows the system
  language and a missing CJK font is reported.
- Windows: integrated title bar (menus, title and window buttons in one row,
  drag, double-click, edge resizing); `appearance.titleBar` switches back.
- Editor: code block language picker, `\[ \]` / `\( \)` math delimiters,
  clickable `\eqref`, a hint when `$…$` is typed with inline math off,
  `<details>` folding and the `background-color` / `font-size` style subset in
  HTML, strict emphasis option, `other.md#heading` links opening in the current
  window, Emoji & Symbols picker, smooth typewriter scrolling.
- Files: copy file path, reveal in sidebar and move to in the File menu,
  deleted-file marker in the title, keep running after the last window
  (`general.lastWindowClosed`), session restore of sidebar, scroll position
  and fullscreen, startup folder, word count on hover.
- Export: quick export next to the document (`file.quickExport`), image
  background color, an error dialog with Pandoc's full output and a progress
  note while exporting.
- Preferences: About page, settings import/export on General, theme folder
  button, "Reload window" for the interface and spelling languages.
- Accessibility: visible blocks as AccessKit nodes with roles and heading
  levels; Enter confirms dialogs; localized file dialog filters.
- Quality: typing-latency benchmark with a p95 regression gate, a 1 MB random
  input parser test; `cargo xtask dist --target` for arm64 MSI and a universal
  dmg; a Flatpak manifest; XWayland fallback (`INKRA_WAYLAND=1` for native
  Wayland).

### M7 — Polish and 1.0
- Preferences window with every setting of SPEC chapter 9, search, import /
  export / reset, and a keymap editor (record, conflicts, reset);
  `keybindings.json` overrides and removals.
- File menu: properties (encoding, line endings, counts), rename, move, delete
  to the recycle bin; read-only files say so and save elsewhere.
- Formatting toolbar; visible whitespace in source mode; drag and drop of text
  and of files from the sidebar.
- Image operations: copy, copy address, reveal, open, reload, make local, zoom,
  properties, delete; selection frame with a resize handle.
- Remote images and an update check over HTTPS (system trust store).
- Help documents (quick start, Markdown reference, themes) in Chinese and
  English; welcome panel (off by default).
- Reduced motion, save and export toasts, text column width / centering /
  auto-hiding scrollbar, interface font size, first-line indent, code line
  numbers and no-wrap code, preserved blank lines, image captions.
- AccessKit node for the document (text and caret position).
- `inkra-cli inspect`, `lint`, `fmt`, `run`, Pandoc formats in `convert`.
- `cargo xtask dist`: tar.gz, deb, rpm, AppImage, zip, MSI, app/dmg with
  Markdown file associations; smaller release binary.

### M6 — Export and import
- HTML export with the theme's styles (only the rules in use) or as a plain
  fragment: heading anchors, `[TOC]`, footnotes, highlighted code, SVG formulas
  (or TeX for MathJax pages), SVG diagrams, alerts; image paths kept, absolute or
  embedded. Copy as HTML uses the same generator.
- PDF export with krilla: the live layout paginated (keep-with-next, no widows or
  orphans, repeated table headers, split code blocks, page-fitting images),
  selectable and searchable text with subset fonts, vector math and diagrams,
  links and internal jumps, bookmarks from headings, title/author metadata,
  header/footer templates; page setup dialog; print via PDF.
- Image export (long PNG or one image per page, 1x–3x); formulas and diagrams
  copy as SVG/PNG and export as images.
- Pandoc bridge (docx, odt, rtf, epub, LaTeX, MediaWiki, rst, Textile, OPML, Org)
  with Mermaid pre-rendered; import through Pandoc (media into `assets/`) or the
  built-in HTML converter; dropping such files offers to import; custom export
  commands.
- `inkra-cli convert in.md out.pdf|out.html|out.png`.

### M5 — Math, diagrams and extensions
- Math: own TeX engine on the OpenType MATH table of STIX Two Math — fractions,
  radicals, big operators with limits, stretchy delimiters (variants and
  assemblies), matrices, cases, aligned/gathered, accents, braces, colors, boxes,
  `\text` with CJK; inline and display math, AMS/all numbering, `\tag`,
  `\label`/`\eqref`; unsupported commands in red. 233-formula corpus.
- Diagrams: Mermaid flowchart (all node shapes, edge styles, labels, chains, `&`,
  subgraphs, styles; layered layout), sequence (participants, actors, six arrows,
  notes, blocks, activations, autonumber) and pie; theme palette; parse errors with
  line numbers over the last good rendering; optional `mmdc` for other types.
- Footnotes numbered by first reference with hover bubbles and jumps; `[TOC]`;
  HTML block subset and inline HTML formatting; SVG images (resvg).
- Emoji shortcodes (gemoji) and completion; heading-link and footnote completion;
  smart punctuation; spell checking (spellbook, en_US) with suggestions.
- Commands: alerts, footnotes, link references, HTML blocks, clear format, style
  scope, move paragraph/list item.

### M4 — Tables, sidebar and search
- Tables: aligned source rewrite (CJK width 2), Tab/Shift+Tab/Enter navigation,
  `|` escaping, protected pipes, toolbar and table menu, insert/delete/move rows and
  columns, alignment, numeric sort, resize dialog, copy as Markdown/HTML/CSV, column
  width dragging (display only), drag reordering, horizontal scrolling of wide tables.
- Sidebar: outline (filter, collapse, current section, jump), article list (summaries,
  sorting), file tree (lazy, natural order, filters, drag & drop move, inline rename,
  recycle bin, context menu, keyboard navigation, reveal current file), global search
  (case/word/regex/globs, `.gitignore`, background, grouped results).
- Quick open (`Ctrl+P`) over a background file index and recent files.
- Word count details, remembered view flags and zoom, window positions in the session.
- Fix: incremental re-parsing no longer resynchronizes on a table split off a paragraph.

### M3 — Live rendering MVP
- Live view: documents are flattened into leaf blocks with container frames (quotes,
  list items, footnotes); inline markers are shown only for the node under the caret;
  headings, emphasis, code spans, links, images, task checkboxes, alerts, code blocks
  (syntect highlighting, loaded in the background), tables (grid) and rules render.
- Live editing semantics: block-aware Enter/Backspace/Delete, `Ctrl+Enter`, typing
  ```` ``` ````/`$$`/`|a|b|` + Enter expands into blocks, caret snapping over hidden
  syntax, a new paragraph below a trailing code block on Down.
- Formatting and paragraph commands (SPEC 3.3/3.4 P0), right-click menu, copy as
  Markdown/HTML/plain text/HTML code.
- Files: multi-window, open/recent/drag & drop, atomic save, autosave, recovery every
  5 s with restore dialog, close confirmation, session restore, external change
  detection with banner, window titles with `●`.
- Pasting bitmaps saves `image-<ts>.png` into `./<doc>.assets/`; image files can be
  copied per the image preferences.
- Themes: TOML themes with inheritance and color references; six original built-in
  themes (Paper, Ink, Journal, Mono, Slate, Serif), Auto, hot reload of user themes.
- View: focus mode, typewriter mode, fullscreen, always on top, status/menu bar
  toggles, editor zoom 50–300% (`Ctrl+wheel`), performance overlay.
- Robustness: panics in a frame are caught, unsaved content goes to recovery; the
  watcher ignores reads and unrelated files so an idle editor uses no CPU.

### M2 — Inline parsing and source mode
- Inline parser completes CommonMark (652/652) and GFM extension examples.
- Source-mode editor: custom caret/selection/IME over the rope, Markdown syntax
  highlighting, line numbers, auto-pairing, list continuation and renumbering, Tab
  indentation, find and replace (regex, `$1`), clipboard with HTML conversion.

### M1 — Text kernel
- Incremental Markdown parser with exact source ranges, 1 MB single-char edits in
  ~10 µs; byte-exact round trip of 1673 real-world files; transactions and grouped
  undo; encoding (UTF-8/16, GBK, GB18030) and line ending detection.

### M0 — Skeleton
- Cargo workspace with the 14 library crates of SPEC 2.2, the `inkra` and `inkra-cli`
  apps and `xtask`; workspace-wide `forbid(unsafe_code)`.
- `cargo xtask ci | layering | commands | prefs | keybindings | docs | bundle-macos`.
  The layering check also enforces: no web-runtime crates in any native target graph,
  read-only Markdown parsers only as dev-dependencies, reviewed dependency allowlist,
  workspace lints on every member.
- Command registry (CM-001) with panic isolation at the command boundary (NF-002);
  sample commands `app.quit`, `app.about`, `view.toggleSidebar`.
- Default key map of SPEC 10.2 (UI-010) and the complete menu model of SPEC 10.1
  (UI-001/UI-002); unimplemented items are greyed out. Native `muda` menu on macOS.
- Preferences in `conf.user.json` with atomic writes, unknown-key preservation and
  corrupt-file backup (PR-001, PR-003).
- Fluent i18n with `en-US` and `zh-CN`; UI language follows the system (PR-018).
- Rotating log file `<data_dir>/logs/inkra.log`, 3 × 16 MiB, `RUST_LOG` override (NF-008).
- Window/taskbar icon from `assets/brand/inkra-256.png`, Windows `.ico` resource,
  AppUserModelID / app id `io.inkra.Inkra`, macOS `Info.plist`, Linux `.desktop` (UI-050).
- `inkra --version`, `--help`, `--measure-startup` (prints `startup_ms=` and exits).

### Deviations
- `apps/inkra` relaxes `unsafe_code` from `forbid` to `deny` for two Win32 FFI calls in
  `src/platform_windows.rs` (AttachConsole so `--version` prints in a terminal, and
  SetCurrentProcessExplicitAppUserModelID). See SPEC AD-6 backfill.
