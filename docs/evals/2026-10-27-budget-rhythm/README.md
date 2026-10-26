# Bounded transport repair and Observer cadence (2026-10-27)

This pass implements all six proposed mechanisms without changing qualification.
The four frozen episodes spent **77 provider requests** and produced **no
qualified repair**: baseline 0/2, new runtime 0/2. The new cadence runs live, but
this comparison does not establish better repair performance. Both new episodes
end on a Mistral response-body interruption at the declared 90-second timeout.

## Runtime and task changes

| Mechanism | Implemented contract |
| --- | --- |
| Transport repair pool | Up to four completed, wholly rejected framing/type responses can be charged outside the 32-task-request pool. Every wire receipt remains recorded. Semantic errors, failed checks and interrupted streams remain task charges. |
| Observer capacity | Eight task slots cannot be borrowed by Coder; Coder has at most 24. Pacing normally uses three Coder requests per Observer intervention. Early success, blocked reports and interruptions do not force unused calls. |
| Explicit task instructions | The new prompt gives the exact minimal reproducer, later-failure invariants and required docs edit. |
| Truthful blocked terminal | A native incomplete report preserves all obligations, receives one terminal Observer diagnosis and ends incomplete. Claimed completed criteria still need native evidence. |
| Final full native check | The prompt requires `cargo test --locked --bin spiral-coder -q` after the final mutation; the existing fresh exact-proof gate remains mandatory. |
| Development stages | Exact reproduction, measured behavior change, visible pass, control non-regression, fresh full check and docs/receipts are reported independently of binary qualification. |

The two budget options default to zero for compatibility. The experiment opts
in explicitly; this PR does not silently change every interactive session's
allocation. Local JSON opener recovery costs no additional request and therefore
does not consume a transport repair credit.

## Frozen comparison

The [plan](plan.json) was committed before calls. The task baseline is
`9982a68a6978058c75a9a700e7407df5a49f010c`; visible fixtures, the six-probe oracle,
reference repair, strict scope and original 864-test floor are byte-identical to
the previous visible-regression variant. The new prompt is a separate variant.
For each direction, baseline and treatment have identical prompt, tests,
reference and driver; only the executor and declared allocation differ.

- Old executor: `3a9e523f3ac147acf7c70f7fe4914f0623bca3ec`, binary SHA-256
  `a56753512abaa04f8dd0ba309d4b2cf8fed837952546da371a69c21baea2bbe2`.
- Live treatment executor: `7d2f558644a9e32635ddce023ab8cd99353116c8`, binary SHA-256
  `72afdb8174cf2f92057b8f2b366988fbbb0adc8a7bf156ddb2f43a0d08412b7c`.

There was one attempt per cell, in declared order, at most 136 wire attempts
overall, 1,200 seconds per episode and 90 seconds per provider request. Gemini
was excluded. No failed episode was replaced, resumed or rerun. The treatment
bundles runtime and allocation changes; it is not an isolated causal estimate
of any one mechanism or a comparison with previous prompt variants.

## Outcomes

Full gates, stages, request order and receipt hashes are in [results.json](results.json).

| Executor / Coder → Observer | Task requests (C + O) | Final artifact and stop | Contiguous development stage |
| --- | ---: | --- | ---: |
| Old / Luna 5.6 → Large 4 | 11 (9 + 2) | No change; Observer body interrupted | 1/6 |
| Old / Large 4 → Luna 5.6 | 32 (23 + 9) | No change; request allowance exhausted | 0/6 |
| New / Luna 5.6 → Large 4 | 8 (6 + 2) | No change; Observer body interrupted | 0/6 |
| New / Large 4 → Luna 5.6 | 26 (20 + 6) | `recovery.rs` changed; build fails; Coder body interrupted | 0/6 |

All four remain natively incomplete and independently unqualified. The first
three unchanged candidates build, pass 864 original tests and the frozen TUI
replay, but still fail the visible test (0/1) and independent probe (5/6).
The last candidate references an unimplemented `current_failure_diagnosed`
method. Build and test compilation fail with E0599; no behavioral test result
can be inferred from those failures. Replay cannot start without a candidate
binary. The frozen assessor represents that missing-binary skip with
`timed_out=true`, `exit_code=null`, zero elapsed time and no log; it is **not an
observed replay timeout**. The original receipt is retained unchanged.

The new Luna trajectory contains a successful patch receipt for identical
search and replacement text, but its final byte-level diff is empty. Thus
`native_edit=true` and `actual_edit=false`; an edit receipt alone is not progress.
The new Large trajectory first makes the same overbroad Diagnose-to-Fix
promotion seen in earlier rounds. Its automatic suite catches
`restoring_an_edit_diagnosis_does_not_clear_a_later_failure_or_invent_proof`.
The subsequent repair introduces the missing method. One fresh-failure handoff
is recorded, but the final artifact does not compile.

Large's native reproducer adds `2>&1 | tail -60` to the declared exact command.
The visible assertion really fails, and treating the pipeline's zero exit as a
test failure is correct. However, it does not satisfy the frozen exact-command
reproduction milestone. The count remains 0 rather than retrospectively
normalizing the command. The final behavioral-change stage is `not_evaluated`
because compilation fails. See all six individual stage values in the JSON;
the contiguous count alone does not describe every observation.

## What is demonstrated, and what remains untested live

Both treatment ledgers honor the 24-Coder task ceiling. Their request orders
show three-Coder slices, with an earlier return on a fresh failure. They stop
before consuming all eight Observer slots; full exhaustion behavior is covered
by the deterministic 24-Coder/8-Observer test, not by these live episodes.

One malformed Mistral opening prefix recovers locally in the new Luna run.
**No transport credits are consumed**, so there is no live evidence of a
transport-pool benefit. Neither new run reaches an accepted blocked terminal;
that path is covered by deterministic integration tests. Both stream failures
are now explicit `interrupted` outcomes with a nonzero process exit and retained
receipts. They do not obtain free transport repair attempts.

All **3,651** raw file entries match their sealed hashes, sizes and recorded
modes. All sixteen package-cache resets pass. Input and driver equality was
verified for both pair comparisons. No episode exceeded its wall/output cap,
no secret redaction occurred, and final artifacts stay within scope. The retained
[Large candidate patch](candidate-large-luna.patch) is failure evidence; it has
not been adopted into the runtime. The other three retained patches are empty.

## Review, deterministic checks and follow-up boundary

Fork review found three P2s and one P3: same-turn blocked-plan validation,
unfunded final Observer repair, clean SSE EOF being mistaken for completion,
and arbitrary command errors being mistaken for reproduction. All four were
fixed; the corrective recheck found no additional actionable findings.

The frozen live source passes 951 Rust tests plus server integration (one
existing ignored), 11 TUI replays and 76 Python tests. Earlier full local checks
also pass 80 Web tests, five npm tests, fourteen repo-map cases and server smoke.
[CI for the live executor](live-executor-ci.json) passes both quality platforms,
five native targets and package assembly; publication is skipped intentionally.

Commit `95701b8a12bb6f49e49eacb98d836313b4707acb` adds a separate guard excluding
historical `done`/`blocked` narration from citable acceptance evidence on resume.
Its regression fails before the fix; 952 Rust tests plus server integration
pass afterwards. A focused review is clean and [its CI](followup-guard-ci.json)
passes. This guard is **not in the frozen live executable**, so no live outcome
is attributed to it. [validation.json](validation.json) keeps these evidence
classes separate.

The next useful experiments should isolate byte-level no-op mutation reporting,
exact-command adherence, and constructing a complete repair after the correct
later-failure test is read. Provider interruption needs its own predeclared
availability/control investigation. These are open issues, not reasons to relax
the protected tests, docs requirement, exact proof or binary qualification.
