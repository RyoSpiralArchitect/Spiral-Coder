#!/usr/bin/env bash
set -euo pipefail

if ! grep -q 'src/tui/review_panel.rs' docs/runtime-architecture.md; then
  echo "missing docs follow-up for src/tui/review_panel.rs" >&2
  exit 1
fi

if ! grep -q 'src/tui/review_panel.rs' .spiral-coder/tui_replay.json; then
  echo "missing tui replay follow-up for src/tui/review_panel.rs" >&2
  exit 1
fi

echo "tui-replay smoke ok"
