# Harder repository maintenance with the GPT pair

The same `gpt-5.6-luna` Coder + Observer pair from
[r10](../2026-10-31-gpt-pair/README.md) attempted two larger repository-derived
tasks once each. **One produced a correct, verified patch. The other stopped
without a change.** The frozen assessor qualified neither because it also
rejected a valid count suffix in the successful report. A separate, post-hoc
assessor correction qualifies the replay task, with every other gate unchanged.

| Task | Coder / Observer requests | Artifact and native outcome | Frozen strict | Corrected strict |
| --- | ---: | --- | --- | --- |
| CLI case selection | 18 / 6 | 7 tests run, 1 failure event, no changed files, truthful blocked report | failed | failed |
| JSON replay extension | 12 / 3 | Python 1/1, TUI 5/5, both exact final checks and accepted report | failed | qualified |

[Frozen results](results.json) and [corrected assessment](reassessment.json)
are distinct artifacts. No candidate, receipt, prompt or qualification threshold
was changed after the calls. The correction recognizes the already-authored
`Before: FAILED` / `After: PASSED` rows followed by their native test counts.
The original outcome remains **0/2**, and the corrected assessment is **1/2**.
There were no additional provider calls or successful reruns.

## Task and measurement boundaries

The reusable [maintenance suite](../../../benchmarks/coding-v1/suite-maintenance.json)
uses the original `coding-v1` tasks from fixture origin
`8533870ce583ea95c302be6541178cb88a80e442`. The CLI task repairs selection in a
166-line pilot runner while preserving suite order, deduplication, defaults,
unknown-ID handling and no-overwrite behavior. The JSON task adds one bounded-read
Observer replay to a 1,101-line document while preserving four cases, arrays,
defaults and unknown nested fields. Both require documentation changes.

The model matrix and budget match r10. Each task has 24 task requests, six
reserved for Observer, four bounded transport repair credits and a Coder slice
cap of eight. The wall limit is 600 seconds, each request allows 90 seconds, and
the output caps are 32,768 Coder / 8,192 Observer tokens. The existing Luna route
uses `reasoning_effort=none` and omits temperature on the wire. This is a visible
development suite on repository snapshots, not a held-out capability estimate or
a full-repository self-repair.

Before live calls, offline controls exposed and repaired two measurement defects.

- Running the correct replay reference with r10 created 26 generated files
  inside the candidate workspace. Coupled child TUI replays now inherit an
  external run-owned receipt root, reserve unique directories and retain both
  concurrent reports. The same reference passes all five cases with **zero**
  workspace changes. Explicit replay output and the ordinary standalone default
  remain usable. Absolute-path and symlink checks reject an inherited root
  inside the candidate. Strict file inventory still examines every changed path.
- Python equality treated `false` as `0` and `true` as `1` in preserved JSON.
  Recursive comparison now keeps these types distinct. Both deliberate type
  mutations still passed the behavioral tests, but the corrected preservation
  check rejected them. JSON numeric `0` and `0.0` remain equivalent.

The frozen [controls](controls.json) and [preflight](preflight.json) retain the
old/new runtime result, type-mutation controls, failing baselines and passing
references. These changes preceded runtime commit
`53b1e99b1803a5e745b2c1b2c8714fd443cb99dd` and the
[pre-call plan](plan.json) committed at
`f20f1fe851f5ddab9a84eaca028c0f1c0144bbae`.

## What the live pair exposed

**CLI recovery continuity.** The pair reproduced the original test failure and
read its test location. One two-file `apply_diff` request failed because the
single-file parser treated `*** Update File` and `*** End Patch` framing as
unmatched old text. The pair then used the supported `patch_file` tool, but four
patch attempts were refused by the Recovery Gate. Three successful reads of
the source occurred after the failed diff. Two accepted Observer patch
recommendations have matching calls that the executor refused. The final report
correctly states that source, documentation and final verification are unfinished.

An [offline probe](recovery-probe.rs) reconstructs continuation state after each
of those three native reads. Every prefix retains `target_read=true`, an observed
test location and the pending failed check, yet restores `stage=diagnose`.
`restore_from_session` restores the generic failed-check state, then
`restore_pending_edits` advances only an empty state. The focused test path also
differs from the failed source-edit path, so focused-check restoration cannot
recover that edit diagnosis. This directly reproduces the continuity defect
without making another provider request or changing the live workspace.

The [diagnostic receipt](recovery-diagnostic.json) and [log](recovery-probe.log)
also retain the six original external recovery probes. The combined run has
four passing and three failing tests. Two failures reproduce lost diagnosis
(one synthetic, one from the actual session). The remaining historical control
fails its old internal assumption that an unrelated newer failure leaves the
previous location active. The current runtime instead selects that newer
obligation. It stops before that test's edit-gate assertion, so this run does
not claim that control passed or that its assertion proves a new regression.
The initial diagnostic adapter's missing-import compile error is retained in the
local diagnostic bundle and hashed in the receipt.

**JSON repair and report assessment.** After two initial plan/think refusals,
the pair read the fixture, reproduced the missing-case failure, read the
contract test and used a JSON-pointer append. It then read and updated the
documentation, ran both exact commands after the last edit and authored an
accepted report. All three accepted Observer handoffs have matching executed
calls. The new replay checks the requested advice and queued hint without
claiming to read the actual source file. The native external replay report has
five passing cases, and no generated files entered the candidate workspace.

Reapplying the [retained patch](replay-json.patch) reproduces every candidate
file and passes the original Python contract and TUI replay. Old JSON values,
boolean types, cases and array order are preserved. However, the JSON tool
rewrites object-key order, producing **559 added / 499 removed lines** for one
60-line append. This meets the frozen semantic contract but makes human review
needlessly large. Documentation adds four lines and accurately explains the
advice-only boundary.

The report's Python counts match the native receipts in both phases, including
one baseline failure event and zero final failures. `asserted-outcomes-v2` only
accepted punctuation or a newline after the quoted command, so it rejected the
parenthesized counts. `asserted-outcomes-v3` accepts that bounded count form.
Negative controls still reject negation and arbitrary parentheticals, and a
missing native exact check still fails. [Reassessment](reassessment.json)
changes only report recognition, its revision and the resulting qualification
booleans. Numerical truth remains owned by native count validation.

## Next self-repair targets

1. Preserve completed edit diagnosis across continuation while keeping every
   later failed check, failed read and new edit failure pending until its own
   diagnosis. Use the native prefixes above as a visible reproducer, then check
   the controls against current multi-failure ownership before freezing a new task.
2. Make the single-file diff boundary explicit and actionable. Preserve the
   malformed multi-file request and the supported smaller patch as controls.
   Unsupported framing should be rejected before it becomes a misleading anchor
   mismatch or an indefinite recovery loop.
3. Preserve unaffected JSON object order for a small structural edit. Keep the
   existing semantic/type preservation checks and add a reviewable-diff criterion
   prospectively; do not retroactively fail this candidate for key ordering.

The continuation defect remains open. These targets are now grounded in a
retained paired run rather than inferred from model prose. The gate fixes in
this PR improve measurement and receipt storage; they do not claim to repair
that remaining runtime defect or establish general autonomous-maintenance ability.

## Evidence and validation

Both declared episodes ran once, using **39 wire requests**, all charged to the
task allowance. All 39 provider responses completed. There were zero transport
charges, reconnect attempts or report-only closeouts. Token usage is unknown for
all 39 receipts. The CLI episode spent 125.456 seconds and the replay episode
72.986 seconds. Those times describe these runs only.

[Review details](review.json) preserve counts, reads, refused actions, handoff
matches, patches and the external replay report hash. All **290** entries in the
[raw manifest](raw-manifest.json) were verified against the retained local bundle,
including file hashes, sizes, modes and selected credential absence checks.
The manifest lists the original sessions, model messages, executable and traces.
Generated patches are review artifacts and have not been promoted into production.

Before the calls, local checks passed 1,009 Rust unit tests (one ignored), four
integration tests, 91 Python tests, 80 Web tests, five npm tests, 13 standard TUI
replays, 14 repo-map cases and the server/UI smoke. The post-run report parser
adds two Python tests. [Final validation](validation.json) records the checks
after that correction. The intentionally failing diagnostic probe runs in a
disposable copy and is separate from the ordinary test suite.
