#!/usr/bin/env bash
set -euo pipefail

if ! grep -q 'src/tui/agent/merge_approval.rs' docs/state-schema.md; then
  echo "missing docs follow-up for src/tui/agent/merge_approval.rs" >&2
  exit 1
fi

if ! grep -q 'src/tui/agent/merge_approval.rs' .spiral-coder/runtime_eval.json; then
  echo "missing runtime eval follow-up for src/tui/agent/merge_approval.rs" >&2
  exit 1
fi

echo "pr-ready merge approval smoke ok"
