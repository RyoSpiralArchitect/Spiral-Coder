# PR #83 fork review

Review basis: `8e3cecf` → `df2f3bc`. A fresh read-only subagent received no parent
conversation; the project-local AGENTS.md was hidden by the fork-review skill
and restored immediately after completion. This is best-effort disk/context
separation, not a claim that host system instructions were absent. Candidate
patch archives were treated as experimental records, not deployed code.

## Fresh reviewer output

1. **[P2] Preserve known finalized spend when checking the wire cap** —
   `benchmarks/self-dogfood-v2/coupled-status/reference.rs:108–114`

   When the pending ledger is missing, `total_w` becomes `None`, so
   `total_w.is_none_or(...)` bypasses the budget check even if finalized ledgers
   already exceed `max_wire_requests`.

   Concrete input: `max_iterations=2`, per-iteration task limit `2`, total
   task/wire limits `4/4`; one finalized row with two task requests and three
   transport requests (five unique request IDs); `active_iteration="iteration-02"`
   with no pending ledger; manifest finalized totals `2/5`. The preceding checks
   accept this input, and line 114 accepts `None`, producing successful JSON
   despite the known five wires exceeding the four-wire cap.

   The prompt explicitly requires rejecting known aggregate usage above the
   caps. This also affects benchmark validity: `test_sensitivity.py:88–100` runs
   candidate-authored tests against this reference, so a correct negative test
   for this case would incorrectly fail the reference prerequisite. Check
   finalized sums against the caps before replacing total usage with unknown;
   retain null output totals for otherwise-valid missing-pending cases.

I found no additional actionable regressions in the production changes after
reviewing the full source diff and tracing budget reservation, Observer
closeout, approval barriers, continuation, and recovery ownership.

Validation was read-only source/test/receipt inspection. I did not run Cargo,
compile the reference, or execute live provider requests. The finding follows
directly from the reference’s control flow; it has not been reproduced against
a binary. Archived candidate patches were treated as experimental records,
not deployed implementation.

## Stateful annotation

Accepted. The operator reproduced the finding against the frozen old reference:
four existing tests pass and the added regression fails because an overflowing
record incorrectly succeeds. The correction checks finalized sums before
pending usage is projected to unknown. The regression establishes an at-cap
positive record, then adds one unique transport request and rejects it, for
both active and interrupted missing-ledger cases. Existing assertions, original
oracle cases and fault definitions are preserved. Historical runs are not
rerated and no candidate is adopted. The separate review validation receipt
records binary red/green and preflight/audit results.

## Fix recheck

The same reviewer rechecked `df2f3bc..1264a9d` with project-local context hidden
and restored immediately afterwards. This is a fix recheck, not a second
independent full review.

> No actionable remaining findings in the fix recheck (`df2f3bc..1264a9d`).
> The P2 is resolved.

The reviewer confirmed the finalized guard precedes unknown projection, known
pending usage still reaches the existing aggregate check, both at-cap and
overflow states are covered, and source/receipt hashes are consistent. Red and
green test hashes differ only because one is raw and one rustfmt-formatted.
The recheck was read-only source/receipt inspection, with no Cargo, provider
calls or shared-target access; raw execution logs were not independently
verified. The operator's binary runs are recorded separately in
[the validation receipt](pr83-review-validation.json).

Stateful annotation: accepted; no additional code change was needed. The
reference correction does not qualify S13 or close #82.
