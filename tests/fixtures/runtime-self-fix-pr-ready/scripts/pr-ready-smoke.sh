#!/usr/bin/env bash
set -euo pipefail

if ! grep -q 'src/tui/agent/session_bridge.rs' docs/state-schema.md; then
  echo "missing docs follow-up for src/tui/agent/session_bridge.rs" >&2
  exit 1
fi

if ! grep -q 'src/tui/agent/session_bridge.rs' .spiral-coder/runtime_eval.json; then
  echo "missing runtime eval follow-up for src/tui/agent/session_bridge.rs" >&2
  exit 1
fi

echo "pr-ready smoke ok"
