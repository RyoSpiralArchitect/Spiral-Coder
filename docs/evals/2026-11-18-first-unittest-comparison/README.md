# Compare a clipped assertion after the first diagnostic read

The [previous run](../2026-11-18-test-source-readback/) reached the final
path check with five successful exact checks. Cargo warnings in that check's
output contributed an unrelated Rust location alongside the Python traceback.
The native recovery snapshot had no pending location at that point. The
Coder later read and edited the source while the assertion output remained
clipped.

The updated runtime excludes the location immediately attached to a Rust
warning. When the failing unittest method is identifiable and its assertion
difference is clipped, the completed diagnostic read can immediately yield
to the Observer with the focused comparison arguments.

## Matched replay

Both runtimes reran the exact path check on the same workspace immediately
before native tool step 151. The reconstructed source matches the two
consecutive native line-range reads. Scripted local provider responses
exercised the same Coder and Observer entry points.

| Observation | Previous runtime | Updated runtime |
| --- | --- | --- |
| Native failure location | Unknown | `scripts/status_paths.py` |
| Next diagnostic action | Source read | Read the assertion |
| Focused comparison | Absent | Executed after the first diagnostic read |

The comparison sampled the first failing subtest in the selected method.
Its actual `pending` value was null. Its expected value retained the
recorded directory and used null for the unknown task ID and counts. Both
replays preserved the workspace inventory, and the original path check
remained due. [Replay receipt](prompt-replay.json)

The [live plan](plan.json) pins one Luna/Luna continuation from the
reconstructed checkpoint with the original per-iteration request limits.

## Live continuations

In the first continuation, the Observer offered the comparison after the
first diagnostic read. The Coder's three attempts hit the assumption ledger
because their wording overlapped refuted premises. It later repaired the
source and passed the path check. The launcher's temporary bytecode setting
also caused two Rust tests that inspect the external Python cache to fail.
The independent evaluator used the original environment, and both tests
passed there.

The next runtime treats the exact comparison derived from the current
failure and read as evidence gathering for the assumption ledger. The
[matched replay](assumption-replay.json) reuses the rejected live response.
The updated runtime executes the comparison and records the difference. The
[confirmation plan](confirmation-plan.json) uses the original execution
environment with the dedicated external Python cache.

| Result | First continuation | Confirmation |
| --- | ---: | ---: |
| Task / wire requests | 32 / 36 | 23 / 26 |
| Observer comparison executed | 0 | 1 |
| Exact native checks current after the last edit | 3 / 6 | 6 / 6 |
| Independent groups | 7 / 7 | 7 / 7 |
| Held-out behavior | 24 / 24 | 24 / 24 |

In the confirmation the Coder reproduced the failing check, read the
assertion, ran the Observer's comparison at step 153, and repaired the
reader. The path check passed at step 156. After the state-schema edit at
step 162, the six exact checks passed at steps 163–168. The rejected report
at step 169 put extra text on the required `Before: FAILED` line.

Both continuations start from the saved task and candidate. The authored
test file has the same hash as the previous measured 1/4 fault audit.
The remaining audit cases include symlinks, duplicate request IDs and
parent linkage. [Sealed run and check summaries](continuation-results.json)
