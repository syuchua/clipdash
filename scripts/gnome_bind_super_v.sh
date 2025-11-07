#!/usr/bin/env bash
set -euo pipefail

# Usage: gnome_bind_super_v.sh [binding] [command] [name]
# Defaults: binding='<Super>v', command='clipdash menu', name='Clipdash Menu'

BINDING=${1:-"<Super>v"}
CMD=${2:-"clipdash menu"}
NAME=${3:-"Clipdash Menu"}

if ! command -v gsettings >/dev/null 2>&1; then
  echo "gsettings not found. This script only supports GNOME." >&2
  exit 1
fi

SCHEMA="org.gnome.settings-daemon.plugins.media-keys"
BASE="/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings"

# Read existing list (e.g. "['/.../custom0/', '/.../custom1/']" or "[]")
EXISTING=$(gsettings get "$SCHEMA" custom-keybindings)
# Strip optional type prefix '@as '
EXISTING=${EXISTING#@as }

# Pick next available customN path without parsing the list structure
IDX=0
while true; do
  PATH_ENTRY="$BASE/custom$IDX/"
  if [[ "$EXISTING" == *"'$PATH_ENTRY'"* ]]; then
    IDX=$((IDX+1))
  else
    break
  fi
done

# Append the new path to the list with correct quoting
if [[ "$EXISTING" == "[]" ]]; then
  NEW_LIST="['$PATH_ENTRY']"
else
  # ensure EXISTING ends with ']' and is a proper list
  NEW_LIST=${EXISTING%]}
  NEW_LIST="$NEW_LIST, '$PATH_ENTRY']"
fi

echo "[clipdash] Creating GNOME keybinding at $PATH_ENTRY"
gsettings set "$SCHEMA" custom-keybindings "$NEW_LIST"
gsettings set "$SCHEMA.custom-keybinding:$PATH_ENTRY" name "$NAME"
gsettings set "$SCHEMA.custom-keybinding:$PATH_ENTRY" command "$CMD"
gsettings set "$SCHEMA.custom-keybinding:$PATH_ENTRY" binding "$BINDING"

echo "[clipdash] Bound $BINDING to '$CMD'"
