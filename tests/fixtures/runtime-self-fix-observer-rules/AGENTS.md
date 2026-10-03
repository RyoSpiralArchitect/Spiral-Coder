# fixture contributor guide

- Keep the fix focused on the observer repo-rule layer.
- When replay-sensitive paths change, update `docs/runtime-architecture.md` and `.spiral-coder/tui_replay.json` in the same change.
- Proof for this fixture means:
  - `cargo test -q observer::repo_rules::tests:: 2>&1 && bash scripts/tui-replay-smoke.sh`
