# Completing the small examples with Mistral Coder + Observer

[Frozen plan](plan.json) · [executor pin](executor.json) ·
[baseline/reference controls](preflight.json) · [results](results.json) ·
[validation](validation.json) · [review](review.json)

The two predeclared practice attempts both pass the frozen strict qualification
predicate. Each uses Mistral Large 4 in both roles and 14 task requests, with no
transport credits: **2/2 qualified, 28 wire requests total**. These are two small,
visible development fixtures, not a general success-rate or causal estimate.

| Task | Coder / Observer | Native completion | Independent original tests | Strict scope + authored report |
| --- | ---: | --- | --- | --- |
| `event-summary` | 11 / 3 | completed | 10/10 | pass |
| `env-bool` | 11 / 3 | completed | 10/10 | pass |

The [JSONL candidate](large-large-event-summary.patch) repairs the parser and
updates its usage notes. The [settings candidate](large-large-env-bool.patch)
repairs boolean parsing and documentation. Each records a failing baseline,
native edits to exactly its two requested files, a separate successful exact
check after the last edit and a canonical before/after report. No supplied test
or instruction file changes. Both candidate implementations and docs were also
read manually. Generated candidates were not adopted into production source.

## What changed

The [earlier JSONL attempt](../2026-10-28-practical-examples/README.md) proposed
its docs update but was refused because the latest failed check still required
a fresh read of the test file. Observer had correctly proposed that read. Coder
chose other reads and then an edit; the rejection consumed another task request.

`next_action::Contract` now exposes that existing obligation as a `diagnose`
phase. Only `read_file` with the current exact path and `done` for a truthful
blocked report are offered. A provider that proposes another action still gets
a correlated refusal with no execution. Actual read success reopens the work
tools; an actual read error also releases narrow routing for access/path
diagnosis. Reading never resolves the failed check. Prior reads and advisory
overrides are not current diagnostic evidence, and the final report slot stays
available. No automatic read, synthesized edit or extra provider budget is added.

The compact next-action view also lists explicit `pending_artifact_paths` with
no native edit receipt. This keeps docs visible after a source edit without
claiming that an edited source is semantically correct. Both roles retain the
same task obligations and all original completion gates.

The real coupled regression was red before the change: after Observer's failure
handoff, Coder still received the full tool set. It is green with the exact-path
schema. Additional native-loop controls reject an unoffered signed edit, retain
its original call, reopen tools after successful and failed reads, keep the
exact check unresolved and preserve the final report slot.

## Frozen execution and limits

Both episodes use runtime source `3aa0390ee1ce48cf9c7d63504447407976c14020`,
with its actual executable SHA-256 in the executor pin. No source changed between
that build and the runtime commit. Prompts (revision 2), original tests, reference
repairs, assessor, roles and budgets were held fixed from the preceding PR #42
head. Each episode permits 24 task requests, 6 reserved for Observer, 4 transport
repairs, an 8-request Coder slice ceiling and 600 seconds. The maximum for this
wave was 56; actual usage was 28. Each case ran once with no retry in place.

The final assessment uses the frozen driver and verifies all **81** manifest
entries against their hashes, sizes and modes. Selected credentials were absent
from retained text. Earlier assessments and failed attempts remain unchanged.
The historical JSONL run used prompt revision 1, so comparing its 24 requests
with this run's 14 does not isolate the effect of this runtime change.

## What remains

Strict qualification checks recorded behavior, native proof, required report
rows and file scope. It does not grade every incidental natural-language claim.
The JSONL summary says it “always returns exactly” the all-zero dictionary; the
source and nonzero-count tests correctly count events. That phrase is misleading.
The settings summary also supplies a failure count of nine without a retained
native count supporting it; the frozen baseline control reports fourteen failure
events, including subtests. These authored summaries are preserved unchanged.

JSONL still spends two requests on Plan Gate refusals. Settings has one Plan
Gate refusal and one Evidence Gate refusal before the docs edit. The latter
correctly requires observing the documentation before mutating it. Plan repair
and factual precision in optional report prose remain useful follow-up work;
neither is silently counted as execution or verification.

## Verification

The required local script passes: **981 Rust tests plus one integration test**
(one ignored), **91 Python tests**, **85 Web/npm tests**, **13/13 TUI replays**,
**14/14 repo-map cases**, and server/UI smoke. Live outcomes and CI are separate
receipts. The PR records final CI and merge status on GitHub.
