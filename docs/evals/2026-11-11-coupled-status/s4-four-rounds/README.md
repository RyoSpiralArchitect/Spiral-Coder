# S4: four-round feature delivery, rejected report and hidden failures

S4 started from the same frozen original task, baseline, inputs and six-iteration
budget as S3, with the progressed-blocked continuation correction. It ran four
rounds in **947.206 seconds**, spending **103 task requests / 118 wires**:
78 Coder task requests + 13 transport repairs, and 25 Observer task requests +
two transport repairs. Both roles used `gpt-5.6-luna`. No operator edits or
steering occurred. [Plan](plan.json) · [Assessment](results.json)

| Round | Native termination | Task / wire | Progress |
| --- | --- | --- | --- |
| 1 | blocked | 32 / 36 | CLI/module/npm edits, missing-command reproduction |
| 2 | blocked | 28 / 32 | Parser edits; three Coder requests remained |
| 3 | blocked | 32 / 35 | Parser and both documentation artifacts |
| 4 | observer_unavailable | 11 / 15 | All four exact checks current; no accepted final report |

Round 2 exercises the new continuation rule: its incomplete report and fresh
edit advance within the declared limit despite unused Coder allowance. It does
not establish completion. Round 4 records current successful formatting, visible
behavior, guide and binary-test commands. The frozen `fresh_native_checks` gate
also requires their ordering before an accepted `done`; it remains false because
there is no such report.

Independent replay builds and passes visible behavior, the actual assembled npm
entry and the measured guide. **21/24 independent cases pass.** The failures are
completion/active-state conflict, duplicate request IDs and a symlink in the
finalized ledger path. The full stock executable checks pass (1,148 main Rust
passes, two ignored; six integration tests; 20 TUI cases; 97 Python, 80 web,
five npm and 14 repo-map cases; Lite smoke), but the stock preservation group
fails because existing `docs/state-schema.md` content changed instead of only
being appended. Scope and protected-file seals pass. **The candidate remains
unqualified and is not adopted.** [Original patch](candidate.patch)

The terminal Observer answer repeated `remaining_acceptance`; its one repair
repeated `response_contract.required`. Both are correctly rejected as duplicate
JSON keys. Because the first envelope could not be parsed, the existing
`repairing_done` classifier was false, so the all-proof report-only fallback was
not selected. The raw pair is preserved in
[the native regression fixture](../../../../tests/fixtures/observer-report-closeout/duplicate-pair.json).
The subsequent runtime fix bases this narrow fallback on current native proof,
an exhausted single repair and no approval barrier. Rejected text is never
accepted or normalized into advice. A new Coder report still must pass every
native gate. That fix is validated by replay/tests, not a reclassified S4 run.

[Context measurements](context.json) retain all four native ledgers and history.
Coder assembly projection saved 10,790,463 cumulative serialized bytes across
91 requests. These are message-assembly bytes, not billable provider tokens;
no provider usage receipts were present. Raw records remain sealed locally.

Next work: preserve admitted objectives while revising steps; turn explicit
negative-case requirements into a visible self-check checklist; retain the
append-only documentation constraint through long runs; and exercise the fixed
report fallback in a new, separately pinned fresh run. Do not relabel this
21/24 result as a passing candidate or change the frozen oracle.
