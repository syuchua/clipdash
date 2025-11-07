#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")"/.. && pwd)"

BIN_DIR="$HOME/.local/bin"
APPS_DIR="$HOME/.local/share/applications"
AUTOSTART_DIR="$HOME/.config/autostart"
SYSTEMD_USER="$HOME/.config/systemd/user"

echo "[clipdash] Disabling systemd --user service (if any)"
systemctl --user disable --now clipdashd.service || true

echo "[clipdash] Removing symlinks from $BIN_DIR"
rm -f "$BIN_DIR/clipdash-daemon" "$BIN_DIR/clipdash"

echo "[clipdash] Removing desktop entries"
rm -f "$APPS_DIR/clipdash-menu.desktop" "$AUTOSTART_DIR/clipdash-daemon.desktop"

echo "[clipdash] Removing systemd unit"
rm -f "$SYSTEMD_USER/clipdashd.service"
systemctl --user daemon-reload || true

echo "[clipdash] Uninstall complete (history file preserved)."

