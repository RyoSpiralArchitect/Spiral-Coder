# Historical benchmark notes

Moved from the repository overview at commit `394a339` on 2026-10-26.
These are historical observations and proposed directions, not a current success
rate or a statement that each case passes today's evaluator. Local `.tmp/` paths
are retained as original run identifiers, not downloadable artifacts.

[Research home](README.md) · [Current evaluation contract](../evaluation.md)

## Dogfood Benchmarks

Spiral-Coder now keeps lightweight benchmark writeups for generated milestone repos without checking those repos into git.

- [Benchmark Examples](../examples/README.md)
- [Existing Repo Bugfix (Rust)](../examples/existing-repo-bugfix-rust.md)
- [Maze Game (Rust)](../examples/maze-game-rust.md)
- [Maze Game (pygame)](../examples/maze-game-pygame.md)

Each page records the eval prompt shape, provider/model, rerun command, runtime snapshot, and approximate transcript token usage from a green `runtime_eval` report.

---

## Historical Benchmark Milestones

The following milestones describe earlier runs under the previous evaluator. They retain their historical results and have not all been rerun under the stricter completion and proof checks. See the [evaluation contract](../evaluation.md) and [restart validation](../evals/2026-10-03-restart/README.md) for current evidence, including unsuccessful attempts.

- `maze-game-rust-repo` proves the scaffold lane can create a fresh Rust repo, keep gameplay logic in `src/lib.rs`, keep `src/main.rs` runnable, and close with a real `cargo test`.
- `maze-game-pygame-repo` proves the same closeout path works for a non-Rust repo, including a headless `pygame` verification command.
- `resume-session-bridge-fix` shows the runtime can resume an existing Rust bugfix with both seeded session memory and repo-local `.spiral-coder/progress.json` memory, then push the agent back toward the smallest safe patch.
- The medium-plus self-dogfood stretch case `self-fix-observer-repo-rules-review-panel` now passes end-to-end: the runtime patches `src/observer/repo_rules.rs`, carries the required `docs/runtime-architecture.md` and `.spiral-coder/tui_replay.json` follow-ups, and closes with the exact passing verification command in the final handoff.
- The PR-ready self-dogfood case `self-fix-pr-ready-runtime-followup` now passes end-to-end too: the runtime patches `src/tui/agent/followup_requirements.rs`, carries the required `docs/state-schema.md` and `.spiral-coder/runtime_eval.json` follow-ups, and repairs the final handoff when a verified artifact path would otherwise be omitted. Latest green run: `.tmp/runtime_eval_1777229175/report.json` (`tools=7`, `messages=22`, approx `3.85k` transcript tokens) with merge readiness captured in `.tmp/runtime_eval_1777229175/merge_gate.json`.
- The merge-approval self-dogfood case `self-fix-pr-ready-merge-approval` now exercises a fuller PR-ready change set: code fix, docs follow-up, runtime-eval follow-up, exact configured smoke command, and a final handoff that includes the required status label `PR-ready merge approved`. Latest green run: `.tmp/runtime_eval_1777491446/report.json` (`tools=9`, `messages=28`, approx `4.40k` transcript tokens) with merge readiness captured in `.tmp/runtime_eval_1777491446/merge_gate.json`.
- The rollback-guard stretch case `self-fix-merge-gate-rollback-guard` raises the self-dogfood difficulty again: after a source read, the runtime can synthesize the smallest safe mutation for a stalled no-tool turn, carry docs/runtime-eval follow-ups, and close with the required `rollback guard enforced` handoff. Latest green run: `.tmp/runtime_eval_1777493520/report.json` (`tools=6`, `messages=20`, approx `3.83k` transcript tokens) with merge readiness captured in `.tmp/runtime_eval_1777493520/merge_gate.json`.
- Runtime eval closeout now writes a generated `merge_gate.json` next to `report.json`; `spiral-coder merge-gate`, the TUI Merge tab, and the Web GUI merge-gate panel can inspect readiness, approve passing cases, hold cases, and copy rollback previews without executing destructive rollback.
- The benchmark reports now carry provider/model metadata plus approximate transcript token telemetry, which makes it easier to compare runs without pretending those numbers are billing-accurate.

These records describe individual observed runs; they do not establish a general success rate or long-running reliability.

---

## Ideas recorded in the earlier overview

The next stretch is less about making prettier demos and more about making the runtime trustworthy over longer work.

- **Long-running durability**: session resume is much better now, and repo-local progress snapshots are in place, but true multi-hour work still wants stronger context compaction, drift-triggered replanning, and more checkpoint-aware recovery.
- **Broader benchmark mix**: fresh repo scaffolds plus one existing-repo bugfix are a good start; the next useful cases are small web apps, medium refactors, and docs/config tasks where the right verification floor is easier to get wrong.
- **Merge-ready self-dogfood loops**: PR-ready fixture work now has branch/checkpoint safety, a generated merge gate, shared TUI/GUI human review state, and a green merge-approval handoff benchmark. The next useful step is replay/eval-backed promotion from approved gate state into an actual PR-ready merge workflow.
- **Stronger verification pressure**: passing one test command is good, but adversarial evaluator loops, rollback/checkpoint restore, and quality-oriented checks would raise the floor a lot.
- **Patch recovery under pressure**: malformed or truncated patches now stop overpowering the evaluator loop, but the next step is a stronger exact-snippet repair path so retries recover faster instead of relying on another model turn.
- **Better telemetry**: transcript token estimates are useful for comparison, but provider-native usage accounting and benchmark trend history would make optimization much more concrete.
- **Measured harness evolution**: the repo now has overlay, promotion candidate, and human-gated apply flow; the next step is learning which promoted rules actually improve future runs and which ones just add prompt weight.

---

