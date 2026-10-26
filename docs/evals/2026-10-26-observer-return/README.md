# Return failed verification to Observer (2026-10-26)

This pass keeps Coder + Observer as one repair system. It compares a freshly
executed, cache-corrected baseline with a reviewed runtime that returns new
verification failures to Observer and narrowly recovers missing JSON openers.
The [plan](plan.json) was committed before calls: two unchanged role pairs,
two waves, at most four episodes and 128 provider requests. Each episode has
a 32-request cap and a 1,200-second wall limit. There is no in-place retry or
automatic adoption of model-generated changes.

## Frozen comparison

The task is the protected visible regression from PR #38, with baseline source
`9982a68a6978058c75a9a700e7407df5a49f010c`, the same prompt, editable scope,
reference repair and six independent controls. The driver resets the package's
Cargo artifacts under a cache lock before baseline, reference, live and candidate
assessment execution. Dependencies remain cached. PR #38's earlier episodes
with recorded cache ambiguity are excluded from this comparison.

The old executor is `5b9315dbfb4fbfc134b5a98ae59e658837f94d6d` (its Rust source
matches the PR #38 merge); the reviewed executor is
`5be9d51f54520d5e8ff57a018247d734a2703859`. The latter's binary SHA-256 is
`b74db0f8649898fffb76f80686f56ace4d5e62c74c027bd5e51d12991e811b9c`.
The supplied chunk limit remains eight, but the new executor applies it on
every continuation and yields earlier on a fresh verification failure. These
control changes and JSON recovery are one treatment, not isolated ablations.

## Outcomes

The sealed four-episode comparison is recorded in [results.json](results.json).

| Wave / Coder → Observer | Requests (Coder + Observer) | Visible test | Independent controls | Original Rust suite | Qualified |
| --- | --- | --- | --- | --- | --- |
| Baseline / Luna 5.6 → Argon | 32 (31 + 1) | 0/1 | 5/6 | 864 pass | No |
| Baseline / Argon → Large 4 | 10 (8 + 2) | 0/1 | 5/6 | 864 pass | No |
| Reviewed / Luna 5.6 → Argon | 32 (26 + 6) | 1/1 | 4/6 | 863 pass, 1 fail | No |
| Reviewed / Argon → Large 4 | 3 (3 + 0) | 0/1, unchanged | 5/6, unchanged | 864 pass, unchanged | Provider interrupted |

All four remain natively incomplete. **77 requests** were spent in total;
**3,743 raw file entries** matched their sealed hashes, sizes and recorded
modes. Inputs, task baseline inventories and driver hashes agree between waves
for each pair. All sixteen package-reset receipts pass. No wall/output limit
was exceeded, no secret redaction occurred and every candidate stayed within
the frozen editable scope. Successful builds and the seven frozen TUI replays
do not substitute for failed behavioral controls.

Luna's reviewed run records three explicit failure handoffs: one failed native
exec and two failed automatic tests after edits. Its first request fails the
protected regression; the next wire request is to Observer. Later handoffs
also preserve the original pending check. Six Observer requests contain five
authored responses and one HTTP 503 handled by the existing bounded provider
retry. Five assist receipts distinguish the proposed read from what the Coder
actually executed: three suggested reads executed, two did not. Diagnostic
reads are not proof of repair.

The reviewed Luna patch passes the visible regression by promoting an observed
edit to Fix even when a later failure should keep Diagnose. The original
`restoring_an_edit_diagnosis_does_not_clear_a_later_failure_or_invent_proof`
test catches that regression, as do two independent controls. Its required
documentation edit remains blocked. This is a useful negative result: returning
to Observer works, while the proposed source repair still fails qualification.

The reviewed Argon run has one completed Coder request, a request timeout and
a terminal Gemini HTTP 402 response. It stops before calling Observer or making
an edit. The external provider precondition prevents a complete second-pair
comparison; its unchanged-workspace tests are not evidence about the new
runtime's repair capability. No episode is resumed or rerun after that stop.

## JSON recovery evidence

Both baseline Large 4 responses lack the opening `{`, including its one
explicit action-contract repair. Neither new episode exercises framing recovery:
Luna's Observer output is well formed and the Argon episode stops before
Observer. Thus **new live framing-recovery evidence is unavailable**.

The [retained-response replay](retained-observer-replay.json) feeds those two
unchanged Large 4 raw strings through the reviewed binary without provider
calls. Both parse, queue the read hint and retain the exact line bounds
130–200. Raw-response hashes match the baseline receipt. This checks the
real failed output against the new parser/consumer, with a fixed replay Coder
context; it does not establish new live advice quality or execution success.

## Next failure modes to isolate

1. The coupled loop needs to distinguish the original pending exact check from
   a newer automatic-test failure. Luna's five Observer answers repeatedly
   prioritize the original protected test path while the newly failing test
   points into `edit_failure_tests.rs`. Inspect ownership and freshness of each
   diagnostic obligation before changing gates; clearing the old obligation
   indiscriminately would reproduce the candidate's regression.
2. Passing a visible reproducer can encourage an overbroad state transition.
   Keep later/unrelated-failure controls and require a repair explanation that
   accounts for those boundaries, rather than promoting on the visible test.
3. The Argon / Large 4 runtime comparison needs a fresh, separately declared
   episode after provider availability returns. Preserve this interruption;
   do not overwrite it or treat it as a model-quality result.

## Runtime changes and checks

- Every funded Coder slice reserves a subsequent Observer intervention and
  Coder continuation. Short default sessions retain their authored-report retry
  allowance. No new shared budget is created.
- A newly executed failed verification or failed automatic test ends the slice
  before another Coder request. The native failure, exact pending check and
  incomplete outcome remain available to Observer. Refusals and historical
  failures cannot masquerade as new execution.
- The interactive TUI consumes a typed yield reason, independently of display
  wording and the ordinary auto-observe preference. Reflection obligations and
  exact operation identities survive continuation, including a blocked retry
  followed by another continuation.
- Missing `{` or `{"` is recovered only at the start of a known Observer
  envelope key, within 64 KiB. The whole repaired object must pass the shared
  duplicate-key parser and normal action/proof validation. No field, value,
  tool argument or closing delimiter is invented. Raw responses are retained.

The [validation record](validation.json) separates the current runtime's suite
from the frozen task: **929 Rust tests**, one server integration test, **73
Python**, **80 Web**, **5 npm**, **10 TUI replay** and **14 repo-map** cases pass
(one pre-existing Rust test is ignored). The frozen task's original regression
suite has 864 passing Rust tests and seven replay cases. Mocked-provider tests
exercise actual failed verification, Observer return, malformed-envelope
recovery, interactive selection with auto-observe off, and invalid/valid
reflection over two continuations.

Fork-review found and led to fixes for the interactive selector, reflection
restoration and blocked-retry restoration. The corrective recheck found no
actionable issues. CI also exposed a test-only npm signal-readiness race; the
fake child now registers its handler before publishing its readiness receipt.
No production launcher behavior was changed.

## Limits

These are adaptive development episodes on one known defect, not a held-out
benchmark, reliability estimate or model ranking. Native completion, actual
edits, successful diagnostics, independent repair correctness and adoption are
separate gates. Candidate patches retained beside this report are evidence;
they are not applied to the product source. This PR does not publish npm or
merge itself.
