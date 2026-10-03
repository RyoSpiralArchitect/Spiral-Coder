# Maze Game (Rust)

This benchmark asks Spiral-Coder to scaffold a fresh `maze_game/` Rust repo, keep the gameplay logic in `maze_game/src/lib.rs`, keep `maze_game/src/main.rs` runnable, and finish only after a real `cargo test` verification passes.

The generated repo is intentionally not tracked in git. Re-running the command below recreates it under a fresh `.tmp/runtime_eval_<timestamp>/...` directory.

## Reproduce

```bash
cargo run --quiet -- eval --spec .spiral-coder/runtime_eval.json --filter maze-game-rust-repo --max-cases 1
```

## Last Green Snapshot

- Refreshed: 2026-04-18
- Eval case: `maze-game-rust-repo`
- Provider: `openai-compatible`
- Model: `gpt-5-mini`
- Base URL: `https://api.openai.com/v1`
- Language: `en`
- Max iterations: `16`
- Duration: `103.6s`
- Tool calls: `5`
- Agent iterations: `8`
- Session messages: `19`
- Estimated transcript tokens: `2746` input, `1282` output, `4028` total
- Final handoff size: about `213` tokens

## Expected Deliverable

- Repo root: `maze_game/`
- Main logic: `maze_game/src/lib.rs`
- Entrypoint: `maze_game/src/main.rs`
- Verification command:

```bash
test -d maze_game/.git && \
test -f maze_game/README.md && \
test -f maze_game/Cargo.toml && \
test -f maze_game/src/lib.rs && \
test -f maze_game/src/main.rs && \
cd maze_game && cargo test 2>&1
```

## Why This Case Matters

- It exercises scaffold-lane repo creation without nested-git drift.
- It checks that closeout names the repo root and primary gameplay file, not just the last edited file.
- It requires behavioral verification, not only artifact existence.

## Notes

- The snapshot above came from a green runtime-eval report and final handoff, not from a hand-written demo repo.
- Token counts are transcript-side estimates taken from the runtime-eval report. They are useful for relative comparison, not billing.
