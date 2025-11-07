#!/usr/bin/env bash
set -euo pipefail

# Usage: package_release.sh <flavor> [target]
#   flavor: linux-x86_64-gnu | linux-x86_64-musl
#   target: optional Rust target triple for picking binaries under target/<triple>/release

ROOT="$(cd "$(dirname "$0")"/.. && pwd)"
cd "$ROOT"

FLAVOR=${1:?missing flavor}
TRIPLE=${2:-}

OUT_DIR="$ROOT/dist"
mkdir -p "$OUT_DIR"

REL_DIR="target/release"
if [[ -n "$TRIPLE" ]]; then
  REL_DIR="target/$TRIPLE/release"
fi

NAME="clipdash-${FLAVOR}"
TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT

mkdir -p "$TMPDIR/bin" "$TMPDIR/packaging" "$TMPDIR/scripts"

# Required binaries
cp -v "$REL_DIR/clipdash" "$TMPDIR/bin/" || true
cp -v "$REL_DIR/clipdash-daemon" "$TMPDIR/bin/" || true

# UI binary only for gnu flavor (gtk)
if [[ -f "$REL_DIR/clipdash-ui" ]]; then
  cp -v "$REL_DIR/clipdash-ui" "$TMPDIR/bin/"
fi

# Desktop entries and systemd user unit
cp -v packaging/clipdash-menu.desktop "$TMPDIR/packaging/" || true
cp -v packaging/clipdash-menu-gtk.desktop "$TMPDIR/packaging/" || true
cp -v packaging/clipdash-daemon.desktop "$TMPDIR/packaging/" || true
cp -v packaging/clipdashd.service "$TMPDIR/packaging/" || true

# Installer scripts
cp -v scripts/install_dev.sh "$TMPDIR/scripts/install.sh"
cp -v scripts/gnome_bind_super_v.sh "$TMPDIR/scripts/" || true
cp -v scripts/gnome_switch_to_ui.sh "$TMPDIR/scripts/" || true

tar -C "$TMPDIR" -czf "$OUT_DIR/${NAME}.tar.gz" .
echo "Created: $OUT_DIR/${NAME}.tar.gz"

