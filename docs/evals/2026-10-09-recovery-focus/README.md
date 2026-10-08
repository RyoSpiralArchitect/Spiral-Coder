# Localized automatic-test recovery — 2026-10-09

This change retains the edited file, diagnostic, and original check after a
localized automatic-test failure. Unrelated diagnostics cannot substitute for
reading that file, and unrelated passing checks cannot close the task. The live
runs exercise failure localization and target rereads, but neither provider
completes the TUI replay case. The branch remains unqualified for runtime
promotion.

## Frozen comparison

The six cases, checks, and iteration caps are unchanged from
[the restart spec](../2026-10-03-restart/runtime_eval.json). This is a fresh
comparison against the preceding implementation, not a revision of
[the 2026-10-08 evidence](../2026-10-08-harness/README.md).
[receipts-first-candidate.json](receipts-first-candidate.json) preserves the first
candidate. [receipts.json](receipts.json) records the revised comparison, its
revision reason, and the first receipt's hash. Both include identities, all check
outcomes, selected metrics, retained recovery snapshots, and raw artifact hashes.

| Run | Source commit | Binary SHA-256 |
|---|---|---|
| Baseline | `37d36ca0d1f916f7ae4c68fd919e3210dbf4926b` | `b556b8810f4ceb38713e21ad184eed9865d3cbf438780e98b09ab11c932a8754` |
| First candidate (superseded) | `48df8615fe30e4a8869da5371a5abea6cbd28af2` | `8ed06421b7d5026e4fb77fec09e95d2d3e0348cda8ed5ee8eab85af90c6c1d53` |
| Revised candidate | `bed7e03bf7cc1da17784f89e229aa4cdb1c5207c` | `523aff3b26f133df69357a79842435e2c1ddba82e79c820816763b59fbfb3c5c` |

The baseline is the prior candidate executable; the intervening PR commit only
added documentation. The candidate was built and frozen before this evidence
was added. Spec SHA-256 is
`2edcc57bb2291ed10d0130b9a0997e018e52b0593d0fa576e59bc064a66c5db0`.
All 33 fixture file hashes match across the comparison.

Conditions: `mistral-small-2603` and `gpt-4.1-mini`; temperature 0; 2,048 output
tokens; 90-second request timeout; 900-second outer timeout per suite; case caps
14/14/10/8/12/18. Each copied workspace uses its own Cargo target. Each provider's
baseline finished before its candidate began; different providers could overlap.
No suite timed out and no provider retry occurred. No criteria were relaxed.
A revised candidate is evaluated only after fixing the stale-cause bug exposed
by the first candidate below. Published receipts exclude raw
transcripts, credentials, and local machine paths.

## First candidate results and discovered failure

| Provider | Baseline | First candidate |
|---|---:|---:|
| Mistral Small | 5/6 | 4/6 |
| GPT-4.1 mini | 4/6 | 5/6 |

Both first-candidate runs pass exact file creation, Rust repair, damaged-session repair,
and the final-report follow-up task. Both still fail TUI replay. The benchmark
execution-proof resume case changes in opposite directions: GPT-4.1 mini passes
in the candidate, while Mistral reaches its eight-iteration cap without the
required final report, despite satisfying the execution and artifact checks.
Mistral's retained recovery snapshots in that case are clear; the new localized
failure gate was not activated there.

These are one comparison per provider, with different generated trajectories.
The totals establish neither overall improvement nor nonregression. Today's
baseline also differs from the preceding day's run of the same executable.
There is no speed or general reliability claim.

The TUI replay candidates both activate the new localized recovery state:

- GPT-4.1 mini introduces an unsupported `replay_spec_includes` check. The runtime
  retains the replay path and schema error, and the model reads the exact target.
  A later patch removes the unsupported variant, but the same check now fails
  because the case lacks a failure-like assistant target. The new error has no
  file path, exposing a bug in this candidate: focus metadata and hints keep the
  old schema error even though the current tool result reports a different cause.
  No passing automatic test or explicit verification is recorded.
- Mistral introduces trailing JSON characters. The runtime retains the replay
  path, parse location, and original check. The model reads that target, but does
  not produce a verified repair before the iteration cap. The final retained
  focus is still `observed`.

These traces show initial localization and reread behavior, not successful recovery.
The first candidate is superseded because retaining the original check must not
present an obsolete diagnostic as current. Its executable and receipts remain
unchanged so this failure is not erased by the correction.
Snapshot counts describe retained session messages, not every transient state or
an independent completion verdict. Full read → repair → original-check success,
including unrelated diagnostics, resume, and pruning, is established by the
deterministic regressions below rather than by these failing live replay runs.

## Revised candidate results

| Provider | Same frozen baseline | Revised candidate |
|---|---:|---:|
| Mistral Small | 5/6 | 4/6 |
| GPT-4.1 mini | 4/6 | 5/6 |

GPT-4.1 mini passes every case except TUI replay. Mistral now passes benchmark
execution-proof resume, but fails damaged-session repair instead: it never
executes the required patch, leaves `src/lib.rs` unchanged, and has no passing
automatic test. Equal totals across the two candidates conceal different failed
cases and do not establish nonregression. No suite times out or retries a
provider request.

The damaged-session trace is blocked before any tool executes. The model writes
three criteria under `Acceptance Criteria:`, while the existing plan parser
recognizes `acceptance:` and `acceptance_criteria:`, but not the spaced heading.
The Plan Gate therefore keeps reporting
missing criteria until the ten-iteration cap. No recovery-focus snapshot is
created. This is a concrete earlier protocol-formatting gap, not evidence that
localized recovery was exercised or successfully repaired that task.

The revised GPT-4.1 mini replay trace exercises the correction live. Its cause
progresses from trailing JSON characters to unsupported `file_inclusion` and
`replay_success` check variants, then to missing failure-like assistant target
data. At that pathless failure, retained state changes from `observed/current`
to `unlocalized/last_confirmed`, with the new diagnostic instead of the obsolete
schema error. A later failed automatic replay refreshes the cause again. This
establishes live diagnostic refresh and location downgrade, not replay success.

Mistral's revised replay remains at a JSON comma/bracket error despite target
rereads. Neither provider records a passing automatic test, the two required
explicit verification commands, or a completed TUI replay task. The same check
must still pass; the new fallback does not waive verification.

## Deterministic validation

`bash scripts/check-local.sh` passes with reduced debug information and
incremental compilation disabled: 631 Rust unit tests (1 ignored), 1 integration
test, TUI replay 3/3, Python 12, Web 53, repo-map 14/14, JavaScript/shell syntax,
and Lite asset/status/git smoke.

The 20 new tests cover localized failure lifecycle, unrelated successful tools,
exact target reads, raw mutation-cache replacement, unavailable targets, repair
and reread transitions, original-check identity across changed configuration,
workspace launch directory, forged status/cwd text, completion gates, session
round trips, and pruning. Diagnostic regressions exercise long workspace paths,
Rust source locations, Python traceback frames, bounded UTF-8 display, and actual
automatic-test command execution. Existing HTTP mock coverage now also verifies
that local recovery metadata is omitted from provider payloads.

The stale-cause correction adds schema repair followed by a fresh pathless
semantic failure, original-check exec failure refresh, and legacy snapshot
migration through subsequent reads and shell actions. Location evidence stays
separate from observation state: reading the previous path cannot make it the
confirmed location of a new failure. Legacy refresh requires matching configured
check identity and retained correlated failure evidence; identity is not guessed.

Independent implementation and documentation review found no remaining P1/P2
findings after fixes. This is not live qualification.

## Remaining scope

Localization applies to automatic tests after successful file-tool edits when
diagnostics identify the edited file, or its JSON parse position is confirmed
locally within a bounded read. Other failures retain generic recovery. This does
not infer an arbitrary different failing file. Diagnostic display scanning is
bounded; process output capture is unchanged.

The next live bottlenecks are ineffective repairs despite a retained cause,
plan-heading recognition before damaged-session repair, and the first candidate's completion/final-report
stall after valid execution proof. CLI auto-review/TUI
automatic handoff task boundaries and read-only/Web final-handoff parity remain
separate work. The failed live checks remain enforced and visible in the receipts.
