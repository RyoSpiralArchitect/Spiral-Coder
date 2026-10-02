# Fixture rules

- Keep fixes minimal and local.
- When PR-ready merge approval paths change, update `docs/state-schema.md` and `.spiral-coder/runtime_eval.json` in the same change.
- Required proof:
  - `cargo test -q tui::agent::merge_approval::tests:: 2>&1 && bash scripts/pr-ready-smoke.sh`
