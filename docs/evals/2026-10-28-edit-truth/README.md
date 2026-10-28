# Edit truth and exact check selections

Follow-up to the [budget/rhythm episodes](../2026-10-27-budget-rhythm/README.md).
Native file tools now report identical content as `unchanged`; those operations
cannot clear failures, advance repair, trigger automatic checks or count as edits.
Coder and Observer share this outcome. Declared checks can be selected by ID and
are resolved to the exact command at the task root, through normal approval and
proof gates. Signed-provider calls retain their full-command path.

The [predeclared plan](plan.json) freezes two diagnostic episodes, one for each
Luna/Large direction, with at most 72 wire requests in total. There is no concurrent
old-runtime arm and no causal improvement claim. The inherited independent oracle,
visible reproducer, reference patch, original 864-test floor, scope restrictions,
native completion and authored-report requirements remain unchanged. No candidate
adoption is authorized. Provider outcomes will be recorded after the runs.

## Deterministic validation

The [validation receipt](validation.json) records 970 Rust tests (one ignored),
one integration test, 79 Python tests, 85 Web/npm tests, 12 TUI replays and 14
repo-map cases. The local UI/server smoke check also passed. Mock-provider tests
exercise the real Coder/TUI loop for unchanged writes, patches and diffs, selected
checks, root-directory normalization, approval and refusal. The new TUI replay
checks that an Observer check-ID suggestion reaches the Coder hint.

Fresh fork review found four regressions, each reproduced before repair:

- A newer direct or aliased check failure did not supersede a selected success.
- Python evaluators counted unchanged successful operations as edits.
- Unchanged operations cleared repeated-observation pressure.
- Observer receipts lost resolved selections when approval was refused or pending.

The broader repo-map check also caught a search regression: several partial
matches in the new tests outranked the complete production identifier. Spaced
queries now receive the existing whole-symbol match weight for snake-case names;
a focused ranking regression and all 14 existing cases pass without changing the
evaluation criteria. An additional strict Clippy attempt reported style warnings;
it is not a required repository gate and is not claimed clean.
