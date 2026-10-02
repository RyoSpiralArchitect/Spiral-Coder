# fixture contributor guide

- Keep the fix focused on the TUI agent follow-up requirement layer.
- When PR-ready runtime paths change, update `docs/state-schema.md` and `.spiral-coder/runtime_eval.json` in the same change.
- Proof for this fixture means:
  - `cargo test -q tui::agent::followup_requirements::tests:: 2>&1 && bash scripts/pr-ready-smoke.sh`
