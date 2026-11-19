# Observed input at the fixture boundary

The focused [issue #105](https://github.com/RyoSpiralArchitect/Spiral-Coder/issues/105) checkpoint still contained the original failing integration test. The fixture started with one finalized iteration and an interrupted `iteration-02`. The test removed the finalized row, left the pending pointer at `iteration-02`, and wrote invalid JSON to the `iteration-01` ledger. The CLI rejected the directory before reading the ledger.

The [observation runner](status_fixture_observe.py) ran that exact test in a disposable copy. The [forwarding binary](fixture-forwarder.rs) sampled the input immediately before invoking the unchanged CLI. The direct and sampled test each executed one case, returned 101 and reported `Error: invalid pending directory`. The sample preserves the generated and final manifest and ledger fields. [Calibration and sampled fields](input-observation.json)

## Matched diagnostic run

Both conditions used `gpt-5.6-luna` for Coder and Observer, the same 1,673-file starting inventory and the same [task](task.txt). Each got one iteration, 32 task requests and up to four transport repairs, with eight task requests reserved for Observer. The test was the required artifact. A product edit required a failure on coherent input. The first terminal in each condition was sealed and replayed. [Control plan](control-plan.json) · [Observation plan](observed-plan.json)

Local scripted replay executed the real check and diagnostic read. The changed harness supplied the three mismatches to both roles as structured data and preserved the single return to Observer after the read. The control could read the check's ordinary output. The replay made zero external provider requests. [Loopback receipt](loopback-delivery.json)

| First terminal | Task / wire | Coder / Observer wire | Current exact passes | Accepted done | Stop |
| --- | ---: | ---: | ---: | --- | --- |
| Prior harness | 32 / 36 | 28 / 8 | 0 / 5 | No | Budget exhausted |
| Observed input | 32 / 34 | 24 / 10 | 0 / 5 | No | Blocked |

Both episodes reproduced the failure before editing and changed only `tests/coupled_status.rs`. The validator retained its starting bytes. Both Observers proposed writing an empty ledger under the stale `iteration-02` pointer. Neither first proposal corrected the manifest pointer to the expected directory. The observed-input terminal also inserted an invalid finalized row. Independent patch replay reproduced 2 of 5 passing commands for each terminal. Formatting and the public skipped-directory check passed, while the observed runner and both Rust test commands failed. The control terminal failed at the original pending-directory check. The observed-input terminal failed earlier at finalized directory ordering. [Control terminal](control-results.json) · [Control patch](control-terminal.patch) · [Observation terminal](observed-results.json) · [Observation patch](observed-terminal.patch)

On the rebuilt CLIs, a separately generated first-interruption case returned zero task and wire requests. Moving the otherwise valid pending record and ledger to the skipped directory produced a nonzero exit, empty stdout and the expected diagnostic. The full writer-state matrix remained 18/27 in both conditions.

## Coherent input control

In a separate disposable copy, the operator preserved the empty finalized list, set the manifest pointer to `iteration-01`, retained a valid empty ledger there, and made both pending and aggregate totals zero. All five task commands passed with the validator unchanged. The control fixes the expected state for a future run. Both live episodes remain incomplete. [Control patch](coherent-control.patch) · [Check receipts](coherent-control.json)

This pair establishes transport of the concrete mismatch. The next unresolved behavior is using the expected side of the comparison as the repair target when the observed side contains a plausible stale pointer.

## Reproduce the boundary check

Restore the [prior diagnostic starting tree](../2026-11-18-fixture-diagnosis/README.md#recreate-the-observed-input) in a disposable checkout. Copy `status_fixture_observe.py` to `scripts/status_fixture_observe.py` and `fixture-forwarder.rs` to `scripts/status_fixture_forwarder.rs`. Leave the original test, validator, and generator in place. Use a separate Cargo target with locked dependencies already available offline. Reset the package build when switching from another checkout so an older test binary cannot be picked up.

Run the first command in [the task](task.txt) from that checkout. It should execute one test and exit 101 with the sampled pointer and ledger differences. Apply each terminal or control patch to a fresh copy of the same starting tree before running the five checks independently.
