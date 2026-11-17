# S28: the merged runtime completes, one authored-test fault short

S28 is a fresh, unseeded Luna/Luna run with the S20–S27 inputs, unchanged byte
for byte. The executor is merged main `41352c3`, which contains #92–#98. Its
binary is byte-identical to S27's, so S28 is a second sample of the S27
condition, now from main. The [plan](plan.json) was pushed at `fdd244c` before
any provider call. Preflight passed, and both controls behaved as before.
[Controls](sensitivity-controls.json)

## First terminal

**Unqualified, not adopted; #82 remains open.** 25 of 26 gates pass.

- **Chain:** completed natively in iteration 5, using 130 task / 141 wire
  requests in 1,518.6 s. All six exact checks passed after the last edit
  (steps 100–105), and `done` was accepted at step 106.
- **Independent groups:** all seven pass: held-out 24/24, the path matrix,
  visible, guide, npm, build and stock checks.
- **Authored tests:** the four tests detect three of the four faults: missing
  pending data, duplicate request IDs and parent linkage. They miss the symlink
  fault. This is the only failed gate.

| Outcome | S22 | S27 | S28 |
| --- | ---: | ---: | ---: |
| Stop | completed | iteration_limit | **completed** |
| Task / wire requests | 166 / 182 | 192 / 214 | **130 / 141** |
| Gates passed | 25 / 26 | 18 / 26 | **25 / 26** |
| Independent groups | 7 / 7 | 5 / 7 | **7 / 7** |
| Authored-test sensitivity | 0 / 4 | 3 / 4 | **3 / 4** |
| Governor blocks | 18 | 28 | 12 |

[Assessment and evidence](results.json) · [Candidate patch](candidate.patch)

## Closeout

1. The first `done` (step 86) was blocked because the acceptance reference
   `@required-2` had no current success.
2. After the reruns, the second `done` (step 96) received the authored-test
   closeout review (#94). This was its first live use. The review listed the
   nine requirements linked to the test file, including the symlink rejection
   in `behavior-6`, and the three current test functions. The Coder had 14
   requests left.
3. The Coder read the path check and its test file and added one test,
   `contradictory_completed_active_is_rejected`. It then reran all six exact
   checks, and `done` was accepted. The new test covers contradictory state;
   no test covers symlinks.

The verified report-only fallback from #98 was not needed.

## Iteration 3 closed early

The Observer's primary action was a `write_file` of `tests/coupled_status.rs`
with one placeholder comment as content, at confidence 0.70. A 0.99 verify
action stood behind it. Below the 0.75 threshold the advice was correctly not
forwarded. The existing low-confidence path then returned one report-only
request. The Coder wrote a blocked report, and the iteration ended after 9 of
its 32 task requests.

The run still completed with 62 task requests to spare, so this did not decide
S28. Across S20–S28 it is the only low-confidence closeout; the other
report-only requests came at the last Coder request. A separate change keeps
the Coder's slice in this situation while native work remains.

## S20–S28

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

## Reading

S28 is the closest run so far: the same 25 of 26 gates as S22, with three of
four faults detected instead of none. What still varies from sample to sample
is which fault families the authored tests reach. S27 and S28 both missed the
symlink fault, which S21 and S23 caught.
