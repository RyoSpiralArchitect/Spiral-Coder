# Runtime self-fix merge-gate rollback fixture

- Keep the fix focused on `src/merge_gate.rs`.
- When merge-gate rollback board semantics change, update `docs/state-schema.md` and `.spiral-coder/runtime_eval.json` in the same change.
- Required proof:
  - `cargo test -q merge_gate::tests:: 2>&1 && bash scripts/merge-gate-smoke.sh`
