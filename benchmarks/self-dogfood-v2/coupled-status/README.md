# Inspect a long coupled run from inside Spiral-Coder

The next additive self-dogfood task is `spiral coupled-status --run-dir PATH`.
It crosses native CLI wiring, durable request accounting, the installed npm
entry and documentation. A same-model Luna Coder/Observer pair must add the
feature to a frozen full copy of the merged repository, reproduce its absence,
verify the result after all edits and write an evidence-bound report.

The command reads native iteration manifests and child work ledgers. It must
include pending spend exactly once, preserve unknown usage as null, validate
counts and task linkage, and avoid claiming independent qualification from a
recorded completed run. It must not modify records or call a provider.

The operator freezes 24 independent cases, a visible end-to-end check, a real
assembled npm check, a measured documentation example and the entire existing
local check suite. The baseline must fail because the command is missing; the
external reference must pass before any requests. Neither the oracle nor the
reference is copied into the candidate. The visible fixture and checks are
protected. Existing tests retain their identities and results; existing state
schema documentation must be preserved.

Use `materialize.py OUT --baseline COMMIT` to expand the reference outside the
repository, then the normal self-dogfood prepare procedure. The prepared task
uses six native iterations, 32 task requests per iteration, eight reserved for
Observer, four transport repairs and a Coder slice of eight. Total caps are
192 task requests, 216 wires and 3,600 seconds. Output limits remain Coder
32,768 and Observer 8,192; request timeout is 90 seconds. The first terminal
outcome is retained, including no-progress stops and provider errors. These
are larger task and budget conditions than #49, not a matched efficiency trial.

The frozen launcher and assessor are the reviewed three-round catalog wrappers,
with `iterations=6` declared in a new plan. The assessor validates every native
child ledger, required fresh check, scope, sealed patch and independent group.
Candidate and executor remain separate. Successful candidate patches require
review and a separate adoption decision; the live runtime is never replaced.

Later steps depend on this result. First use the status command on actual saved
runs, then target a bounded internal harness change with a failing reproducer,
neighboring controls and unchanged completion gates. Only after that consider
multi-feature work that changes both persistence and execution behavior.

## Progression after the first longer run

[The first frozen series](../../../docs/evals/2026-11-11-coupled-status/README.md)
retains four launches through S4. S4 reaches four current exact checks and a
measured guide in four iterations, but misses three independent cases, existing
document preservation and the accepted final report. It is not qualified.

1. Finish this read-only feature under the unchanged contract: valid summaries,
   contradictory/duplicate/symlink rejection, original document preservation,
   current checks and an authored report. Then inspect saved real runs on copies.
2. Freeze one internal control repair with a failing native reproducer, positive
   continuation and negative stop controls, TUI replay and the original test set.
   The pair edits the candidate harness; the separate executor/evaluator remains
   frozen. This demonstrates a bounded internal repair, not recursive deployment.
3. Only after qualification, combine two dependent features and an interrupted
   continuation: preserve accepted work and evidence, complete the second change,
   rerun affected and full checks, then report. Freeze the total multi-episode
   budget before calls, retaining no-progress and incomplete terminals.

These are capability milestones, not promises that the current pair will pass.
Raising the cap alone does not close missing constraints or final delivery.

## S10 task-derived validation obligations

S10 promotes the original behavior-contract bullets into separate descriptions
in the existing typed task requirements. Coder and Observer retain those exact
rules through request compaction and continuation; plan edits cannot replace
them. The already requested `tests/coupled_status.rs` becomes a required native
artifact and `cargo test --locked --test coupled_status -q` becomes the fifth
exact final check. Passing authored tests proves only their executed cases;
these descriptions are not automatically established semantic truth.

This is a new, stronger input condition, not a matched comparison to S9. The
24-case independent oracle, visible fixtures, baseline and request/time caps
remain unchanged. The operator reference includes its own focused test file;
neither that file nor its implementation is supplied to the candidate. Frozen
S1–S9 records retain their original inputs and four-check contracts.

## S11 boundary controls and authored-test sensitivity

S11 adds a sixth exact check, `python3 -S -m unittest -v scripts.status_paths`.
Its three tests exercise 26 subcases: missing active/interrupted directory,
state directory or ledger file; malformed existing pending ledgers; and live or
dangling symlinks at each record path component. Missing usage stays unknown;
malformed or symlinked existing records remain errors. The original 24-case
independent oracle and its acceptance threshold are unchanged.

`test_sensitivity.py` separately transplants the authored CLI tests onto a
correct reference copy. Tests must first pass there, then detect each of four
advertised fault families: missing-pending rejection, symlink acceptance,
duplicate-request acceptance and parent-link acceptance. Each injected fault
must first change an operator control case. Compile failures, timeouts, ignored
tests, or changed executed test counts do not qualify as detection. The
reference, controls and fault implementation stay outside the live workspace.
This is limited fault sensitivity, not comprehensive coverage or proof of test
quality. The audit has its own frozen 600-second evaluation budget, no provider
calls, and never modifies the sealed candidate. Its result is an additional
qualification gate; it cannot rescue a failed original gate.

The prepared baseline explicitly overlays the prior test-only environment
isolation in `src/config.rs` and `src/config/env_test_support.rs`. Assertions,
existing test identities and production provider logic are preserved. This
removes shared-process environment mutation from those two tests. It does not
rerate S10's retained stock-check failure. These overlays, new public controls
and stronger prompt make S11 a new condition, not a matched S10 comparison.

## S12 native fixtures and uncertain-advice handoff

The protected `native_work.json` template is produced by the real WorkBudget
writer: create a durable ledger, reserve three requests, finish them, reclassify
one as transport, and read it back through `WorkState`. Only the task identity and
clocks are normalized. The test
`work_budget::native_fixture::saved_work_state_fixture_uses_native_serialization`
compares native serialization to the checked-in template without provider calls.
To export to a new file, set `SPIRAL_NATIVE_FIXTURE_OUT` for that exact test;
existing files are never overwritten. The public `status_fixture.make` helper
retains the native fields and varies scenario values. The manifest fixture and
expected output contract remain the same; only the ledger template is generated
by the native writer.

Coder/Observer now retain an explicit typed obligation to use that helper from
public-CLI tests, establish an accepted base fixture, and change one invariant
per negative case. Numeric request IDs and unrelated native fields must survive.
This is a stronger input condition; the original oracle, six exact checks,
four-fault sensitivity gate, acceptance thresholds and provider caps are unchanged.

A current valid Observer primary below 0.75 cannot become executable advice.
The separate executor can spend one existing Coder request on an honest report
instead. A progressed accepted blocked report uses the existing bounded next
iteration rule; neither uncertain advice nor the new report opportunity creates
completion evidence, permission or extra budget.

Rust tests can invoke `python3 -S scripts/status_fixture.py TEMP_DIR completed`
with `Command` arguments, then invoke the native status binary and assert success
before mutating one field. The helper also accepts `running`, `interrupted`,
`unknown` and `stopped`; it calls the same protected `make` function used by the
original visible and independent cases. Operator reference tests exercise this
CLI entry before any live run. No candidate assertions or implementation are
supplied by the helper.


## S13 fixture shape and current failure guidance

The typed test obligation explicitly requires inspecting row/request cardinality
before indexing. The unchanged completed helper contains one finalized row. Any
extended positive scenario must itself succeed before one negative mutation;
cloning records just to bypass a setup panic is not test repair. No fixture,
reference, oracle, threshold, sensitivity fault, check or budget is weakened.

The executor suppresses stale implementation guesses while a correlated native
failure remains pending. Shared failure guidance retains its current location,
cause and exact check after diagnosis and resume. A diagnostic location is not
edit permission: original scope, supplied-test preservation, recovery and final
evidence gates still decide admissible actions. Once the relevant checks actually
succeed, ordinary implementation discovery resumes. S13 combines this runtime
change, the post-S12 diagnostic correction and a stronger prompt; it is not a
matched causal comparison.


## Reference correction after the PR #83 fork review

The operator reference now checks known finalized sums against the declared caps
before an absent pending ledger makes total usage unknown. A missing suffix cannot
hide a known wire-budget violation. A new operator-only native CLI regression
covers active/interrupted missing-ledger cases: at-cap finalized usage remains
valid with null total/remaining fields, but one additional known wire is rejected.
The original 24-case oracle, public fixtures, fault definitions and live budgets
are unchanged. This correction applies to newly prepared runs; S1–S13 frozen
references, receipts and qualification results are retained without rerating.
