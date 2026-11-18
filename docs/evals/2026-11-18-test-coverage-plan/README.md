# S31: the coverage plan fires once, the run hits the iteration limit

S31 is a fresh, unseeded Luna/Luna run with the S20–S30 inputs, unchanged byte
for byte. The executor is this branch at `000959d`. It is #102's head plus a
one-time `[Test coverage plan]`, which appears once the linked test file has
been written.

The [plan](plan.json) was pushed at `1320c65` before any provider call.
Preflight passed, and both controls behaved as before.
[Controls](sensitivity-controls.json)

## First terminal

**Unqualified, not adopted; #82 remains open.** 18 of 26 gates pass.

- **Chain:** stopped at the iteration limit, using 192 task / 213 wire requests
  in 2,365.6 s. Every iteration ended with an accepted blocked report. The final
  report left four of the six exact checks unexecuted.
- **Independent groups:** 5 of 7 pass. Two fail:
  - held-out 23/24: a wrong task ID was accepted (`test_wrong_task_id`);
  - the stock checks: existing text in `docs/state-schema.md` was changed, where
    the task asks for an append.

  The path matrix, visible, guide, npm and build checks pass.
- **Authored tests:** one test function, which detects none of the four faults.

## The coverage plan fired once, late

Iterations 2–5 changed only `src/main.rs` and `src/coupled_status.rs`. The test
file and both documentation files were first written in iteration 6.

1. The test file was written at step 152, with 12 Coder requests left in the
   iteration. The automatic test passed, so the result carried the
   `[Test coverage plan]`. The plan listed the nine linked requirements in full
   beside the one current test function. This was the plan's first live use.
2. Both documentation files and four exact checks were still outstanding. The
   Coder wrote the docs, fixed formatting, reran one check and wrote a blocked
   report. It added no tests.
3. No completion claim passed the other gates. Neither the completion review nor
   the coverage-map return fired.

S31 therefore does not yet show whether the plan improves coverage.

## Requests lost to the Recovery Gate

The run had 28 governor blocks. 12 of them were Recovery Gate blocks at
`stage=diagnose`, and 9 of those 12 followed two harness behaviors rather than a
workspace failure:

1. **An Evidence Gate rejection resets recovery to diagnose.** In iteration 4, a
   patch produced a compile error at line 79. The Coder then read lines 60–84,
   and the stage moved to fix. Its next patch (step 85) was rejected for an
   invalid `<evidence>` block, and that rejection reset the stage to diagnose.
   Five identical patches (steps 86–90) were then blocked as new work, until the
   Coder read lines 78–80 again. The same sequence cost one block at steps
   30–31.
2. **Reading a file that does not exist yet enters diagnose.** In iteration 6,
   the Coder read the declared test file before creating it (step 142). The "No
   such file" error entered diagnose, and three writes creating the file (steps
   143–145) were blocked. The file was written at step 152.

The other three diagnose blocks followed real failures: a failed automatic test
and an edit that did not apply.

[Assessment and evidence](results.json) · [Candidate patch](candidate.patch)

## S20–S31

| Run | Stop | Task / wire | Gates | Independent groups | Held-out | Sensitivity |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| S20 | observer_unavailable | 85 / 95 | 18 / 26 | 4 / 7 | 23 / 24 | 0 / 4 |
| S21 | iteration_limit | 192 / 212 | 19 / 26 | 4 / 7 | 21 / 24 | 4 / 4 |
| S22 | completed | 166 / 182 | 25 / 26 | 7 / 7 | 24 / 24 | 0 / 4 |
| S23 | no_new_edit_or_verification | 192 / 209 | 19 / 26 | 5 / 7 | 22 / 24 | 4 / 4 |
| S24 | iteration_limit | 192 / 201 | 18 / 26 | 5 / 7 | 24 / 24 | 0 / 4 |
| S25 | iteration_limit | 192 / 213 | 18 / 26 | 5 / 7 | 22 / 24 | 0 / 4 |
| S26 | observer_unavailable | 168 / 183 | 20 / 26 | 7 / 7 | 24 / 24 | 1 / 4 |
| S27 | iteration_limit | 192 / 214 | 18 / 26 | 5 / 7 | 23 / 24 | 3 / 4 |
| S28 | completed | 130 / 141 | 25 / 26 | 7 / 7 | 24 / 24 | 3 / 4 |
| S29 | report_incomplete | 31 / 34 | 17 / 26 | 3 / 7 | 23 / 24 | 0 / 4 |
| S30 | completed | 186 / 206 | 25 / 26 | 7 / 7 | 24 / 24 | 1 / 4 |
| S31 | iteration_limit | 192 / 213 | 18 / 26 | 5 / 7 | 23 / 24 | 0 / 4 |

## Reading

S31 spent most of its budget on the implementation and reached the test file
only in its last iteration. The plan then came with too little room to act on
it. That is a sample of model variance: S22, S28 and S30 finished the same
implementation with iterations to spare.

The run also exposes two places where the harness, not the workspace, held the
Coder back:

- an evidence-format rejection discards a diagnosis that is still current;
- a read of a declared artifact that does not exist yet is treated as a failure
  to diagnose.

Both are candidates for the next change, before another sample of the plan.
