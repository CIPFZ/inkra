#!/bin/sh
# Installs Inkra from this folder into ~/.local (or PREFIX), registering it
# for Markdown files.
set -e
PREFIX="${PREFIX:-$HOME/.local}"
HERE="$(cd "$(dirname "$0")" && pwd)"
mkdir -p "$PREFIX"
cp -R "$HERE/bin" "$HERE/share" "$PREFIX/"
command -v update-desktop-database >/dev/null && update-desktop-database -q "$PREFIX/share/applications" || true
command -v update-mime-database >/dev/null && update-mime-database "$PREFIX/share/mime" || true
command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q -t "$PREFIX/share/icons/hicolor" || true
echo "Inkra installed to $PREFIX/bin/inkra"
