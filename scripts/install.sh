#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${REPO_ROOT}"

if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo not found. Install Rust (rustup), then retry." >&2
  exit 1
fi

echo "[install] cargo install --path . --force"
cargo install --path . --force

cat <<'NOTE'

Installed: spiral-coder

If `spiral-coder` is not found, ensure `~/.cargo/bin` is in your PATH, e.g.:
  echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.zshrc
  source ~/.zshrc

NOTE
