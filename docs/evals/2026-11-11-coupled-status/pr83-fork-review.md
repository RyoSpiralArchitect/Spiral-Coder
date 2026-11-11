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
