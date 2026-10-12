# Changelog

All notable changes to Inkra are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow SemVer.

## [Unreleased]

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
