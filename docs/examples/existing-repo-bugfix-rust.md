# Existing Repo Bugfix (Rust)

This benchmark asks Spiral-Coder to resume work on a tiny existing Rust repo, patch the smallest safe fix in `src/lib.rs`, and finish only after `cargo test 2>&1` passes.

Unlike the maze-game scaffolds, this case is intentionally about editing an existing file with resume memory. The fixture includes both a seeded `session.json`-style resume snapshot and a repo-local `.spiral-coder/progress.json` snapshot so the runtime can carry forward prior verification and dead-end avoidance.

## Reproduce

```bash
cargo run --quiet -- eval --spec .spiral-coder/runtime_eval.json --filter resume-session-bridge-fix --max-cases 1
```

For the fresh non-resume variant:

```bash
cargo run --quiet -- eval --spec .spiral-coder/runtime_eval.json --filter fix-failing-rust-test --max-cases 1
```

## Fixture Shape

- Fixture root: `tests/fixtures/runtime-fix-rust/`
- Resume seed: `tests/fixtures/runtime-fix-rust/session_bridge_resume_seed.json`
- Repo progress seed: `tests/fixtures/runtime-fix-rust/.spiral-coder/progress.json`
- Expected changed file: `src/lib.rs`
- Verification command:

```bash
cargo test 2>&1
```

## Last Green Completion Snapshot

- Refreshed: 2026-04-04
- Eval case: `resume-session-bridge-fix`
- Duration: `35.8s`
- Tool calls: `2`
- Agent iterations: `6`
- Session messages: `20`
- Final handoff: `src/lib.rs` fixed, `cargo test 2>&1` cited

## Latest Progress-Bridge Telemetry Snapshot

- Refreshed: 2026-04-18
- Eval case: `resume-session-bridge-fix`
- Provider: `openai-compatible`
- Model: `gpt-5-mini`
- Base URL: `https://api.openai.com/v1`
- Language: `en`
- Max iterations: `6`
- First-turn telemetry:
  - `progress_state_loaded`
  - `progress_bridge_prompted`
  - `session_bridge_prompted`
- Estimated transcript tokens before provider failure: `485` input, `0` output, `485` total
- Outcome: provider returned `HTTP 429 insufficient_quota` before the first tool call

## Why This Case Matters

- It exercises the existing-file edit lane, not just fresh repo scaffolding.
- It shows the runtime can seed cross-session resume from both transcript memory and repo-local progress memory.
- It makes over-observation more visible: the useful path is `read_file(src/lib.rs) -> patch_file -> cargo test`, not broad rediscovery.

## Notes

- The last fully green completion snapshot predates the new progress-bridge eval assertion, but the latest telemetry run confirms that `progress_state_loaded`, `progress_bridge_prompted`, and `session_bridge_prompted` all fire before model execution.
- Current live-provider reruns are limited by quota. Deterministic Rust tests still cover the progress-state derivation and bridge prompt logic locally.
