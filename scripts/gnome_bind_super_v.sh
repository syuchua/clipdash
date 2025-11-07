#!/usr/bin/env bash
set -euo pipefail

# Best-effort GNOME custom keybinding to map Super+V to 'clipdash menu'
CMD="clipdash menu"
NAME="Clipdash Menu"
BINDING="<Super>v"

if ! command -v gsettings >/dev/null 2>&1; then
  echo "gsettings not found. This script only supports GNOME." >&2
  exit 1
fi

SCHEMA="org.gnome.settings-daemon.plugins.media-keys"
BASE="/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings"

# Read existing list
EXISTING=$(gsettings get "$SCHEMA" custom-keybindings)
# Normalize to bash array-ish by trimming brackets and quotes
trim_list() { echo "$1" | sed -e "s/^\[//" -e "s/\]$//" -e "s/'//g"; }
LIST=$(trim_list "$EXISTING")

# Find an unused slot
IDX=0
while true; do
  PATH_ENTRY="$BASE/custom$IDX/"
  case ",$LIST," in
    *",$PATH_ENTRY,"*) IDX=$((IDX+1));;
    *) break;;
  esac
done

NEW_LIST=$LIST
if [ -n "$NEW_LIST" ]; then NEW_LIST="$NEW_LIST, $PATH_ENTRY"; else NEW_LIST="$PATH_ENTRY"; fi

echo "[clipdash] Creating GNOME keybinding at $PATH_ENTRY"
gsettings set "$SCHEMA" custom-keybindings "[$NEW_LIST]"
gsettings set "$SCHEMA.custom-keybinding:$PATH_ENTRY" name "$NAME"
gsettings set "$SCHEMA.custom-keybinding:$PATH_ENTRY" command "$CMD"
gsettings set "$SCHEMA.custom-keybinding:$PATH_ENTRY" binding "$BINDING"

echo "[clipdash] Bound $BINDING to '$CMD'"

