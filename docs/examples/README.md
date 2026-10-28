# Benchmark Examples

Start with the [small practical examples](../../benchmarks/examples-v1/README.md):
repair a boolean setting parser or a JSONL event summary with Coder + Observer.
Each uses one short source file, documentation and ten original tests, with no
third-party dependencies. The frozen runner supports Luna / Mistral Large in
both directions and records behavior, native checks and completion separately.

The historical pages below document repeatable Spiral-Coder benchmark runs
without checking the generated repos into git.

- Generated repos stay under local `.tmp/runtime_eval_<timestamp>/...` directories.
- The docs keep the prompt shape, provider/model, CLI command, and benchmark snapshot so the same case can be rerun later.
- Token numbers are **approximate transcript estimates**, not provider billing totals.

Current examples:

- [Existing Repo Bugfix (Rust)](existing-repo-bugfix-rust.md)
- [Maze Game (Rust)](maze-game-rust.md)
- [Maze Game (pygame)](maze-game-pygame.md)
