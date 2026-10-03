# Maze Game (pygame)

This benchmark asks Spiral-Coder to scaffold a fresh `maze_game_pygame/` repo, keep the gameplay logic in `maze_game_pygame/game.py`, keep `maze_game_pygame/main.py` runnable, and finish only after a headless pygame unit-test run passes.

The generated repo is intentionally not tracked in git. Re-running the command below recreates it under a fresh `.tmp/runtime_eval_<timestamp>/...` directory.

## Reproduce

```bash
cargo run --quiet -- eval --spec .spiral-coder/runtime_eval.json --filter maze-game-pygame-repo --max-cases 1
```

## Last Green Snapshot

- Refreshed: 2026-04-18
- Eval case: `maze-game-pygame-repo`
- Provider: `openai-compatible`
- Model: `gpt-5-mini`
- Base URL: `https://api.openai.com/v1`
- Language: `en`
- Max iterations: `18`
- Duration: `103.0s`
- Tool calls: `6`
- Agent iterations: `9`
- Session messages: `21`
- Estimated transcript tokens: `2840` input, `1895` output, `4735` total
- Final handoff size: about `256` tokens

## Expected Deliverable

- Repo root: `maze_game_pygame/`
- Main logic: `maze_game_pygame/game.py`
- Entrypoint: `maze_game_pygame/main.py`
- Verification command:

```bash
test -d maze_game_pygame/.git && \
test -f maze_game_pygame/README.md && \
test -f maze_game_pygame/game.py && \
test -f maze_game_pygame/main.py && \
test -f maze_game_pygame/test_game.py && \
cd maze_game_pygame && SDL_VIDEODRIVER=dummy python3 -m unittest -q 2>&1
```

## Why This Case Matters

- It exercises non-Rust scaffold behavior without relying on `cargo new`.
- It checks that the runtime carries the repo root, gameplay file, and verification command into the final handoff.
- It proves the closeout path works even when most edits happen through `write_file` plus auto-test.

## Notes

- The snapshot above came from a green runtime-eval report and final handoff, not from a hand-written demo repo.
- Token counts are transcript-side estimates taken from the runtime-eval report. They are useful for relative comparison, not billing.
