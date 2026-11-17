# S24: admitted obligations resumed when an unguided plan keeps the acceptance

[S23](../2026-11-17-authored-test-closeout/README.md) ended in round 6 after
five consecutive plan-drift rejections. Ten of its twelve rejections kept the
admitted acceptance unchanged: seven reworded only the goal, and three
enumerated the same items on one line.

This branch makes an unguided revision that was rejected only as drift resume
the admitted goal and criteria. That happens only when its acceptance equals
the admitted acceptance, either verbatim or as the same items enumerated in
order on one line. The revision's own steps, risks and assumptions are kept,
and ordinary plan validation must still pass. A changed, reordered or missing
criterion is still rejected.

A sealed fixture holds the S23 admitted plan and its eleven rejected proposals.
All eleven fail ordinary validation, at least eight resume, and a reworded
acceptance stays rejected.

S24 is a fresh, unseeded Luna/Luna run with the S20–S23 inputs unchanged byte
for byte. The executor is this branch at `5e85d6c`, which is S23's executor
plus this change. The [plan](plan.json) was pushed at `f216c42` before any
provider call. Preflight passed, and both controls behaved as before.
[Controls](sensitivity-controls.json)

## First terminal

**Unqualified, not adopted; #82 remains open.**

- **Stop.** The chain used all six rounds and stopped at the iteration limit,
  after 192 task / 201 wire requests in 1,564.3 s.
- **The change was exercised live.** It fired twelve times
  (`plan_goal_retained`). Plan-gate rejections fell from twelve in S23 to one.
  Rounds 5 and 6 had no governor blocks at all.
- **Passing groups.** Held-out cases passed 24/24, along with the guide,
  visible, npm and build groups.
- **Failing.**
  - Six path subcases fail. A dangling symlink at the running or interrupted
    round's pending components is treated as missing instead of rejected.
  - The stock checks stop at rustfmt, because the last edit left formatting
    behind.
  - The only authored test (a missing run directory) detects none of the
    faults.
  - No completion claim was made, so the authored-test closeout review was not
    exercised.

| Outcome | S22 | S23 | S24 |
| --- | ---: | ---: | ---: |
| Stop | completed | no_new_edit_or_verification | iteration_limit |
| Gates passed | 25 / 26 | 19 / 26 | 18 / 26 |
| Held-out cases | 24 / 24 | 22 / 24 | 24 / 24 |
| Path subcases | 26 / 26 | 26 / 26 | 20 / 26 |
| Authored-test sensitivity | 0 / 4 | 4 / 4 | 0 / 4 |
| Plan-drift rejections | 5 | 12 | **1** |
| Governor blocks in rounds 5–6 | 4 | 9 | **0** |

All six exact checks were green except the path matrix by the end of round 4.
Rounds 5 and 6 were spent on fourteen patch-and-rerun cycles against the same
six dangling-link subcases. The fix needed was to inspect links without
following them. The pair never reached it, and nothing in the harness blocked
or redirected it.

[Assessment and evidence](results.json) · [Candidate patch](candidate.patch)

## Reading

Across S20–S24, each terminal failure that came from the harness has been
repaired with a sealed reproducer:

| Run | Harness failure | Repair |
| --- | --- | --- |
| S20 | Formatter and contract-invalid repair deadlock | #92 |
| S21 | Path misclassification | #93 |
| S22 | Thin tests accepted at closeout | #94 |
| S23 | Verbatim-acceptance drift rejections | this change |

S24 ran with almost no gate friction. Its remaining failures are
product-semantics and test-authoring work inside the fixed budget. Each half
of qualification has been met in separate runs:

- Every original gate and every independent group passed in S22.
- Full sensitivity was reached in S21 and S23.
- Held-out cases passed 24/24 in S22 and S24.

These are single samples per executor, not causal comparisons.
