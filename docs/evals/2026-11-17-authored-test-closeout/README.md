# S23: one authored-test closeout review before completion

[S22](../2026-11-17-assertion-classification/README.md) completed natively.
Every original gate and all seven independent groups passed, but its two
authored tests exercised none of the four audited faults. The original task
links `tests/coupled_status.rs` to eight requirements. Once the exact checks
passed, `done` was admissible, and it was accepted with 26 task requests
unused.

This branch adds one `[Authored-test closeout]` review that fires before the
first completion claim of a task, but only after that claim has passed every
other gate. The review lists each requirement linked to an authored test
beside the test functions currently in the file. Its limits:

- **Budget floor.** It applies only if the remaining budget covers the declared
  checks plus 2.
- **No recovery stage.** It does not move the run into recovery.
- **One state.** The marker in the task's tool results is its only state, so a
  second `done` proceeds.
- **Adds nothing.** It adds no pass condition and no completion evidence.

S23 is a fresh, unseeded Luna/Luna run on the S20–S22 inputs, unchanged byte
for byte. The executor is this branch at `ad54c03`. The [plan](plan.json) was
pushed at `b420617` before any provider call. Preflight passed, and both
controls behaved as before. [Controls](sensitivity-controls.json)

## First terminal

**Unqualified, not adopted; #82 remains open.**

- **Stop.** The chain stopped with `no_new_edit_or_verification` in round 6,
  after 192 task / 209 wire requests in 1,567.6 s.
- **Closeout review.** It was never issued, because no completion claim was
  made. The condition under test was not exercised.
- **Authored tests.** Independently of the review, the Coder wrote six of them,
  and they detect all four faults.
- **Remaining failures.**
  - The guide example does not match `expected('completed')`.
  - Two held-out cases fail: budget overflow and completion conflict.
  - Five exact checks were pending at the end.

| Outcome | S21 | S22 | S23 |
| --- | ---: | ---: | ---: |
| Stop | iteration_limit | completed | no_new_edit_or_verification |
| Task / wire requests | 192 / 212 | 166 / 182 | 192 / 209 |
| Gates passed | 19 / 26 | 25 / 26 | 19 / 26 |
| Independent groups | 4 / 7 | 7 / 7 | 5 / 7 |
| Held-out cases | 21 / 24 | 24 / 24 | 22 / 24 |
| Path subcases | 23 / 26 | 26 / 26 | 26 / 26 |
| Authored-test sensitivity | 4 / 4 | 0 / 4 | 4 / 4 |

[Assessment and evidence](results.json) · [Candidate patch](candidate.patch)

## Where round 6 went

The information needed to fix the guide example was available. In round 5,
#86's focused unittest comparison had printed the full difference between the
documented example and `expected('completed')`.

Round 6 ended in five consecutive plan-gate rejections, each reporting
`plan drifted away from the requested task`. The chain then stopped.

Classifying every plan-drift rejection against the admitted plan quoted in each
block shows the pattern:

| Plan-drift blocks | S21 | S22 | S23 |
| --- | ---: | ---: | ---: |
| Total | 9 | 5 | 12 |
| Acceptance verbatim; only the goal reworded | 2 | 0 | **7** |
| Same acceptance items concatenated on one line | 3 | 0 | 3 |
| Acceptance actually reworded | 4 | 5 | 2 |

Ten of S23's twelve rejections carried the admitted acceptance unchanged. For
example, the admitted goal was "Finish coupled-status, required artifacts,
strict validation, and all exact checks." A rejected plan said "Finish
coupled-status strict validation, required artifacts, and exact verification."
over the same three acceptance lines.

#86 already preserves the admitted goal for an Observer-guided call when the
acceptance criteria match. Unguided Coder turns still receive a rejection.

## Reading

Across S21–S23, the two halves of qualification were each reached in separate
runs. The authored tests reached full sensitivity in S21 and S23. Every
original gate and every independent group passed in S22. The closeout review
remains unexercised.

S23's terminal loss came from the plan gate rejecting plans whose acceptance
matched the admitted plan exactly. The next bounded repair is to preserve the
admitted goal, and to accept the admitted items when they are written on one
line, whenever the acceptance matches. Reworded acceptance would still be
rejected.
