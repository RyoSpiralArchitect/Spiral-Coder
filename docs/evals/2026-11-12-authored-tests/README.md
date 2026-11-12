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
