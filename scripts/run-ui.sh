#!/usr/bin/env bash
set -euo pipefail

HOST="127.0.0.1"
PORT="18080"

usage() {
  cat <<'USAGE'
Usage: bash ./scripts/run-ui.sh [--host <host>] [--port <port>]

Builds and runs the Spiral-Coder web UI server from an isolated CARGO_TARGET_DIR.
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --host)
      HOST="${2:-}"
      shift 2
      ;;
    --port)
      PORT="${2:-}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown arg: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${REPO_ROOT}"

if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo not found. Install Rust (rustup), then retry." >&2
  exit 1
fi

export CARGO_TARGET_DIR="${REPO_ROOT}/.tmp/cargo-target-ui"
mkdir -p "${CARGO_TARGET_DIR}"

echo "[run-ui] cargo build"
echo "[run-ui] CARGO_TARGET_DIR=${CARGO_TARGET_DIR}"
cargo build

BIN="${CARGO_TARGET_DIR}/debug/spiral-coder"
if [[ ! -f "${BIN}" ]]; then
  echo "spiral-coder binary not found at: ${BIN}" >&2
  echo "Try: cargo build" >&2
  exit 1
fi

echo "Spiral-Coder UI: http://${HOST}:${PORT}/"
exec "${BIN}" serve --host "${HOST}" --port "${PORT}"
