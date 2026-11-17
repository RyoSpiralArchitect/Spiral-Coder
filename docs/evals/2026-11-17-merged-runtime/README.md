# S20: fresh coupled-status run on the merged runtime

S20 is a fresh, unseeded Luna/Luna run for
[#82](https://github.com/RyoSpiralArchitect/Spiral-Coder/issues/82). It used
the original feature-free baseline `7ad8be4`, prompt, driver, native fixture,
24-case oracle, 26-subcase path matrix, six exact checks, four-fault audit and
caps (6 rounds, 192 task / 216 wire requests, 3,600 s), the same as S16–S19.

Only the executor changed: main `d6655b6`, which adds #86 (final revision),
#87, #88 and #90 to the S19 runtime. #89 is not merged, so the executor has
no `coupled-status` command. The [plan](plan.json) was pushed at `c2cbd8b`
before any provider call.

The preflight checks behaved as expected: the baseline failed for the
required reasons and the reference passed all seven groups. The reference
tests detect all four faults. S12's retained tests, used as the negative
control, fail on the valid reference.
[Controls](sensitivity-controls.json)

## First terminal

**Unqualified, not adopted; #82 remains open.** Three rounds ran: 85 task /
95 wire requests in 502.1 s. In round 3 the chain stopped with
`observer_unavailable`, leaving 107 task requests and three rounds unused.

| Outcome | S19 | S20 |
| --- | ---: | ---: |
| Rounds | 6 | 3 |
| Task / wire requests | 181 / 196 | 85 / 95 |
| Wall seconds | 1,568.9 | 502.1 |
| Stop | completed | observer_unavailable |
| Current exact checks | 6 / 6 | 0 / 6 |
| Independent groups | 6 / 7 | 4 / 7 |
| Held-out cases | 22 / 24 | 23 / 24 |
| Path subcases | 26 / 26 | 21 / 26 |
| Authored-test sensitivity | 2 / 4 | 0 / 4 |

The build, visible, npm and guide checks pass. The remaining failures:

- **Held-out:** one case, `test_active_without_ledger`, fails. An active
  round without a ledger reports the finalized usage (2) instead of `null`.
- **Path matrix:** five subcases fail on the running round's pending
  components. Missing components should give unknown usage, and dangling
  links are accepted.
- **Stock regressions:** these stop at rustfmt for `tests/coupled_status.rs`.
- **Authored tests:** only two exist (the completed fixture's JSON and a
  missing run directory), and they detect none of the four faults.

[Assessment](results.json) · [Candidate patch](candidate.patch)

## Why the chain stopped

1. **The formatter check failed.** Round 3 wrote `tests/coupled_status.rs`,
   and `cargo fmt --all -- --check` then reported two rustfmt hunks in that
   file.
2. **The prescribed formatter was blocked.** Once the reported file has been
   read, the recovery focus prescribes `formatter_for_check(check)`, here
   `cargo fmt --all`. The Coder read the file and ran that formatter. The
   assumption ledger blocked the call because the call's goal and next step
   shared half of their keywords with the Coder's own refuted reflection,
   "The new test file was already formatter-compliant." The block message's
   evidence asked for the read that had just happened.
3. **The recheck was blocked too.** Re-running the check was then blocked by
   the recovery gate.
4. **The Observer's advice was rejected.** The Observer twice proposed that
   same formatter command, at confidence 0.99, as `kind: edit` with
   `suggested_tool: exec`. The action contract rejects exec for edits, and
   after the single repair the reply was retained for review.
5. **The chain stopped.** The work return added in #86 covers only replies
   with transport-level malformation, and both replies deserialized. So the
   scheduler stopped with `observer_unavailable`.

Earlier in the same session, `cargo fmt --all` ran successfully for a
module-ordering repair. The block therefore depended only on keyword overlap
with a reflection, not on the action itself.
[Chain-stop evidence](chain-stop.json)

## Next condition

Two bounded runtime repairs follow. For each, the sealed S20 strings serve as
a failing-before / passing-after reproducer:

1. **Formatter repairs:** a formatter write command that `formatter_for_check`
   prescribes acts on the refutation. It depends on no premise, so the
   assumption ledger must not treat it as reuse of a refuted assumption.
2. **Contract-invalid Observer repairs:** a completed Observer repair whose
   envelope parses but fails the action contract gets the same single work
   return as a malformed one. The conditions stay the same: a Work turn,
   outstanding native obligations, no approval or preservation barrier, and
   at most one per iteration. No Observer action is adopted.

The oracle, fixtures, checks, sensitivity faults and caps stay frozen. The
next fresh run will be a combined condition, not a matched causal comparison.
