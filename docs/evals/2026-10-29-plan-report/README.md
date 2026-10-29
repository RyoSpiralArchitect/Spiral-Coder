# Replanning and factual final reports

[Validation](validation.json) · [source review](review.json) · [r5 results](results-r5.json) ·
[r6 results](results-r6.json) · [r7 results](results-r7.json)

This follow-up addresses the plan refusals and incidental report inaccuracies
recorded in [the preceding examples run](../2026-10-29-diagnostic-actions/README.md).
Coder and Observer remain one coupled task, with Mistral Large 4 in both roles.
The task prompts, original tests, independent assessor and budgets are unchanged.

Across five predeclared adaptive attempts, **four strictly qualified and one
remained incomplete**, using **82 wire requests**. All five candidate workspaces
pass the independent original tests. The final-runtime confirmation uses 16
requests and accepts a truthful structured count report without a report retry.
These are development observations across three runtime versions, not a model
success rate or controlled causal comparison.

| Wave / task | Coder / Observer requests | Independent original tests | Native completion + strict qualification |
| --- | ---: | --- | --- |
| r5 / `event-summary` | 16 / 5 | pass | pass |
| r5 / `env-bool` | 14 / 4 | pass | pass, after two false report refusals |
| r6 / `event-summary` | 11 / 3 | pass | pass |
| r6 / `env-bool` | 9 / 4 | pass | incomplete: Observer unavailable |
| r7 / `env-bool` | 12 / 4 | pass | pass |

The first wave used 39 requests and the second used 27. A passing candidate is
not a completed task: r6 settings has a source repair, docs and fresh exact-check
proof, but no accepted final report. Its incomplete result remains unchanged.

## What changed

- **Pending-step revisions retain their admitted objective.** A revision with
  exactly the same goal and ordered acceptance criteria can reuse its natively
  admitted alignment anchor. The anchor survives pruning and session resume;
  prose alone, a changed goal, malformed metadata or a new human task cannot
  authorize it. The original shape, read-only, scope and completion gates remain.
- **Plan parsing preserves structure.** Multiline line-start ordinals are kept,
  so `(exit 1) recorded` inside one step no longer becomes an extra step. A leading
  `plan>` or standalone `>` can recover only a missing opener when the closing
  tag and all unique authored fields are present. Raw text is retained. Missing
  fields, quotations, duplicate closers and real oversized lists still fail.
- **Both roles see observed test counts.** Native exec results retain optional
  unittest/libtest counts before output compaction. The final-report view binds
  them to the exact before/after call IDs, commands and statuses. A bounded
  authored-report check asks for correction or omission of unsupported counts
  without invalidating successful checks. Example-output guidance asks for the
  input condition rather than presenting an example as a constant result.

Unittest failure events are not distinct failed test methods: the settings
baseline runs 10 methods but records 14 failure events, including subtests. The
runtime does not subtract those events to invent a passed-method count. Unknown,
partial, oversized or multi-suite output remains unknown; counts never replace
native verification authority. Local metadata is removed from provider wire
messages and projected through the shared task obligations instead.

## What the live loops exposed

r5 JSONL spent two requests on complete plans whose leading opening delimiter was
truncated. The second original refusal survives in the regression fixture; the
first was already content-compacted before fixture export. The missing-`<`
variant in that test is explicitly constructed. r6 adds the narrowly bounded
recovery and retains a real admitted `plan>` action. Its oversized acceptance
list still fails the four-criterion ceiling. Request differences across these
adaptive single attempts do not establish a causal efficiency improvement.

r5 settings exposed a bug in the new report checker itself. These two correct
reports were refused:

```
Before: tests_run 10, failure_events 14, errors 0, skipped 0.
Before: errors=0, failure_events=14, runner=unittest, skipped=0, tests_run=10.
```

Removing punctuation and underscores had attached each number to the *next*
field, reading the first line as “10 failures” and “14 errors.” The accepted r5
report omitted numeric counts after two retries. Both original rejected reports
are retained unchanged as regression fixtures. The final parser preserves
field/value boundaries; truthful reports pass, and changing 14 to 9 still fails.
The correction was made after r6 had already been frozen and started, so r6 keeps
the earlier checker. r7 separately confirms the final runtime on this task.

The r5 and r6 JSONL reports match native counts: 10 tests, 8 failure events and
5 errors before repair; 10 tests and zero failure events/errors after repair.
Both now condition the all-zero example on empty input. These are manual checks
of the retained prose in addition to the unchanged strict qualification predicate.

r7 settings uses the final count-field parser. Its first authored `done` report
contains `tests_run=10, failure_events=14, errors=0, skipped=0` before repair
and `tests_run=10, failure_events=0, errors=0, skipped=0` afterward. These match
the native receipts and explicitly distinguish subtest events from test methods.
The report passes without a Final handoff refusal. Source and documentation were
also inspected; two earlier plan/evidence refusals remain in the record.

## Frozen execution and artifacts

| Wave | Source commit | Declaration and controls | Supplementary review |
| --- | --- | --- | --- |
| r5 | `9077edf69b4ee8b643cc3081eca08da6e7e526a4` | [plan](plan-r5.json), [executor](executor-r5.json), [preflight](preflight-r5.json) | [review](review-r5.json) |
| r6 | `15da20219936bc6cef49f7a50b9bf3bc949f4337` | [plan](plan-r6.json), [executor](executor-r6.json), [preflight](preflight-r6.json) | [review](review-r6.json) |
| r7 | `d2fe7876e358589c4836ba70d14c17e576127b8b` | [plan](plan-r7.json), [executor](executor-r7.json), [preflight](preflight-r7.json) | [review](review-r7.json) |

Every wave was pinned before its provider calls, with a separate frozen driver,
binary, prompts, workspace, original checks and reference controls outside the
candidate scope. Each attempt permits 24 task requests, 6 reserved for Observer,
4 transport repairs, an 8-request Coder slice ceiling and 600 seconds. The five
predeclared attempts permit at most 140 wire requests; no episode is retried in
place. Earlier assessments and incomplete attempts are retained.

Candidate diffs are retained for [r5 JSONL](r5-event-summary.patch),
[r5 settings](r5-env-bool.patch), [r6 JSONL](r6-event-summary.patch),
[r6 settings](r6-env-bool.patch) and [r7 settings](r7-env-bool.patch). Each patch
is reapplied to its original fixture and compared byte for byte with its retained
candidate. No candidate patch is adopted into production source. Unified-diff
context spaces are kept intact. Raw manifests and credential-absence scan counts
are recorded in each wave's review.

## Remaining limits

The r6 settings Observer proposed `done` without acceptance evidence covering
each completed criterion exactly once. The existing gate correctly refused it.
Its one permitted action-contract repair then exhausted 8192 completion tokens
(8191 reasoning tokens) and emitted no response content. The task terminates
`observer_unavailable`; extra requests or success were not fabricated. Tighter
Observer completion shaping and bounded handling of output-limited repair are
follow-up work, not outcomes established by the count-parser correction.

A revision that paraphrases its goal still needs to satisfy normal new-plan
alignment. The narrow reuse rule does not equate different objectives. Missing
mutation evidence also continues to cause refusals.

The count checker covers explicit English before/baseline and after/final
integer claims and native field/value notation. It is not a general fact checker
for arbitrary prose, languages or runner formats. Code-behavior explanations
remain subject to review; green tests do not prove universal correctness.

## Validation

The final required local checks pass: **990 Rust tests and one integration test**
(one ignored), **91 Python tests**, **85 Web/npm tests**, **13/13 TUI replays**,
**14/14 repo-map cases**, and server/UI smoke. Recorded red/green regressions cover
the two real plan failures, unsupported counts, native truncated-plan handling,
and the false structured-count refusals. Live outcomes and GitHub CI are separate
receipts. CI and merge status belong to the PR on GitHub.
