# Practical examples: Mistral Coder + Observer results

The [examples suite](../../../benchmarks/examples-v1/README.md) adds two authored
practice tasks: boolean settings and JSONL summaries. Each changes one short
source file and one documentation file, with ten visible original tests and no
external dependencies. The difficult coding/recovery suites and their gates are
unchanged. This is a new task tier, not evidence of a runtime performance gain.

## First wave: Luna / Large 4

[Plan](plan.json), [executor pin](executor.json), [preflight](preflight.json),
[results](results.json). The baseline and reference controls passed before calls.

| Coder → Observer | Task | Wire requests | Outcome |
| --- | --- | ---: | --- |
| Luna → Large 4 | env-bool | 3 | Coder HTTP 429 retries exhausted its slice before any action |
| Luna → Large 4 | event-summary | 3 | Same, before any action |
| Large 4 → Luna | env-bool | 8 | Three Coder reads, then five Observer HTTP 429 attempts |
| Large 4 → Luna | event-summary | none started | Withheld after an explicit account-exhaustion response |

The third episode retained `error.type=insufficient_quota` and
`error.code=credit_balance_exhausted`. Earlier Coder errors lost the response body
when the request allocator refused a further retry; their native ledgers retain
HTTP 429, but the exact body cannot be reconstructed. No episode edited a file,
executed a baseline check or completed. There are **0 qualified of 3 attempted**,
with **1 declared but not run**. This does not measure whether the pair can solve
the tasks. The native unrun request count remains unknown in the assessor; no
request was launched for that episode by this driver.

All 92 entries in the raw manifest were verified against hashes, sizes and modes;
the selected credentials were absent from retained text artifacts. Candidate
patches are empty. The assessor is the frozen driver, not the later runtime.

## Repairs prompted by that wave

Three actual coupled-loop regressions fail before the fix and pass afterward:
Coder quota, Observer quota, and temporary throttling that runs out of Coder
allowance. Explicit structured account exhaustion now stops after one request;
temporary throttling keeps bounded retries, and budget denial retains the
previous HTTP cause. Neither case receives transport credit or task completion.
The same quota policy covers the completions fallback; native Anthropic is
unchanged. These fixes were not used in the first wave.

## Follow-up: Mistral Large 4 in both roles

After the quota result, Ryō selected Mistral × Mistral. The follow-up uses a
separate matrix and fresh output directory, two tasks once each, at most 56 wire
requests. It does not retry or replace the first wave's rows. The role pair and
runtime both change, so this cannot isolate a causal harness effect.

[Plan](mistral/plan.json), [executor](mistral/executor.json),
[results](mistral/results.json).

| Task, prompt revision 1 | Coder / Observer requests | Result |
| --- | ---: | --- |
| env-bool | 12 / 4 | Native completion, fresh exact checks, independent tests and strict file scope pass; report labels fail |
| event-summary | 18 / 6 | Source repaired; documentation unchanged; truthful `blocked` terminal, no final exact check or accepted completion |

The first task's final summary used `Before (baseline, before any edit): FAILED`
and `After (all edits complete): PASSED`. Its actual repair is valid, but the
frozen canonical report predicate rejects those labels: **0/2 strictly qualified**,
**1/2 independently passing artifacts**. The runtime did not activate its native
before/after report check because the final-answer clause omitted a test/check
term. A regression with both real prompt files fails before the wording fix and
passes afterward, exercising the native validator rather than a text substring.

The second task spends two Plan Gate and two Recovery Gate refusals, repairs a
non-string-level regression, then runs out of Coder allowance before its docs
edit. It explicitly retains pending checks and remaining work. Its automatic
test shows the documentation failure; that does not become a fresh exact native
exec. Only `summary.py` changes, and the original tests correctly reject the
incomplete deliverable. All 79 final-manifest entries were verified.

## One adaptive report-contract follow-up

[Plan](report-followup/plan.json), [executor](report-followup/executor.json),
[results](report-followup/results.json),
[candidate diff](report-followup/large-large-env-bool.patch).

Prompt revision 2 explicitly asks for **before/after test outcomes** and places
recorded outcome rows in `done.summary`. The existing native guard now supplies
the exact recorded rows and requires the model to author them. Tests, reference
behavior, file scope, budgets and the frozen assessor are unchanged. The frozen
assessor still rejects parenthetical label variants.

The one `env-bool` attempt uses **21 task requests: 15 Coder + 6 Observer**, no
transport credits, and **strictly qualifies (1/1)**. Both requested files change,
the failing baseline precedes edits, the separate exact check passes afterward,
all ten original tests pass independently, no other file changes, and the native
`done` contains the actual canonical before/after rows. Its implementation and
usage notes were also inspected manually. All 58 raw-manifest entries were
verified; no selected credentials appeared in retained text.

The executable was rebuilt during validation and its hash differs from the
preceding wave, although the only Rust source delta is a `cfg(test)` regression.
The pre-run runtime label retained the production-logic base; the full build-tree
revision and actual frozen hashes are clarified in the append-only
[provenance note](report-followup/provenance-note.json). This adaptive single
attempt is not an isolated causal estimate or a general success rate. No
generated candidate was adopted into the repository implementation.

## Offline report assessment repair

The frozen assessor tagged the blocked JSONL report's phrase `After: PASSED is
NOT claimed` as a positive report field. Overall qualification was already false
because the native completion/check and independent artifact gates failed.
The new assessor recognizes bounded affirmative fields and rejects negation or
example prefixes. Canonical label requirements stay intact. Two regression tests
retain parenthetical-label rejection and prevent denial text from qualifying.

The [offline audit](reporting-audit.json) preserves and hashes both original and
new assessments, including their exact assessor snapshots. It makes no provider
calls: original Mistral qualification remains 0/2; the report follow-up remains
1/1. This is a measurement correction, not another successful live episode.

## Validation and next work

[Validation receipts](validation.json): 979 Rust tests plus integration (one
ignored), 91 Python tests, 85 Web/npm tests, 13/13 TUI replays, 14/14 repo-map
cases and server/UI smoke. Required tests, independent behavior, native reports
and live outcomes remain distinct evidence.

The next useful problem is source-to-documentation handoff after an expected
partial-task test failure. The JSONL run still spends several reads and refused
fixes before updating the next required artifact. Measure this against the
retained incomplete candidate with unchanged gates; do not pool it with the
successful smaller parser task. Task report obligations should also be made
less dependent on natural-language trigger words in a later typed-contract PR.
