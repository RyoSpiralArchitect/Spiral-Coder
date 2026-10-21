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

## Round r3: accept the same receipts in a paragraph

Source: `45aec130dadd6bd99b10c9931b796d833716842f`. See
[predeclaration](r3/predeclared.json), [receipts](r3/receipts.json) and
[raw manifest](r3/raw-manifest.json). No input or cap changed and no run was retried
in place. The two new rounds consumed 92 requests in total (47 + 45).

| Same model for Coder and Observer | Requests | Assists | Runtime completed | Original tests pass independently | Remaining failure |
| --- | ---: | ---: | --- | --- | --- |
| mistral-small-2603 | 18 | 1 | no | yes | missing acceptance-evidence row |
| gpt-4.1-mini | 9 | 0 | yes | yes | none in the recorded task/report checks |
| gemini-3.5-flash-lite | 18 | 1 | no | no | existing-file write refused after continuation |

The retained OpenAI paragraph now passes, with the changed filename and actual
before/after statuses in the authored report. No report-format retry was needed.
Mistral repaired and verified the source but repeatedly omitted a citation for
its third acceptance criterion. Its Observer supplied that row with an unexecuted
`git diff --name-only` command; the run did not reach a valid completed handoff.
This remains a distinct acceptance-mapping problem, not a reason to fabricate
citations or weaken the evidence gate.

Gemini's Observer proposed the required diagnostic read of `test_greet.py` and
the continuation executed it. The next `write_file(greet.py)` was refused because
the current source contents needed to be read again; the shared budget then ran
out. That is a remaining continuation/read-freshness problem. Original tests and
configuration remained unchanged in all three runs.

Strict all-file qualification stays false, including incidental runtime/cache
files for the two repaired candidates as described above. One r3 completion and
two passing independent oracles are not evidence of stable three-provider success.
