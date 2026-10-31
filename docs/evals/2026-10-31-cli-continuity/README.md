# CLI repair with preserved edit diagnosis

The GPT Coder + Observer pair **strictly qualified on `pilot-cli`**, using 17
requests (13 Coder / 4 Observer). It reproduced the original failure, repaired
the source and documentation, reran all seven original tests after the last
edit, and submitted an accepted authored report. The original frozen assessor
for this wave reports **1/1**, with no post-run correction or retry.
[Results](results.json) · [Candidate patch](pilot-cli.patch)

| Evidence | r11 | r12 |
| --- | --- | --- |
| Requests | 18 Coder / 6 Observer | 13 Coder / 4 Observer |
| Changed files | none | requested source and documentation only |
| Original independent tests | 7 run, 1 failure | 7/7 pass |
| Native finish | truthful blocked report | accepted completed report |
| Strict qualification | failed | qualified |

The task text, complete case inputs, references and budget match
[r11](../2026-10-31-repo-maintenance/README.md). Only `pilot-cli` was selected.
Both roles use `gpt-5.6-luna`, with 24 task requests, six reserved for Observer,
four possible transport repairs and an eight-request Coder slice. The existing
model settings, 90-second request timeout and 600-second wall limit are unchanged.
The r11 report-assessor correction was already present before this wave's freeze.

## Runtime repair and its direct evidence

`EditFailureMemory` now reconstructs explicit diagnosis provenance from native
correlated tool results. A successful read of the latest failed edit's target
can restore `Fix` despite a generic `Diagnose` state inherited from an older
failed check. The check remains pending and must still pass after the edit.

Later actual failures supersede that diagnosis. Reading the older source cannot
clear a newer failed check, unrelated failed read or another failed edit.
Refusals do not count as executed failures. A failed reread of the same target
requires a fresh successful read, and cannot revive diagnosis already superseded
by a different failure. The pending focused check can still require its own read.

Nine native regressions cover continuation and JSON roundtrips, current
multi-failure ownership, full/ranged reads, stale or unmatched evidence,
refusals, actual history pruning and later failures without failed-check memory.
The historical helper was adapted to require the original check to remain
active **or deferred**, with the newer unrelated check explicitly active.
The old external probes and their r11 failure results remain unchanged.

The exact [r11 native-session probe](../2026-10-31-repo-maintenance/recovery-probe.rs)
also passes on the new source. All three actual post-failure reads restore `fix`
while retaining the failed check, where the old runtime restored `diagnose`.
[Native prefix log](native-prefix-probe.log) · [Validation receipts](validation.json)

This is an operator-authored runtime repair informed by the existing
`recovery-edit-truth` reference, with additional provenance guards and tests.
It is not a model-generated repair of Spiral-Coder itself.

## What the live pair actually did

After initial Think/Plan refusals, the pair executed the failing baseline and
read its test location. An attempted source diff was refused for missing source
evidence. It then read the source and successfully applied the one-line case
filter. The original tests passed. The pair read and edited the documentation,
reran the exact command after that final edit, and authored its report.

The patch retains suite order, deduplicates selected IDs, runs every case by
default, and preserves unknown-ID and no-overwrite behavior. Documentation
includes the requested repeated-flag example and selection semantics. Replaying
the patch on the original fixture reproduces the entire candidate inventory and
passes all seven original tests. No supplied test or out-of-scope file changed.

Three accepted Observer suggestions have exact matching executed calls. The
fourth recommends `done`; its continuation completes, but the Coder changes the
report arguments, so the exact-argument matcher correctly records no match.
The final report itself is independently present and accepted. These are
separate receipts, not four claims of verbatim suggestion execution.

**The live episode did not contain an actually failed edit.** Its refused diff
never executed, and its two real patches both succeeded. It therefore establishes
a complete repair under the candidate runtime, not a causal estimate that this
runtime change made the pair succeed. The direct evidence for the recovery fix
is the deterministic old-session replay and regression controls above. This is
one visible practice task and one attempt, not a general success-rate estimate.

## Reproduction and retained evidence

- Runtime commit `b4cd6c93921bae2d45fe927b9f5eba84c78e0505` was built and checked
  before calls. The [plan](plan.json) was committed at `9770e67` before execution.
  [Preflight](preflight.json) retains the failing baseline and passing reference.
- All 17 wire receipts completed and used task allowance. There were zero
  transport repairs, reconnect attempts or report-only closeouts. Provider token
  usage is unknown for all 17 receipts. The episode lasted 92.792 seconds.
- [Review](review.json) retains native counts, both exact post-source/post-doc
  checks, refusals, handoffs, final report and patch replay. All **180** entries
  of the [raw manifest](raw-manifest.json) were verified against the local bundle,
  including hashes, sizes, modes and selected credential absence checks.
- `scripts/check-local.sh` passed 1,018 Rust unit tests (one ignored), four
  integration tests, 93 Python tests, 80 Web tests, five npm tests, 13 TUI replays,
  14 repo-map cases and the server/UI smoke. The temporary native-session probe
  was removed before rebuilding and freezing the production executable.

Run a new bounded attempt using the
[maintenance-suite instructions](../../../benchmarks/coding-v1/README.md#repository-maintenance-with-the-practical-pair-budget)
with `--case pilot-cli` at preparation. Preserve the run's original outcomes.
The malformed single-file diff boundary and noisy structural JSON edits from
r11 remain separate follow-ups. Initial plan/think formatting also still consumed
two requests in this successful episode.
