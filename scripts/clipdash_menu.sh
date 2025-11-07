#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.." >/dev/null 2>&1 || true

if command -v cargo >/dev/null 2>&1; then
  cargo run -q -p clipdash-cli -- menu
else
  clipdash menu
fi

