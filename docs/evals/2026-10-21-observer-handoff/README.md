# Observer diagnosis and authored before/after follow-up

This is the same small `greet.py` regression as
[the preceding baseline review](../2026-10-21-review-baseline/README.md).
The task, original tests, provider models, 18-request shared cap, temperature,
token limit and request/wall timeouts are unchanged. Each round starts a new
workspace and retains its original predeclaration and raw-run hashes.
This is not a rerun of the larger repository/JSON panel and is not a provider
ranking or general reliability estimate.

## Round r2: diagnosis alignment and grounded report fields

Source: `65a12e1d55f94d2f1ac15d3e1c36162828074b65`. See
[predeclaration](r2/predeclared.json), [receipts](r2/receipts.json) and
[raw manifest](r2/raw-manifest.json).

| Same model for Coder and Observer | Requests | Assists | Runtime completed | Original tests pass independently | Authored before/after present |
| --- | ---: | ---: | --- | --- | --- |
| mistral-small-2603 | 12 | 0 | yes | yes | yes |
| gpt-4.1-mini | 18 | 1 | no | yes | yes, but rejected |
| gemini-3.5-flash-lite | 17 | 1 | yes | yes | yes |

All three recorded the failing exact baseline before a successful native edit,
then a fresh successful exact check. The supplied tests and configuration stayed
byte-identical. The independent oracle copied candidate `greet.py` and the frozen
original tests into a fresh directory, excluding candidate bytecode and tests.

OpenAI exposed a gate defect: both Coder and Observer put the correct canonical
receipt fields in a paragraph. The line-only matcher rejected them repeatedly
until the shared cap ran out. The follow-up accepts these same fields as lines
or sentences, preserving exact labels, status and command matching. Regression
coverage uses that retained paragraph and still rejects missing, stale,
contradictory-status and unrendered fields.

These live assists concerned final reports, so the live sample does not establish
diagnosis-repair effectiveness. Deterministic native/TUI/Web cases cover wrong
diagnosis targets, the single repair request, correct reads, uncertain advice,
forged display metadata and root-task reset.

The collector's strict `fully_qualified` field is false for all r2 runs: the
literal all-files scope also includes runtime-written `.spiral-coder/tui-work.json`
and Python bytecode in this fixture without ignore rules. Receipts retain those
paths rather than silently filtering them. Source repair, frozen-test integrity,
runtime completion and authored reporting are reported separately above.
