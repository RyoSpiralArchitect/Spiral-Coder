# Expected-input-first fixture repair

The shared diagnosis gives Coder and Observer the intended fixture fields ahead of the sampled input. The guidance ties related references, records and counts to the same repair. It retains the required native read and exact recheck. Declared formatting checks can run before the remaining verification sweep when the task leaves the order open.

The workspace, protected observation runner, human task, model pair and 32-task cap match the [fixed fixture trial](../2026-11-19-fixed-fixture-expectations/README.md). The effective temperatures were 0 for Coder and 0.2 for Observer. The execution runtime changes the presentation of the fixture comparison. The same named integration test was run without a provider before the trial. The original fixture still failed, both earlier incorrect patches still showed differences against the fixed input, and the coherent control passed. [Plan](plan.json) · [Baseline](replays/baseline.json) · [Earlier control](replays/former-control.json) · [Earlier observed input](replays/former-observed.json) · [Coherent control](replays/coherent.json)

## First terminal

The first approved observation reproduced the stale pointer and empty-ledger failure. The native read triggered the first corrective Observer advice. It directed Coder to use `iteration-01`, write a valid empty ledger there, and set the pending and aggregate counts to zero. The durable assist receipt matches that proposal to Coder's first changed call at step 3. The Coder changed only the integration test. The validator and protected commands retained their starting bytes.

The first correction passed the fixture check. Formatting required a second local edit. Steps 8 through 12 then recorded five successful current checks, with the format check first. The native `done` was accepted at step 13. The first terminal completed in one iteration across 17 task and 17 total requests in 179.397 seconds. [Native receipts](results.json)

An independent replay re-created the entire terminal inventory and ran each exact check again. All five passed, including the full Rust integration suite and the separate skipped-directory check. The test patch is retained as an evaluation artifact. The broader interrupted-run accounting issue remains open. [Terminal patch](candidate.patch)
