# First same-model replay catalog episode

One `gpt-5.6-luna` Coder and Observer pair attempted to add a read-only TUI replay catalog to the real repository. The candidate did not qualify. It registered a CLI variant and dispatch in `src/main.rs`, then exhausted the task budget with the called function still missing. No candidate changes were adopted. [Issue #49](https://github.com/RyoSpiralArchitect/Spiral-Coder/issues/49) · [frozen plan](plan.json) · [results](results.json) · [candidate patch](candidate.patch)

| Evidence | Observed |
| --- | --- |
| Live episode | 32 completed requests, 23 Coder and 9 Observer. All charged to the shared task budget, zero transport repairs, 302.419 s runner wall time. Token usage was unavailable. |
| Native reproduction | The protected visible test failed before the first edit with the expected missing-command diagnostic. |
| Changes | Six changed edit receipts, all in `src/main.rs`. One other patch executed with a missing anchor. A refused edit made no change. |
| Observer handoffs | Seven accepted suggestions, all reads. Six had an executed call with the suggested arguments. The native suggestion receipts do not record a success value for those reads. |
| Native closeout | `budget_exhausted`, no accepted or attempted `done`, no authored final report. Both required native checks remained pending. |
| Independent verification | Cargo exited 101 with E0425 at `src/main.rs:641` because `tui_replay::list` did not exist. The visible test also encountered the build error, and the full check stopped at compilation. |
| Scope and replay | Only `src/main.rs` changed. Both the assembled baseline and the original Git snapshot reproduced the full candidate file inventory from the sealed patch. The required npm launcher and documentation edits were absent. [Plain-source replay](plain-replay.json) |

## What the pair encountered

The first edit registered a variant whose argument type had not yet been defined. While adding that type, a patch placed it between the original parser attributes and `struct Cli`. Later edits duplicated the type name, then recovered the original CLI declaration. One diff applied only the first of two hunks. Its receipt records a real file change and a failed automatic test. The last edit added a match arm that called an unimplemented function.

One request proposed four inspection calls together. The runtime refused all four, including reads of the replay implementation and npm launcher. Neither file was subsequently read through an executed native file call. The Observer repeatedly redirected the Coder to ranges of the CLI file during these compiler failures. The second role was active, but the pair did not reach the listing behavior, package routing or documentation in this episode.

## Controls and evidence

The executor and candidate were pinned separately to merged commit `55351155d73675605e478e0518cc09279ae02b80`. One visible test script was added to the candidate baseline. The reference and independent checks stayed outside it. The task and driver were committed before the run, including a digest binding the executor, prompt, preflight and frozen driver. The installed `spiral` has its own routing table, so the contract includes its launcher and an actual local package entry-point check.

On the frozen baseline one visible test, eleven held-out catalog tests and one package-entry test failed for the declared reasons. The operator reference passed all thirteen. The unchanged stock tests passed on both baseline and reference, including the original 1,030 Rust unit passes, two ignored unit tests and four integration passes. The verifier also checked Python, Web, npm, the 13 stock replays, repo-map and server/UI smoke in disposable copies. [Preflight](preflight.json) · [task and probes](../../../benchmarks/self-dogfood-v2/replay-catalog/README.md)

Three candidate checks needed a successfully built binary. They did not execute. The frozen driver marked those three rows `timed_out: true`, with no exit code, no log and zero elapsed time. The updated driver records `executed: false` and `skip_reason: candidate_binary_unavailable` in that situation. The original assessment and its failing gates are retained. [Selected raw artifact digests](receipts.json)

The local raw roots are recorded by basename in the plan. The live episode is under `~/.cache/spiral-coder/replay-catalog-native-r1-20261101`. Its pinned run receipt binds the candidate and the native role ledger, session, report and trace. The earlier two preparations were model-free checks while designing the package and native verification paths.

## Next experiment

After sealing the episode, an operator replayed the **reference** in four stages on another copy. Adding the leaf implementation, registering its module and sharing selection, wiring the native CLI, then updating the npm launcher and docs each produced a successful build. Ordinary replay passed after the shared-selection change, the visible check passed after CLI registration and the package-entry test passed at the end. These seven offline checks establish a feasible edit order for the reference. The first diagnostic invocation stopped during cache setup because its receipt path belonged to a different output directory. The corrected invocation used a fresh directory and made zero provider requests. [Script](reference_order.py) · [Results](reference-order.json)

The next question is whether the pair can add the listing implementation and share selection code before exposing a CLI command that refers to it. A separately declared comparison can keep task, model pair, budget, protected inputs and qualification checks fixed, and change the order of work supplied to the pair. The current episode remains the first observation.

The [second episode](order-r2/README.md) ran that comparison. Its first changed edit added the catalog module and it reported a blocked outcome with the remaining work. The independent build still failed.

The [third episode and retained-session replay](order-r3-routing/README.md) use a corrected executor. Both saved verification proposals kept their requested command. The live candidate remained blocked on a missing CLI type, with a separately reproduced patch-format failure.

The [fourth episode](reuse-r4/README.md) added an optional helper reuse paragraph to the shared task. Its new module called three existing functions, but its one-line registration failed on the old file tool. Both the proposed edit and its replay on the corrected tool are retained.
