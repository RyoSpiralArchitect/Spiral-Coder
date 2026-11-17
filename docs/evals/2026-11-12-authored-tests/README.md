# Authored test requirements

S16 passed every product check for [#82](https://github.com/RyoSpiralArchitect/Spiral-Coder/issues/82).
Its two authored tests exercised a completed fixture and malformed manifest
JSON. The independent sensitivity audit detected none of the four specified
faults. The Observer's first test-writing suggestion contained a placeholder,
and later suggestions prioritized the remaining exact checks.

The runtime now links each declared test artifact to its original requirement
IDs, bounded complete test-only descriptions, and the latest recorded edit.
Both roles use this context to identify the fixture, changed condition, and
public assertion for a required case. The projection is advisory. The external
audit still measures sensitivity by executing tests against the same controls.

## Model-free replay

Three sealed S16 session prefixes cover the turn before the first test write,
its receipt, and the turn after all six exact checks passed. Both executors
projected the same original requirements, pending checks and recovery
obligations. The new executor retains the test cases even at the final prefix,
where structural obligations are empty. The replay used zero provider calls.
[Results and input hashes](results.json)

## Fresh Luna run from the original baseline

S17 was an unseeded run on the same baseline, prompt, driver, model pair and
independent checks as S16. The fixed limits were six rounds, 192 task
requests, 216 wire requests and 3,600 seconds. The baseline and reference
controls were checked before any provider calls.

| Outcome | S16 | S17 |
| --- | ---: | ---: |
| Rounds | 4 | 5 |
| Task / wire requests | 114 / 127 | 159 / 174 |
| Wall seconds | 1,039.170 | 1,617.084 |
| Current exact checks | 6 / 6 | 6 / 6 |
| Independent groups | 7 / 7 | 6 / 7 |
| Held-out cases | 24 / 24 | 23 / 24 |
| Authored-test sensitivity | 0 / 4 | 2 / 4 |
| Qualified | No | No |

In round three, Observer proposed an exact test-file read and supplied concrete
fixture and assertion requirements in the primary reason. Coder authored
four tests, all of which passed against the reference. They detected the
missing-pending and duplicate-request faults. Symlink and parent-link faults
survived. The candidate also accepted an incorrect parent in its first finalized
row, which accounts for the held-out failure. Native completion was accepted
after all six exact commands passed following the final edit.

The two fresh runs provide one sample per executor. S17 reached native
completion with stronger tests and a different implementation failure.
[Recorded outcomes](results.json)

## Test-only continuations from S16

Two bounded continuations used disposable copies of the sealed S16 candidate,
whose implementation passed all seven independent groups. Both used the same
test-only prompt, workspace path, Luna pair and 32 task / 36 wire ceilings.
The prompt used named scenarios from the original task and kept all six
exact checks. These are assisted continuations of a completed candidate.

The first run stopped before editing. Observer supplied a path-only patch
proposal with a detailed explanation at confidence 0.92. The native schema
rejected it. The repaired reply was below the forwarding threshold and the
Coder returned an incomplete report after six requests.

The follow-up runtime retains the reason and confidence of an eligible
path-only test proposal as a no-tool clarification. In a model-free replay
of the exact rejected reply, the updated executor forwarded the explanation
with empty tool arguments and the same six pending checks. The matching live
continuation also delivered a recipe. Coder attempted an edit, and the patch
tool rejected nonunique hunks. The run ended `observer_unavailable` after
seven task requests and eleven wire requests. Both resulting workspaces
matched the sealed input inventory. The sensitivity audit has no new
test artifact to evaluate from either continuation.

Final local validation passed 1,191 Rust main tests, six integration tests,
20 TUI replays and the remaining checks in `scripts/check-local.sh`.
[Replay and live receipts](results.json)

## Shared requirements and rejected Observer repair

Both roles receive complete behavioral descriptions linked to test artifacts
within the existing 4 KiB description budget. Dedicated test requirements take
priority. This keeps the required source decision and the corresponding public
assertion visible during closeout. A nonunique edit followed by a target read
also carries guidance for a uniquely anchored retry.

Three sealed session prefixes showed the shared parent-link rule at closeout
and the specific edit guidance after the target read. The same six pending
checks survived the earlier failed edit. In a second replay, both executors
received the exact malformed Observer answers from the earlier test-only run.
The new executor returned one Coder slice within its original allowance and
then requested Observer again. Both replays used zero external provider calls.
[Replay receipts](closeout-results.json)

S18 was a fresh unseeded Luna/Luna run with the same original task, limits and
oracles. It reached the six-round cap after 192 task and 210 wire requests
in 1,894.667 seconds. All 24 held-out cases passed, including first-row parent
linkage. Six of seven independent groups passed. The protected path suite
still rejected the candidate on three dangling pending-link cases. Formatting
was the last mutation, and the final report recorded all six checks as pending
after it. The authored tests detected one of the four faults. The first terminal
result remains sealed and is unqualified.
[Fresh run receipts](closeout-results.json)

### Assisted test-only continuations

Two further Luna/Luna runs edited only the test file of the sealed S16
implementation. Each retained the six exact checks and earned an accepted
native report.

| Outcome | Test authoring | Test-design repair |
| --- | ---: | ---: |
| Task / wire requests | 21 / 21 | 15 / 15 |
| Wall seconds | 173.931 | 121.965 |
| Current exact checks | 6 / 6 | 6 / 6 |
| Independent groups | 7 / 7 on readback | 7 / 7 |
| Authored-test sensitivity | 3 / 4 | 4 / 4 |

The first test suite detected missing pending data, symlink acceptance, and
incorrect parent linkage. Its duplicate-request test changed the request
count as well as the ID, so count validation rejected the input even with the
duplicate-ID fault enabled. The repair assigned an existing ID to a second
existing request while preserving counts and charges. The symlink test uses a
Unix guard and exercises ledger files. Directory-component coverage remains
in the protected path suite. Both test patches replayed to the exact final
bytes. The initial assisted assessment omitted the frozen baseline test
identity list. A readback with that list and the same candidate passed all
seven groups. The initial evaluation receipt remains in the results.
[Assisted run receipts](closeout-results.json)

The final assisted CLI read three actual archived runs with credentials
absent from the process environment. It reproduced the durable request
counts for completed S16, stopped S18, and S7 with a recorded active
iteration. Archive inventories matched before and after each call.
The runtime passed `scripts/check-local.sh`, including 1,194 Rust main tests,
six integration tests, and 20 TUI replays. Fresh #82 qualification remains
open. [Archive and validation receipts](closeout-results.json)

## Admitted plans and focused comparisons

The retained S18 guide comparison was rejected when Coder rewrote the
admitted task goal to describe the immediate diagnosis. A model-free replay
used the same Observer advice, Coder plan and tool call in both runtimes,
with `think.next: @observer` selecting the exact supplied exec. The old
runtime rejected the plan. The updated runtime preserved its original goal
and acceptance criteria, executed the comparison and captured both assertion
operands. It kept the original check pending and reopened two receipts under
the existing freshness rules. Changing a criterion or the execution directory
still caused rejection. A control using only a short command prefix retained
the goal but hit the existing assumption gate. [Replay receipts](observer-continuation-results.json)

S19 used that runtime in a fresh Luna/Luna run with the original prompt,
baseline, limits and independent checks. It finished in six rounds after
181 task and 196 wire requests in 1,568.927 seconds. All six exact checks
passed after the last edit, and the authored completion report was accepted.
In round four, Coder used `@observer` for the exact integration test proposed
by Observer. The correlated call executed and passed. Plan-goal restoration
was exercised in replay. [Fresh run receipts](observer-continuation-results.json)

The independent path suite passed. The held-out score was 22/24, with
failures for contradictory completion and an incorrect ledger task ID.
The authored tests detected missing pending data and incorrect parent linkage.
The other two injected faults survived. The duplicate-ID case changed the
request count as well as the ID. The candidate preserved its path scope and
the sealed patch replayed. Fresh qualification remains open.
[Independent assessment](observer-continuation-results.json)

The later grouped comparison change was tested on the sealed fifth-round
history. Before its required read and after its repair, the comparator
stayed absent. After the read, the updated runtime recognized three failed
subtests of one method, despite a space at the clipped assertion boundary.
On a disposable copy with the final one-line repair reversed, the focused
method returned exit 1 and showed recorded task/wire usage of 2/3 against
expected null values. The original path check remained pending and the
candidate inventory matched before and after the comparison. This replay
used no provider requests. The final runtime passed the local suite with
1,200 Rust main tests, six integration tests and 20 TUI replays.
[Grouped replay and validation receipts](observer-continuation-results.json)
