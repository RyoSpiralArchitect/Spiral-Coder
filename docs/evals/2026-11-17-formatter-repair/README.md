# S21: prescribed formatter repair and contract-invalid Observer repair

[S20](../2026-11-17-merged-runtime/README.md) stopped in round 3 with 107 task
requests unused, because of two runtime defects:

1. **Prescribed formatter blocked.** The assumption ledger blocked the
   formatter that recovery focus prescribes after a failed rustfmt check
   (`cargo fmt --all`). The only reason was keyword overlap with the Coder's
   own refuted reflection.
2. **Contract-invalid Observer replies stopped the chain.** Two Observer
   replies proposed that formatter as `kind: edit` with
   `suggested_tool: exec`. Both parsed, but failed the action contract. The
   work return covered only malformed envelopes, so the chain ended.

This branch changes the runtime in two places:

- **Formatter exception.** The exact write-mode formatter that
  `formatter_for_check` prescribes (`cargo fmt` or `cargo fmt --all`) is no
  longer treated as reuse of a refuted assumption.
- **Work return.** A completed Observer repair whose envelope parses but fails
  the native action contract now gets the same single work return as a
  malformed one, under the same conditions:
  - Work turn
  - outstanding native obligations
  - no approval or preservation barrier
  - at most one per iteration

  It is not charged as transport, and no Observer action is adopted.

Both regressions use the sealed S20 strings and fail before the change. Their
neighbouring controls keep chained, extended and script formatters behind the
keyword check and preserve the malformed-repair return.

S21 is a fresh, unseeded Luna/Luna run with S20's inputs, unchanged byte for
byte. The executor is this branch at `4faa408`. The [plan](plan.json) was
pushed at `2232805` before any provider call. Preflight passed, and the
controls behaved as in S20: the reference tests detect all four faults, and
S12's retained tests fail. [Controls](sensitivity-controls.json)

## First terminal

**Unqualified, not adopted; #82 remains open.** The chain used all six rounds
and stopped at the iteration limit: 192 task / 212 wire requests in
1,344.5 s. No round stopped early.

| Outcome | S20 | S21 |
| --- | ---: | ---: |
| Rounds | 3 | 6 |
| Task / wire requests | 85 / 95 | 192 / 212 |
| Wall seconds | 502.1 | 1,344.5 |
| Stop | observer_unavailable | iteration_limit |
| Gates passed | 18 / 26 | 19 / 26 |
| Independent groups | 4 / 7 | 4 / 7 |
| Held-out cases | 23 / 24 | 21 / 24 |
| Path subcases | 21 / 26 | 23 / 26 |
| Stock regressions | fail (rustfmt) | pass |
| Authored-test sensitivity | 0 / 4 | **4 / 4** |

**Sensitivity.** S21 is the first fresh #82 run whose authored tests detect all
four injected faults: missing pending data, symlink acceptance, duplicate
request IDs and incorrect parent linkage. The best earlier fresh results were
2 / 4, in S17 and S19.

**Repairs in practice.**

- The prescribed formatter ran twice, in rounds 3 and 5, and succeeded both
  times.
- The assumption ledger blocked nothing during the run.
- The stock rustfmt failure from S20 is gone.
- No contract-invalid Observer repair occurred, so the new work return was not
  exercised live; its evidence remains the model-free regression.
- The remaining governor blocks were progress (9), recovery (9), plan (9) and
  evidence (2).

**Remaining failures.** The build, visible, npm and stock checks pass. These
fail:

- **Held-out (3).** `test_active_without_ledger` reports finalized usage
  instead of `null`. `test_completion_conflict` and
  `test_interrupted_total_conflict` exit 0 on contradictory records.
- **Path matrix (3).** The three missing running-round components report
  usage instead of unknown.
- **Guide.** The appended `docs/state-schema.md` section never names
  `coupled-status`.
- **Native.** Rounds 3–5 were spent mostly on repeated visible-check failures.
  The harness digest clipped the dictionary difference to
  `{'iterations': …[465 chars]…} != {'version': 1, …}`. The visible check
  passed late in round 5. Five exact checks were still pending when the budget
  ran out, so no accepted report was produced.

[Assessment and repair evidence](results.json) · [Candidate patch](candidate.patch)

## Reading

The two repairs removed S20's terminal stall. The pair used its whole budget,
and its tests reached full sensitivity. What remains is product semantics:
pending usage must stay unknown for a running round's missing ledger,
contradictory records must be rejected, and the guide must name the command.
Delivery is also slow, because a long assertion difference is clipped before
the Coder sees which field differs. One run per condition is a single sample;
this is not a causal comparison with S20.
