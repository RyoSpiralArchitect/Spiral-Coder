# S27: verified report-only closeout after a semantically rejected repair

[S26](../2026-11-17-recovery-diagnosis/README.md) produced a complete candidate:
all seven independent groups passed, and all six exact checks passed after the
last edit. It still stopped `observer_unavailable` before any `done`. The
Observer's repaired answer proposed `done` with a report that failed the
standalone outcome line. The verified report-only fallback admitted only
structurally rejected repairs, so the run had no way to recover.

This branch admits that fallback after a semantically rejected repair, but only
when every native obligation is current. It also stops the authored-test review
from firing inside a report-only turn: that turn offers only `done` within one
wire attempt, so the Coder could not act on a review.

S27 is a fresh, unseeded Luna/Luna run with the S20–S26 inputs, unchanged byte
for byte. The executor is this branch at `4973f7e`. The [plan](plan.json) was
pushed at `ee36afa` before any provider call. Preflight passed, and both
controls behaved as before. [Controls](sensitivity-controls.json)

## First terminal

**Unqualified, not adopted; #82 remains open.**

The chain used all six rounds and stopped at the iteration limit: 192 task /
214 wire requests in 2,412.0 s.

**Authored tests.** The Coder wrote seven. They detect three of the four
faults: missing pending data, duplicate request IDs and parent linkage. The
symlink fault was not detected.

**Independent checks.** Five of seven groups pass. The remaining failures are
one held-out case (`active_without_ledger`) and three path subcases. All four
cover the same semantics: an active round whose pending components are missing
must report unknown usage.

**Exact checks at the end.** By the end of round 5, five of the six exact
checks were current, and only the path matrix was pending. In round 6, the
path repair invalidated the others again. The budget ran out before the
`coupled_status` and path checks could be rerun.

**Changes not exercised.** No completion claim and no Observer repair occurred
at closeout. So neither the report-only change nor the authored-test review was
exercised live; their evidence remains the model-free reproducers.

## S20–S27

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

[Assessment and series evidence](results.json) · [Candidate patch](candidate.patch)

## Reading

Every terminal harness defect observed from S20 to S26 now has a repair with a
sealed reproducer: #92, #93, #94, #95, #97 and this change.

What still varies from sample to sample is product semantics and test design.
That is mainly the unknown-usage rule for an active round with missing pending
components (S20, S21, S27), plus how many fault families the authored tests
reach. The final verification sweep is also costly in the fixed budget,
because six exact checks must be rerun after every last edit.
