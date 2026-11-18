# S30: native completion on the continuation executor, tests short again

S30 is a fresh, unseeded Luna/Luna run with the S20–S29 inputs, unchanged byte
for byte. The executor is this branch at `13a2f83`, which combines:

- main `1d2dd75` (#92–#100);
- #101's coverage-map closeout review;
- the S29 repair: a rejected report-only closeout advances to the next
  iteration under the progress rule.

The [plan](plan.json) was pushed at `05a23da` before any provider call.
Preflight passed, and both controls behaved as before.
[Controls](sensitivity-controls.json)

## First terminal

**Unqualified, not adopted; #82 remains open.** 25 of 26 gates pass.

- **Chain:** completed natively in iteration 6, using 186 task / 206 wire
  requests in 2,413.4 s. All six exact checks passed after the last edit
  (steps 151–156), and the first `done` was accepted at step 157.
- **Independent groups:** all seven pass, including held-out 24/24 and the
  path matrix.
- **Authored tests:** the four tests detect one of the four faults, missing
  pending data. They miss the symlink, duplicate request and parent link
  faults. The tests are: completed fixture, interrupted pending summary,
  unknown usage as null, and malformed manifest. This is the only failed
  gate.

## The coverage map was not exercised

The closeout review asks for the map, but it fires only while the Coder can
still afford one edit, all six declared checks and the report: 8 requests.
S30's first completion claim came at the end of iteration 6 with 5 Coder
requests left, so `done` proceeded unreviewed.

The test file was first written much earlier, at step 123 in iteration 5,
with a whole iteration still ahead.

Iteration 1 again ended with the reserved last-turn report. This time the
blocked report was accepted, and the chain advanced as before.

| Outcome | S22 | S28 | S30 |
| --- | ---: | ---: | ---: |
| Stop | completed | completed | completed |
| Task / wire requests | 166 / 182 | 130 / 141 | 186 / 206 |
| Gates passed | 25 / 26 | 25 / 26 | 25 / 26 |
| Independent groups | 7 / 7 | 7 / 7 | 7 / 7 |
| Authored-test sensitivity | 0 / 4 | 3 / 4 | 1 / 4 |
| Closeout review | not issued | issued (step 96) | not issued (5 left) |

[Assessment and evidence](results.json) · [Candidate patch](candidate.patch)

## S20–S30

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

## Reading

Three of the last nine runs (S22, S28, S30) completed natively and passed
every gate except authored-test sensitivity. The implementation side is now
reliable. What remains is that the tests do not reach the fault families the
task names, and the review that asks for coverage can arrive too late to act
on. Asking for the coverage map when the test file is first authored would
give the Coder room to act on it.
