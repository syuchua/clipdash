#!/usr/bin/env bash
set -euo pipefail

UI_BIN="$HOME/.local/bin/clipdash-ui"
if [[ ! -x "$UI_BIN" ]]; then
  echo "[clipdash] $UI_BIN not found. Build/install UI first:"
  echo "         CLIPDASH_WITH_GTK=1 bash scripts/install_dev.sh"
  exit 1
fi

if ! command -v gsettings >/dev/null 2>&1; then
  echo "gsettings not found. This script only supports GNOME." >&2
  exit 1
fi

SCHEMA="org.gnome.settings-daemon.plugins.media-keys"
BASE="/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings"

EXISTING=$(gsettings get "$SCHEMA" custom-keybindings)
EXISTING=${EXISTING#@as }

# Flatten to newline-separated list of paths
LIST=$(echo "$EXISTING" | sed -e "s/^\[//" -e "s/\]$//" -e "s/'//g" -e 's/, /\n/g')

echo "[clipdash] Disabling old Super+V bindings that point to clipdash menu" 
while IFS= read -r P; do
  [[ -z "$P" ]] && continue
  BIND=$(gsettings get "$SCHEMA.custom-keybinding:$P" binding | tr -d "'")
  CMD=$(gsettings get "$SCHEMA.custom-keybinding:$P" command | tr -d "'")
  if [[ "$BIND" == "<Super>v" ]] && [[ "$CMD" == *"clipdash menu"* ]]; then
    gsettings set "$SCHEMA.custom-keybinding:$P" binding "''" || true
    echo " - disabled $P ($CMD)"
  fi
done <<< "$LIST"

echo "[clipdash] Rebinding Super+V to $UI_BIN"
"$(dirname "$0")/gnome_bind_super_v.sh" "<Super>v" "$UI_BIN" "Clipdash UI"
echo "[clipdash] Done. Test with Super+V or run $UI_BIN"

