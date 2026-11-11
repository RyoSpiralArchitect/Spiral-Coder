# S11: missing pending paths improve; test sensitivity rejects the candidate

S11 is a fresh, unseeded Luna Coder/Observer run from the original feature-free
source plus the explicitly declared test-only config environment isolation.
The new public matrix covers missing directory/state/file, malformed pending
JSON and live/dangling symlinks; its command is the sixth exact final check.
The original 24 independent cases are unchanged. Authored tests must also pass
a valid reference and detect four advertised fault families on a separate copy.
These are stronger inputs, not a matched comparison with S10.

Preflight passes all seven reference groups. Before any provider calls, the
reference test control detects all four faults, a thin test control misses three,
and unchanged S10 authored tests fail the valid-reference prerequisite. The
new evaluator correctly refuses to infer coverage from names or test counts.
[Prospective controls](../s11-sensitivity-controls.json) · [Frozen plan](plan.json)

The first terminal is retained: **1,120.588 seconds, four rounds, 124 task
requests / 137 wires**, terminating as `observer_unavailable`, incomplete.
The rounds spend 32/33, 32/36, 32/36 and 28/32 task/wire requests. Only four of
six required paths are edited; both documentation deliverables are absent.
The visible check passes in round four, but the subsequent authored-test edit
invalidates that proof and fails its automatic check. All six final exact checks
remain pending, and no final done/report is accepted. Earlier blocked reports
remain in the history; they are not a final completion report.

The last Observer response is valid and does describe the remaining horizon.
Its primary edit confidence is **0.74**, below the existing **0.75** hint threshold.
Only the primary action is eligible; no hint or native read window is produced,
and the scheduler stops with Coder capacity left. `termination::project` maps
that fallthrough to `observer_unavailable`. This is **not an observed API outage**.
The two preceding repair attempts were rejected by the plan gate after inline
acceptance text drifted from the admitted four-item plan. The threshold and
scheduler are unchanged in this record. [Terminal diagnostics](diagnostics.json)

Independent sealed-patch replay:

- Build, original visible check and actual npm routing pass.
- Original oracle: **21/24**. Completion conflict, interrupted total conflict
  and ledger/task-ID mismatch still fail. This is below S10's 23/24.
- Public path matrix: six missing-pending subcases and two malformed-existing
  subcases pass. Six dangling pending symlinks are incorrectly treated as
  absence. The three methods cover 26 subcases; six fail. `Path::exists()`
  follows dangling links, so it cannot establish that a path component is absent.
- Guide fails because it was not authored. Stock regressions stop at rustfmt;
  subsequent stock suites are **unrun**, not passing.
- The live automatic test records 1,148 main Rust passes/two ignored, followed
  by one authored pass/one failure. This does not replace independent checks.
- Both authored tests fail on the valid typed reference. The success fixture
  uses a string request ID and omits native fields. The test named for wrong
  parent linkage changes a limit without its checked product and asserts success.
  Because reference compatibility fails, mutant test executions are skipped;
  no sensitivity credit is awarded. All four operator fault controls do exhibit
  their intended changed behavior. This is not a claim that four mutants were
  executed against the authored tests.

**Unqualified; #82 remains open; no candidate adopted.** New controls expose
specific failures, but this single run does not establish improved model ability,
causality, efficiency or repeatability. Candidate and frozen executor remain
separate from the post-run diagnostic change below.

A post-run host correction filters exact successful Rust test-result lines from
failure digests. In S11, three passing test names containing `failed` and
`assertion` occupied the four-line diagnostic before the actual assertion.
A regression using those observed lines fails before the fix and preserves the
real panic location/assertion after compaction and pruning afterward. It changes
display diagnostics only, not status/evidence/acceptance authority. The sealed S11
result is unchanged; no fresh provider run of this post-run fix is claimed.
[Post-run validation](postrun-validation.json)

[Results](results.json) · [Exact patch](candidate.patch) · [Context](context.json) ·
[Preflight](preflight.json) · [Frozen executor checks](runtime-validation.json)

Next work should keep low-confidence advice non-authoritative while distinguishing
it from missing/invalid Observer output and providing an honest bounded handoff.
Authored tests should derive valid native fixtures first, then change one invariant
per negative case. Re-running the same inputs until a green result would not
resolve these identified failure modes.
