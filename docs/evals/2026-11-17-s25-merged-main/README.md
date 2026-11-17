# S25: second sample of the merged runtime

S25 is a fresh, unseeded Luna/Luna run with the S20–S24 inputs unchanged, byte
for byte. It runs on main `8b448de`, which contains #92–#95. The executor binary
is byte-identical to S24's (`90eb3c3a…`), so S25 is a second sample of the S24
condition, taken from main. The [plan](plan.json) was pushed at `55cc2ce`
before any provider call. Preflight passed, and both controls behaved as before.
[Controls](sensitivity-controls.json)

## First terminal

**Unqualified, not adopted; #82 remains open.**

- **Run.** Six rounds stopped at the iteration limit after 192 task / 213 wire
  requests in 1,593.7 s.
- **Passing groups.** The path matrix (26/26), guide, npm, build and stock
  groups pass.
- **Failing.** The visible check and two held-out cases (`running`,
  `wrong_task_id`) fail. The running state's summary still differs from the
  fixture.
- **Authored tests.** The two tests detect none of the four faults.
- **Unexercised changes.** No completion claim was made, so the closeout review
  was not exercised. The plan-retention change did not fire either, because no
  proposal reworded only the goal.

**Gate friction.** S25 had 36 governor blocks, against 11 in S24. 27 were
`[Recovery Gate]`: 25 at `stage=diagnose` and 2 at `stage=verify`. In 22 of
these, during rounds 5–6, the Coder patched or re-ran directly after a failed
check without the diagnostic observation the stage requires. The rest were
3 plan, 2 progress, 2 evidence, 1 assumption-ledger and 1 blocked-report block.

## S20–S25

| Run | Stop | Task / wire | Gates | Independent groups | Held-out | Sensitivity | Blocks |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| S20 | observer_unavailable | 85 / 95 | 18 / 26 | 4 / 7 | 23 / 24 | 0 / 4 | 15 |
| S21 | iteration_limit | 192 / 212 | 19 / 26 | 4 / 7 | 21 / 24 | 4 / 4 | 31 |
| S22 | completed | 166 / 182 | 25 / 26 | 7 / 7 | 24 / 24 | 0 / 4 | 18 |
| S23 | no_new_edit_or_verification | 192 / 209 | 19 / 26 | 5 / 7 | 22 / 24 | 4 / 4 | 18 |
| S24 | iteration_limit | 192 / 201 | 18 / 26 | 5 / 7 | 24 / 24 | 0 / 4 | 11 |
| S25 | iteration_limit | 192 / 213 | 18 / 26 | 5 / 7 | 22 / 24 | 0 / 4 | 36 |

Each terminal harness defect found in S20–S23 now has a merged repair with a
sealed reproducer (#92–#95). The S24 and S25 samples come from the same
executor bytes, yet they differ in where the budget went:

- **S24** spent its last rounds on dangling-link semantics, with almost no gate
  friction.
- **S25** spent its last rounds on the running summary, under repeated
  recovery-gate diagnose blocks.

Every component of qualification has been met in some run, but never all of
them in one run within six rounds of 32 task requests:

- every original gate and every independent group (S22)
- 24/24 held-out cases (S22, S24)
- full sensitivity (S21, S23)

[Assessment and series evidence](results.json) · [Candidate patch](candidate.patch)
