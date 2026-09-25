#!/usr/bin/env bash
set -euo pipefail

PREFIX="${PREFIX:-$HOME/.local}"

rm -f "$PREFIX/bin/hafthi"
rm -f "$PREFIX/libexec/hafthi/g"
rmdir "$PREFIX/libexec/hafthi" 2>/dev/null || true
rm -f "$HOME/.local/share/applications/hafthi.desktop"
rm -f "$HOME/.local/share/icons/hicolor/scalable/apps/hafthi.svg"

if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database "$HOME/.local/share/applications" >/dev/null 2>&1 || true
fi

if command -v gtk-update-icon-cache >/dev/null 2>&1; then
  gtk-update-icon-cache -f -t "$HOME/.local/share/icons/hicolor" >/dev/null 2>&1 || true
fi

echo "Hafþi application files removed."
echo "User configuration in ~/.config/hafthi was left untouched."
