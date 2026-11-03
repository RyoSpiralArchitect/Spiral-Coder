# The end of a failed check

C1's first native npm routing check really failed. Python's unittest appended
its explanation after a multiline list diff. The native action retained only
the start of stderr, and the narrow Observer diagnostic contained no result
detail. The sealed C1 reproduction gate therefore remained unsatisfied.

On disposable copies of the C1 baseline, a loopback responder requested the
same first command using the original Coder plan. The test ran the actual npm
routing function. The old executor lost the reason from both its action and
the next Observer request. The updated executor preserved it on both surfaces.
Replacing only the launcher with the C1 pair's repaired launcher produced a
passing result and no failure reason on either surface of either executor.
The first check's native status, pre-existing source files and C1's sealed
assessment were preserved. [Measured old and new controls](controls.json).

Run these local regression tests to exercise the two output limits, a noisy
second stream, Unicode and the controls for a stale or unrelated Observer result.

```sh
cargo test --locked --bin spiral-coder failed_exec_keeps_the_trailing_reason_in_native_action_evidence
cargo test --locked --bin spiral-coder diagnostic_read_quotes_the_current_matching_failure_only
```
