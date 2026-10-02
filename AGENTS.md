# spiral-coder contributor guide

This repo is converging on enterprise-style coding-agent quality gates.
Keep the unique Observer/Coder/Chat design, but raise the floor on structure,
state, and repeatable verification.

## Core rules

- Prefer adding focused modules over extending high-churn orchestration files.
- Treat replay/eval artifacts as first-class verification, not optional extras.
- When a change affects a typed contract, update the docs in `docs/` in the same change.
- Keep state typed and auditable. Prefer explicit structs over "smart" ad hoc memory.

## High-touch files

These files already attract unrelated changes and should grow more slowly:

- `src/tui/agent.rs`
- `src/tui/events.rs`
- `src/tui/app.rs`
- `src/streaming.rs`

Rules for these files:

- Do not add new functionality to these files if the logic can live in a new module.
- Target Rust modules under roughly 500 LoC where practical.
- If a file is above roughly 800 LoC, prefer extracting the new functionality.
- When extracting code, move the related tests or type docs toward the new owner.

See `docs/tui-agent-split-plan.md` for the current extraction plan.

## State and contracts

Spiral-Coder now has multiple typed state surfaces. Before adding new fields, decide
which layer owns them.

- Runtime/provider config: `src/config.rs`
- Persistent TUI prefs: `src/tui/prefs.rs`
- Session persistence and observation cache: `src/agent_session.rs`
- In-memory orchestration state: `src/tui/app.rs`
- Intent normalization / anchor state: `src/tui/intent.rs`

Do not add new persistent state until it has a clear owner in
`docs/state-schema.md`.

## Replay and eval policy

Changes that affect control flow should update the matching replay/eval path.

### Required checks by change type

- TUI visible or TUI control-flow changes:
  - `cargo test -q ... tui::events::tests::`
  - `cargo run -- ... tui-replay --spec .spiral-coder/tui_replay.json`
- Coder loop / rescue / governor / done-gate changes:
  - `cargo test -q ... tui::agent::tests::`
  - `cargo run -- ... eval --spec .spiral-coder/runtime_eval.json` for at least the affected case(s) when practical
- Repo-map runtime fallback changes:
  - `python3 scripts/repo_map.py eval --root .`
  - affected runtime eval or TUI replay case
- Observer suggestion changes:
  - `cargo test -q ... tui::suggestion::tests::`
  - `cargo test -q ... tui::events::tests::`
  - `cargo run -- ... tui-replay --spec .spiral-coder/tui_replay.json`

If a provider is flaky, keep deterministic replay coverage in place and note
the live-provider limitation in the final summary.

## Rust workflow

- Run `cargo fmt --manifest-path Cargo.toml` after Rust changes.
- Run targeted tests for the modules you changed.
- Prefer deep object comparisons in tests when practical.
- Avoid adding opaque positional bool/`Option` parameters when an enum or helper
  would make the callsite clearer.

## Documentation updates

Update `docs/` when you change:

- state ownership or persistence
- replay/eval policy
- runtime architecture
- typed contracts such as intent, suggestion, done/evidence, or replay specs

