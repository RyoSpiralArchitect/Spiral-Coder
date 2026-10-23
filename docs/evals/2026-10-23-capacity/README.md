# Coupled capacity experiments, 2026-10-23

The same task exposed three different limits: a provider stream-decoding defect,
runtime gates that lost valid recovery opportunities, and unfinished proof/report
closeout after a correct edit. These observations motivate harness changes; they
do **not** establish a coding-capability ranking between Gemini models.

Every episode uses the same selected model for Coder and Observer. The
[reusable development suite](../../../benchmarks/coding-v1/README.md) freezes
tasks, initial files, runtime binary, model parameters, driver and independent
oracles before calls. Its two new repository-derived tasks accompany the retained
large-file task. Copies can also be used for interactive TUI dogfooding.

## Conditions and accounting

The suite has three cases: `large-file` (18 shared wire requests), `pilot-cli`
(24) and `replay-json` (24). The CLI and replay cases use snapshots from
`8533870ce583ea95c302be6541178cb88a80e442`, with a deliberate CLI-selection bug
and a new replay requirement. Reference repairs are outside candidate workspaces.
Original tests execute independently with only the candidate editable artifacts
copied into an isolated directory. Candidate tests cannot certify their own edit.

All profiles use temperature 0, a 32,768 output-token ceiling, a 90-second request
timeout and a 600-second episode wall cap. Provider thinking defaults are kept;
equal caps do not equalize compute, latency or cost. The increased token ceiling
follows Google's [Argon output-limit guidance](https://ai.google.dev/gemini-api/docs/latest-model#setting-output-token-limits).
GPT-4.1 is the larger counterpart of the existing GPT-4.1-mini pilot, **not** the
latest OpenAI flagship. The separate model-catalog PR #34 is not a runtime
dependency of these explicit model IDs. Mistral uses `mistral-medium-3-5`.

| Round | Runtime commit | Episodes | Requests used / ceiling | Independent artifacts | Behavior + native proof + report | Strict all files |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| R1 | `a0488fe55c3ce875788d51274a66a8dd6e946447` | 12 | 170 / 264 | 7 / 12 | 4 / 12 | 0 / 12 |
| R2: stream repair | `4a139ec` | 6 | 117 / 132 | 1 / 6 | 0 / 6 | 0 / 6 |
| R3: recovery gates, CLI only | `625e4bc` | 2 | 48 / 48 | 1 / 2 | 0 / 2 | 0 / 2 |

There were **335 task requests**, plus **two separate protocol probes**. No episode
was retried in place or given a fallback model. The three rounds are adaptive and
have different panels; do not aggregate their pass rates as a leaderboard.
Mistral returned usage for 44/44 requests. Gemini and OpenAI usage is absent in
these retained stream receipts, recorded as unknown rather than zero. The
[machine-readable comparison](comparison.json) records per-episode results,
role counts, pre-tool rejections, usage coverage and immutable assessment hashes.

Strict qualification keeps every changed path, including `.spiral-coder/`
runtime state, Python bytecode and replay output under `.tmp/`. These paths were
not removed or excluded to improve a score. All runtime completions in R1 also
pass the behavior/proof/report measure; none satisfies full file scope.

## R1: four-model baseline

Each cell shows **independent artifacts / qualified closeout**, followed by wire
requests. A completed test run alone is not qualified closeout.

| Same model in both roles | large-file | pilot-cli | replay-json |
| --- | --- | --- | --- |
| Gemini 3.5 Flash-Lite | pass / fail · 18 | fail / fail · 24 | fail / fail · 24 |
| Gemini 4 Argon | fail / fail · 1 | fail / fail · 1 | fail / fail · 1 |
| GPT-4.1 | pass / pass · 16 | pass / pass · 17 | pass / fail · 24 |
| Mistral Medium 3.5 | pass / pass · 10 | pass / pass · 10 | pass / fail · 24 |

Argon stopped at its first stream in all three tasks: `streamed tool-call id
changed at index 0`. A separate minimal request showed distinct explicit call
IDs in successive SSE deltas without an `index` field. The decoder treated each
array position as the same call. This was a transport defect before native task
execution, not evidence of weak coding ability. The old runtime also exited
before writing its report; the durable work ledger accounts for each request
without supplying invented execution proof.

Flash-Lite's CLI run repeatedly narrowed or moved a read range after an oversized
request. Reflection identity included only the path, so five changed requests
were rejected as unchanged actions. Its large-file edit passed the independent
tests but did not finish native proof/report obligations. In the JSON task it
spent its budget on observation and gate recovery without the requested edit.

GPT-4.1 and Mistral completed the CLI and large-file tasks. Both produced JSON
artifacts that passed independent checks, but GPT-4.1 ended with pending native
verification after a later repair. Mistral had fresh native checks but no accepted
authored final report. A larger model did not remove the closeout failure mode.

The first offline assessor incorrectly rejected Mistral's CLI documentation: its
example used a shell line continuation, and its unknown-ID explanation said
"does not match a known". Assessor revision `shell-whitespace-v2` accepts those
forms. R1 assessment 001 (6 artifact passes, 3 qualified) is retained; 002 fixes
that false negative (7, 4), and 003 additionally exposes pre-tool rejection data.
This is an **assessor correction**, not a model or runtime improvement. The
documentation check remains a bounded lexical proxy, not a prose-quality judge.

## R2: streaming repair exposes the next boundary

The decoder now groups ID-only streams by explicit call ID, preserving first-seen
order and call-owned continuation data. Ambiguous fragments and indexed identity
conflicts still fail. Failed coupled loops now retain their incomplete report,
session and consumed budget before returning an error. These changes are in
PR #31; regression tests cover the actual failure shape.

Fresh Gemini pairs reused the exact task hashes, initial inventories and matrix.
Flash-Lite consumed 16/24/24 requests, passed the large-file artifact check and
completed no task. Argon now decoded successfully, then repeatedly returned
multiple tools while the executor required one. It consumed 14/20/19 requests,
with **53 single-tool pre-execution rejections and zero native actions**. Its
failure display used `[governor]`, which the Observer's failure-target detector
did not recognize; no Observer request was made in any of these three episodes.

A second protocol probe sent `parallel_tool_calls: false`. This one HTTP 200
response still contained two calls. We did not add that flag as an assumed fix,
execute a rejected batch, or infer a universal provider-parameter guarantee from
one response. The sanitized [ID-only](protocol-probes/id-only-calls.json) and
[parallel-disabled](protocol-probes/parallel-false.json) receipts retain raw SSE
hashes. Opaque continuation data stays in the local raw evidence.

## R3: predeclared recovery changes

Before running the fresh CLI pair, the [intervention plan](r3-intervention-plan.json)
declared two changes: compare effective read bounds in reflection identity, and
use the recognized `GOVERNOR BLOCK` marker for tool-batch rejection. The task,
initial files, model parameters, independent checks, proof requirements and
24-request cap remain identical to R2. This joint intervention and one trial per
model do not identify separate causal effects.

Flash-Lite now executed changed ranges, repaired a suite-order error through
several native patches, updated `docs/pilot.md`, and passed the exact configured
check after the final edit. All independent artifact checks pass. It spent
23 Coder + 1 Observer requests and had **no accepted final report**, so completion
remains false. Its R3 trace no longer contains a false unchanged-range block.

Argon reached Observer rescue: 23 Coder + 1 Observer requests, compared with
20 Coder + 0 Observer on R2's CLI. Its 23 Coder turns still hit the single-tool
gate; no native action executed and the Observer assist did not execute. The
marker repair restored the recovery route, but did not resolve the model/tool
contract mismatch or complete the task. Calling the Observer is not itself a
successful rescue.

## Harness requirements inferred from the failures

These are scoped next experiments, not claims of implemented capability gains.

| Evidence | Requirement to test next | Required success signal |
| --- | --- | --- |
| Argon spends every Coder turn on rejected multi-call responses; weak models also spend calls repairing prose gates | A structured next-action contract with a concise plan/status and one executable action; make allowed transitions visible without requiring repeated prose-tag repair | Accepted native actions on the unchanged tasks, with rejected/ambiguous calls still never executed |
| Valid revised ranges were blocked, and one failure marker bypassed Observer | Use typed action identity and failure categories end to end; range identity and the display-marker repair are the first fixes | Changed effective arguments can advance; identical retries still block; every actionable failure can reach the shared rescue path |
| Flash-Lite R3 and Mistral JSON reach fresh checks but never author the final report | Reserve closeout capacity inside the same total cap; give both roles a compact ledger of remaining artifacts, exact checks and report fields | Authored accepted completion after fresh proof, without synthesizing a report or spending extra requests |
| Model source edits and native checks can pass while file scope fails | Explicit ownership/location for harness state and test outputs | Strict all-file qualification improves while the oracle still counts every path |
| A literal documentation checker misclassified a valid result | Keep artifact semantics, report formatting and evaluator proxies distinct; version offline reassessment | Retained old assessments, negative proxy tests, and no reclassification presented as runtime gain |

The next useful coding run should target the structured action/closeout contract
with this same pair and the preserved failure fixtures. Add a separate unseen
task set before any broad reliability claim. These repository snapshots are
development exercises, not full-repository maintenance or held-out evaluation.

## Evidence and validation

- R1: [declaration](r1/predeclared.json), [offline preflight](r1/preflight.json),
  [initial assessment](r1/assessment-001.json), [corrected proxy](r1/assessment-002.json),
  [final assessment](r1/assessment-003.json), [raw manifest](r1/raw-manifest-003.json).
- R2: [declaration](r2/predeclared.json), [offline preflight](r2/preflight.json),
  [assessment](r2/assessment-001.json), [raw manifest](r2/raw-manifest-001.json).
- R3: [declaration](r3/predeclared.json), [offline preflight](r3/preflight.json),
  [final assessment](r3/assessment-002.json), [raw manifest](r3/raw-manifest-002.json).
  The local assessment 001 was an in-flight snapshot with one episode unfinished;
  it is retained locally and is not used in any final count above.

All three broken fixtures fail their baseline check and their reference repairs
pass offline. Final runtime validation: **812 Rust tests passed, 1 ignored;
1 integration passed; TUI replay 5/5**. The benchmark and existing Python tests
pass **35/35**. The new runner's negative coverage includes rewritten candidate
tests, lost JSON values/order, symlinks, stale native proof, missing report fields,
out-of-scope runtime files, process timeout, lost failed-stream reports and
refusing in-place retries. Earlier runtime snapshots have their own retained
validation; passing deterministic checks is separate from these live outcomes.

Full sessions, traces, candidate files, runtime binaries, driver snapshots and
protocol streams remain under the three `local_root` paths in the manifests.
Published assessments retain ordered action excerpts, independent-check output,
per-request ledgers and native proof fields. They omit raw provider continuation
data. Each final manifest hashes the retained bytes and the assessment; it is an
audit locator, not a substitute for the actual result. Historical benchmark
suites and their failures remain unchanged.
