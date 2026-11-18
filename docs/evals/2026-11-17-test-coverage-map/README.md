# S29: the coverage-map condition, stopped after one iteration

S29 is a fresh, unseeded Luna/Luna run with the S20–S28 inputs, unchanged byte
for byte. The executor is this branch at `8aa7ed8`: main `1d2dd75` (#92–#100)
plus the case-to-test coverage map in the authored-test closeout review. The
[plan](plan.json) was pushed at `1109fdf` before any provider call. Preflight
passed, and both controls behaved as before.
[Controls](sensitivity-controls.json)

## First terminal

**Unqualified, not adopted; #82 remains open.** The chain stopped after its
first iteration with `report_incomplete`, using 31 task / 34 wire requests in
394.6 s. It passed 17 of 26 gates and 3 of 7 independent groups. No authored
test file existed yet.

**The coverage map was not exercised.** No completion claim was made, so
neither the closeout review nor the coverage-map return fired.

## Why the chain stopped

1. By the end of iteration 1 the Coder had edited `src/main.rs`,
   `src/coupled_status.rs` and the npm launcher, and had reproduced the
   visible check's pre-edit failure. 31 of the 32 task requests were spent,
   including all 8 reserved for the Observer.
2. The driver gave the Coder its reserved last turn for a report. The Coder
   wrote a blocked report but claimed criterion 1 complete with
   `cargo test 2>&1` as evidence. That command was not a successful
   verification, so the `[Blocked report]` gate rejected the report.
3. A wholly rejected response is charged as transport. The ledger therefore
   still showed one Coder request.
4. An iteration ending `report_incomplete` could advance only when the ledger
   showed no Coder allowance. The chain stopped with five predeclared
   iterations unused, although the iteration had made progress.

Across S20–S28, every last-turn report had been accepted, so this path had not
come up. The follow-up change lets a rejected report-only closeout advance
under the same progress rule as an accepted blocked report. That closeout is
the iteration's final Coder turn, whatever the ledger shows.

[Assessment and evidence](results.json) · [Candidate patch](candidate.patch)

## S20–S29

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
