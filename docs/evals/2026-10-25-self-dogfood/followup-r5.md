# Runtime follow-up: grounded progress and action receipts

This is a fresh runtime-only continuation of the [foundation experiment](README.md).
Exactly two new episodes were run, using 31 requests in total. Across all six
recorded episodes, 131 requests were reserved and no repair candidate qualified.

## Frozen conditions

Executor and driver source: `5b9315dbfb4fbfc134b5a98ae59e658837f94d6d`.
Executor SHA-256: `5279c0f6b8719863bec83f38d47b192574c4ddb52c34c6ff4080eaacff69165c`.
Candidate baseline remains `9982a68a6978058c75a9a700e7407df5a49f010c`.
Each pair's entire frozen input inventory matches r4: task, six-case independent
probe, reference, editable/protected paths, required docs and exact native check.
The 32-request cap, initial Coder slice of eight, 1,200-second wall cap,
90-second request timeout, 32,768 Coder tokens and 8,192 Observer tokens are
unchanged. Luna still uses reasoning_effort=none. Baseline/reference preflights
passed for both pairs before any calls. No episode was retried in place.

The runtime now distinguishes passing Rust test rows from real error markers,
including after preview/history shortening. Shared work progress separates the
original task from declared proof obligations and native activity. Exact
repeated read/search outputs can be refused without an active check failure;
new ranges and required diagnosis remain allowed. Assistance receipts now
separate whole-continuation execution from exact proposed-action execution.

## Results

| Coder → Observer | Coder / Observer requests | Seconds | Native complete | Target probe | Qualified |
| --- | ---: | ---: | --- | --- | --- |
| Luna → Argon | 20 / 1 | 264.987 | Yes | 5 pass / 1 fail | No |
| Argon → Large 4 | 8 / 2 | 200.497 | No | 5 pass / 1 fail | No |

[Selected receipts](followup-r5-results.json) retain gates, effective settings,
external pins, hashes and the new assist outcomes. Argument digests in this
public summary use sorted compact UTF-8 JSON; the full proposal remains in the
local pinned native report. All raw-manifest file hashes were verified: 930 for
Luna and 867 for Argon. No credentials required redaction. Both episodes passed
build, original Rust tests and candidate-built TUI replay, preserved file scope
and exact role accounting, and failed the same independent target regression.
Recorded native accesses stayed inside the candidate; this is not OS isolation.

### Luna → Argon

The output-classifier fix was exercised: `cargo test 2>&1` returned native
`OK (exit_code: 0)` with passing tests. Comparable verbose commands had been
misclassified as failed in r4. The Observer correctly identified the unimplemented
source repair and an earlier byte-identical patch. Its proposed search was first
blocked by Think and then executed. The new receipt retains both matching call
IDs with `executed=true, succeeded=null`; search execution does not by itself
prove successful advice or repair.

There were 25 native actions, 14 executed and 11 blocked. Several blocks came
from a six-call batch and protocol gates. The duplicate-observation gate did
not fire in this episode, so this run does not establish its live benefit.
The last exact check followed the final mutation, and native done was accepted
with no pending checks. Every qualification gate except independent checks passed.

The [retained patch](followup-r5-luna-argon.patch) adds documentation and guards
restoration with `g.stage.is_none()` immediately after initializing that stage
to None. The condition is always true there and does not fix handoff. The final
report overclaims repair; the independent probe still observes Diagnose instead
of Fix. This candidate is not adopted.

Two older runtime gaps also remain visible. An earlier source `apply_diff`
request was replaced with a documentation read by required-follow-up coercion;
that source action was not executed. Another source patch used identical search
and replacement text, yet received `OK: patched` and an automatic test. Native
write counts are accurate as write records but do not establish changed bytes,
let alone changed behavior.

### Argon → Large 4

The Coder made observations but no edits or execs during its eight-request slice.
The Observer now explicitly recognized zero source writes and zero commands,
instead of reducing the task to documentation and testing. It nevertheless
asserted that diagnosis was complete merely because files had been read and
proposed preserving a field already present in the source. Those assertions
remain model hypotheses, not evidence.

Both the first Observer output and its one permitted format repair lacked the
opening of the JSON object. The first started with `summary":`, and the repair
with `"summary":`; both ended with `}`. Adding the missing prefix offline makes
the JSON parse, but the primary patch proposal still supplies only `path`,
without `search` or `replace`. Syntactic salvage cannot create an executable
edit. The Observer response parser retained the invalid output and forwarded no hint,
so there were no assistance continuations and 22 requests remained unused.

Both requests completed below the 8,192-token cap (6,953 and 5,263 completion
tokens). This was neither empty output, an observed length-limit failure nor a
timeout. The duplicate-read gate did not fire. No exec or assist continuation
occurred, so this episode cannot measure the classifier or action-receipt live
behavior. The final patch is empty and completion remains false.

## Validation and review

The updated runtime passed 918 Rust tests with one pre-existing ignored test,
one integration smoke, 61 Python tests, 85 Web/npm tests and TUI replay 7/7.
[Validation receipts](followup-r5-validation.json) bind local logs and source CI.
Linux/macOS quality checks, all five native builds and package assembly passed
for the frozen runtime source. Publication was skipped for the PR.

Fresh fork review found two P2 issues: pending approval had been mislabeled as
a failed proposed action, and ordinary repair tasks could omit the new Coder
progress prompt. Both were fixed and covered through actual native projection
and prompt APIs. Focused re-review found no remaining actionable issue. Project
context files were restored after each review. The new duplicate-observation
control has deterministic coverage, but these two live episodes did not exercise
its refusal path.

## Next bounded tasks

1. Introduce a separately labeled task variant with a visible, protected failing
   behavioral regression before editing, while retaining the independent six-case
   controls. Keep its baseline/prompt changes separate from runtime-only comparisons.
2. Distinguish byte-identical writes from meaningful mutation/progress, and avoid
   replacing an authored source repair with a documentation-only follow-up action.
3. Preserve useful Observer diagnosis when formatting fails without inventing
   missing edit arguments or treating unsupported proposals as executable actions.

No further calls were made after these two attempts. Their small, stochastic
sample does not establish a causal improvement rate or a model ranking. Native
completion, action execution, correct repair and adoption remain separate.
