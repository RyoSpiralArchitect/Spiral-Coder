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
