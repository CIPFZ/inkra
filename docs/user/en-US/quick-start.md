# Inkra Quick Start

Inkra is a WYSIWYM Markdown editor: you write plain Markdown and see a formatted document. Markdown syntax appears where the caret is and hides again when you move on.

[TOC]

## Writing

- Just type. A line starting with `# ` becomes a heading, `- ` a list, `> ` a quote; three backticks start a code block.
- Select text and press `Ctrl+B` for bold, `Ctrl+I` for italic, `Ctrl+K` for a link.
- `Ctrl+T` inserts a table; inside a table `Tab` moves to the next cell and `Enter` adds a row.
- Type `$$` and Enter for a formula block; ```` ```mermaid ```` for a diagram.
- Paste or drop images: they are copied into an `assets` folder next to the document (configurable in Preferences).
- `Ctrl+/` switches between the live view and source mode.

## Files and folders

- `Ctrl+N` new window, `Ctrl+O` open a file, `Ctrl+Shift+O` open a folder (quick open on macOS).
- With a folder open, the sidebar (`Ctrl+Shift+L`) shows the outline, an article list, the file tree and global search.
- `Ctrl+P` quick open: type a few letters of a file name to jump to it.
- Unsaved changes are written to a recovery area regularly; after a crash Inkra offers to restore them.

## View

- Focus mode (`F8`) dims everything but the current paragraph; typewriter mode (`F9`) keeps the current line in the middle.
- `Ctrl+Shift+=` / `Ctrl+Shift+-` zoom, `Ctrl+Shift+0` resets (or hold `Ctrl` and turn the mouse wheel).
- The Themes menu switches themes; you can add your own (see "Custom Themes").

## Export

File → Export writes HTML, PDF or images; with [Pandoc](https://pandoc.org/installing.html) installed also Word, EPUB, LaTeX and more. File → Page Setup sets paper, margins, header and footer.

## Settings and shortcuts

`Ctrl+,` opens Preferences. Every menu command can be rebound on the Keymap page. Help → Keyboard Shortcuts lists all commands.
