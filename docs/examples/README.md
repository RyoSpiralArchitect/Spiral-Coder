# Benchmark Examples

Start with the [small practical examples](../../benchmarks/examples-v1/README.md):
repair a boolean setting parser or a JSONL event summary with Coder + Observer.
Each uses one short source file, documentation and ten original tests, with no
third-party dependencies. The quick start uses Mistral Large in both roles;
a separate matrix supports Luna / Large in both directions. The frozen runner
records behavior, native checks and completion separately.
The [recorded Mistral pair run](../evals/2026-10-29-diagnostic-actions/README.md)
completes both tasks in 14 requests each, with candidate diffs and independent
checks available for inspection.

The [later plan/report loops](../evals/2026-10-29-plan-report/README.md) retain
newer runtime results, count-checker corrections and an incomplete Observer
handoff. Use the individual frozen receipts rather than treating every passing
artifact as a completed task.

The historical pages below document repeatable Spiral-Coder benchmark runs
without checking the generated repos into git.

- Generated repos stay under local `.tmp/runtime_eval_<timestamp>/...` directories.
- The docs keep the prompt shape, provider/model, CLI command, and benchmark snapshot so the same case can be rerun later.
- Token numbers are **approximate transcript estimates**, not provider billing totals.

Current examples:

- [Small practical repairs with Mistral Coder + Observer — PR #42 archive](coupled-practical-repairs-mistral.md)

- [Existing Repo Bugfix (Rust)](existing-repo-bugfix-rust.md)
- [Maze Game (Rust)](maze-game-rust.md)
- [Maze Game (pygame)](maze-game-pygame.md)
