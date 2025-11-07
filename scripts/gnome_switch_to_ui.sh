#!/usr/bin/env bash
set -euo pipefail

# Bind Super+V to the native GTK UI binary
# Usage: gnome_switch_to_ui.sh [binding]
# Default binding: <Super>v

BINDING=${1:-"<Super>v"}
ROOT="$(cd "$(dirname "$0")"/.. && pwd)"

if ! command -v gsettings >/dev/null 2>&1; then
  echo "gsettings not found. This script only supports GNOME." >&2
  exit 1
fi

if ! command -v clipdash-ui >/dev/null 2>&1; then
  echo "clipdash-ui not found in PATH. Build/install with UI first:" >&2
  echo "  CLIPDASH_WITH_GTK=1 bash $ROOT/scripts/install_dev.sh" >&2
  exit 1
fi

"$ROOT/scripts/gnome_bind_super_v.sh" "$BINDING" "clipdash-ui" "Clipdash UI"
echo "[clipdash] Super+V now bound to GTK UI"

