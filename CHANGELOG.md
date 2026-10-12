# Changelog

All notable changes to Inkra are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow SemVer.

## [Unreleased]

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
