#!/usr/bin/env bash
set -euo pipefail

grep -q 'src/merge_gate.rs' docs/state-schema.md
grep -q 'src/merge_gate.rs' .spiral-coder/runtime_eval.json
cargo test -q merge_gate::tests:: 2>&1
printf 'merge gate rollback smoke ok\n'
