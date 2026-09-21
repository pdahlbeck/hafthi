#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PREFIX="${PREFIX:-$HOME/.local}"
BIN_DIR="$PREFIX/bin"
APP_DIR="$HOME/.local/share/applications"
ICON_DIR="$HOME/.local/share/icons/hicolor/scalable/apps"

echo "Building Hafþi..."
cargo build --release --manifest-path "$ROOT_DIR/Cargo.toml"

mkdir -p "$BIN_DIR" "$APP_DIR" "$ICON_DIR"

install -m 755 "$ROOT_DIR/target/release/hafthi" "$BIN_DIR/hafthi"
install -m 644 "$ROOT_DIR/assets/hafthi.svg" "$ICON_DIR/hafthi.svg"
# Desktop entries do not expand $HOME or shell-style single quotes in Exec.
# Write the installed binary's absolute path so launchers can start it.
awk -v binary="$BIN_DIR/hafthi" '
  /^Exec=/ { print "Exec=\"" binary "\""; next }
  { print }
' "$ROOT_DIR/assets/hafthi.desktop" > "$APP_DIR/hafthi.desktop"

if command -v desktop-file-validate >/dev/null 2>&1; then
  desktop-file-validate "$APP_DIR/hafthi.desktop"
fi

if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database "$APP_DIR" >/dev/null 2>&1 || true
fi

if command -v gtk-update-icon-cache >/dev/null 2>&1; then
  gtk-update-icon-cache -f -t "$HOME/.local/share/icons/hicolor" >/dev/null 2>&1 || true
fi

echo
echo "Hafþi installed."
echo "Binary:  $BIN_DIR/hafthi"
echo "Launcher: $APP_DIR/hafthi.desktop"
echo "Icon:     $ICON_DIR/hafthi.svg"
echo
echo "If your launcher is already open, close and reopen it to refresh the application list."
