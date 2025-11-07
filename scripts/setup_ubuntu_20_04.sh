#!/usr/bin/env bash
set -euo pipefail

echo "[clipdash] Detected: $(lsb_release -ds || echo Ubuntu 20.04)"

echo "[clipdash] Updating apt index..."
sudo apt update

echo "[clipdash] Installing base toolchain..."
sudo apt install -y \
  build-essential \
  pkg-config \
  git \
  curl \
  ca-certificates \
  libsqlite3-dev \
  libx11-dev \
  libxfixes-dev

echo "[clipdash] GTK4 is optional and not required for today's skeleton.\nGTK4 packages may not be available on Ubuntu 20.04 default repos.\nSkip for now; we'll add when the UI is implemented."

echo "[clipdash] Installing clipboard helpers (best-effort): wl-clipboard and xclip"
sudo apt install -y wl-clipboard xclip || true

echo "[clipdash] Installing GTK dialog (zenity)"
sudo apt install -y zenity || true

if ! command -v cargo >/dev/null 2>&1; then
  echo "[clipdash] Installing Rust via rustup (minimal profile)..."
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
  # shellcheck disable=SC1090
  source "$HOME/.cargo/env"
else
  echo "[clipdash] Cargo found: $(cargo --version)"
fi

echo "[clipdash] Adding rust components (clippy, rustfmt)..."
rustup component add clippy rustfmt || true

echo "[clipdash] Done. Try: cd clipdash && cargo build"
