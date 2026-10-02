#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "${SCRIPT_DIR}/.."

# Model-free checks. Cargo may download locked dependencies on a fresh checkout.
cargo fmt --all -- --check
cargo test --locked --all-targets
cargo run --locked -- tui-replay --spec .spiral-coder/tui_replay.json
python3 -S -m unittest discover -s tests -p 'test_*.py'
node --test tests/web/*.test.mjs
for file in web/*.js web/core/*.js web/observer/*.js scripts/*.mjs; do
  node --check "$file"
done
for file in scripts/*.sh; do
  bash -n "$file"
done
python3 -S scripts/repo_map.py build --root .
python3 -S scripts/repo_map.py eval --root .
bash scripts/e2e-smoke.sh
