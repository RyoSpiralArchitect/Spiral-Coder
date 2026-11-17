# S22: quoted assertion output classified as logic

In S21, the protected visible check failed four times with this output:

```
AssertionError: 1 != 0 : Error: No such file or directory (os error 2)
```

The candidate had exited 1 on a missing pending ledger. The contract treats a
missing pending ledger as unknown, so this was a logic failure in the
candidate. The classifier instead matched `no such file` anywhere in the
output and prefixed the result with `⚠ PATH ERROR … verify paths with ls`.
The Coder then spent rounds 3–4 probing `CARGO_TARGET_DIR` and directory
listings, which triggered 9 progress-gate blocks in the run.

The executor change:

- A new `assertion_message` module removes Python/Node/pytest `AssertionError`
  messages and Rust assertion messages before keyword classification. Each
  message runs to the next blank line or report structure.
- A failure with no other evidence is classified as logic.
- A genuine `FileNotFoundError`, `SyntaxError`, chained exception or
  missing-command line outside an assertion keeps its class.

Both the sealed S21 shape and a real unittest run were classified `Path`
before the change. A quoted `parse error` was classified `Syntax`.

S22 is a fresh, unseeded Luna/Luna run with the S20/S21 inputs unchanged, byte
for byte. The executor is this branch at `c04cf43`: main `fb18d89` plus this
one change. The [plan](plan.json) was pushed at `48184da` before any provider
call. The preflight and both controls behaved as in S20/S21.
[Controls](sensitivity-controls.json)

## First terminal

**Native completion, but unqualified; not adopted. #82 remains open.**

- **Run:** six rounds, 166 task / 182 wire requests in 1,334.5 s.
- **Round 6:** used six task requests. All six exact checks passed after the
  last edit, and the second `done` was accepted with standalone
  `Before: FAILED` / `After: PASSED` lines.
- **Gates:** all 25 original native, report and chain gates passed, and all
  seven independent groups passed.
- **Sensitivity:** the authored tests detected none of the four faults. This
  was the only failed gate.

| Outcome | S20 | S21 | S22 |
| --- | ---: | ---: | ---: |
| Stop | observer_unavailable | iteration_limit | **completed** |
| Task / wire requests | 85 / 95 | 192 / 212 | 166 / 182 |
| Gates passed | 18 / 26 | 19 / 26 | **25 / 26** |
| Independent groups | 4 / 7 | 4 / 7 | **7 / 7** |
| Held-out cases | 23 / 24 | 21 / 24 | **24 / 24** |
| Path subcases | 21 / 26 | 23 / 26 | **26 / 26** |
| Authored-test sensitivity | 0 / 4 | 4 / 4 | 0 / 4 |
| `PATH` hints / progress blocks | 0 / 3 | 4 / 9 | **0 / 1** |

**Classifier effect.** Every failed-check hint in S22 was `LOGIC`. When the
visible check's long dictionary assertion was clipped, the focused unittest
comparison added by #86 surfaced the exact differing fields. The visible check
passed in round 4; in S21 it passed late in round 5. The guide and the
path matrix passed in rounds 5–6.

**The remaining gap.** `tests/coupled_status.rs` is linked by the original task
requirements to eight requirement IDs (`behavior-1`…`behavior-7`,
`native-tests`). It holds two tests:

- the completed fixture through the public CLI
- an unsupported manifest version

Both pass on the reference, but neither exercises missing pending data,
symlinks, duplicate request IDs or parent linkage.

The Coder's own round-5 blocked report listed exactly that work as a next step
("Confirm authored tests cover missing pending components, dangling and valid
symlinks, duplicate request IDs, wrong parent linkage…"). In round 6 the
Observer asked only for the last exact check. `done` was then admissible and
accepted with 26 task requests unused. The authored-test view is advisory and
feeds no done gate, so nothing required the linked requirements to be
exercised before completion.

[Assessment and evidence](results.json) · [Candidate patch](candidate.patch)

## Reading

S20–S22 were each one sample under a different executor. Together they show
both halves of qualification reached separately: full authored-test
sensitivity in S21, and every original gate plus every independent group in
S22. What is still missing is a closeout that keeps the original typed test
requirements in force once the exact checks pass. This is not a causal claim
about any single executor change.
