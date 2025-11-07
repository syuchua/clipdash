#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")"/.. && pwd)"
cd "$ROOT"

echo "[clipdash] Building release binaries..."
cargo build --release -p clipdash-daemon -p clipdash-cli

BIN_DIR="$HOME/.local/bin"
mkdir -p "$BIN_DIR"

echo "[clipdash] Installing symlinks to $BIN_DIR"
ln -sf "$ROOT/target/release/clipdash-daemon" "$BIN_DIR/clipdash-daemon"
ln -sf "$ROOT/target/release/clipdash" "$BIN_DIR/clipdash"

APPS_DIR="$HOME/.local/share/applications"
AUTOSTART_DIR="$HOME/.config/autostart"
SYSTEMD_USER="$HOME/.config/systemd/user"
mkdir -p "$APPS_DIR" "$AUTOSTART_DIR" "$SYSTEMD_USER"

echo "[clipdash] Installing desktop entries"
install -m 0644 "$ROOT/packaging/clipdash-menu.desktop" "$APPS_DIR/clipdash-menu.desktop"
install -m 0644 "$ROOT/packaging/clipdash-daemon.desktop" "$AUTOSTART_DIR/clipdash-daemon.desktop"

echo "[clipdash] Installing systemd --user unit"
install -m 0644 "$ROOT/packaging/clipdashd.service" "$SYSTEMD_USER/clipdashd.service"
systemctl --user daemon-reload || true
systemctl --user enable --now clipdashd.service || true

echo "[clipdash] Done. You may bind a hotkey to 'clipdash menu' or '$ROOT/scripts/clipdash_menu.sh'."

