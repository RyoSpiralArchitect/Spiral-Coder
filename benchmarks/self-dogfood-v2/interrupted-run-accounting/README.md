# Interrupted runs

The native writer can persist an iteration manifest before its first work ledger, and it can record a spent request before finalizing the iteration. This challenge asks the Luna Coder and Observer to implement a read-only projection across those boundaries. [Challenge #105](https://github.com/RyoSpiralArchitect/Spiral-Coder/issues/105)

The fresh condition uses the original frozen source tree, human task and six-iteration budget. The focused condition starts with the saved two-iteration ID-scope repair, an empty conversation, one new public reproduction and three iterations. Its allowed changes are the validator and its public Rust tests. The frozen plans specify the same model for both roles and retain each first terminal. [Original benchmark](../coupled-status/README.md) · [Focused task](focused-task.txt) · [Plans and results](../../../docs/evals/2026-11-18-interruption-challenge/README.md)

## Run the independent checks

From the repository root, use a binary built from the candidate and supply copies of genuine finalized run directories. The archive check compares request totals and iteration identities, and inventories the files around the invocation. Use a disposable home and build caches for the authored-test checks.

```sh
case_dir=benchmarks/self-dogfood-v2/interrupted-run-accounting
fixture=benchmarks/self-dogfood-v2/coupled-status/status_fixture.py
python3 -BS "$case_dir/interruption_cases.py" "$candidate_binary" "$fixture"
python3 -BS "$case_dir/read_archives.py" "$candidate_binary" "$stopped_run" "$completed_run"
python3 -BS "$case_dir/authored_controls.py" "$candidate_workspace" "$reference_binary" \
  "$host_cargo_cache" "$windows_cargo_cache" "$new_output_directory"
```

The writer-state cases check exact public JSON for valid records, then require rejection with an empty stdout and a diagnostic for inconsistent records. The suite spans empty, absent and spent first ledgers, finalized and pending limits, and native charge defaults.

The authored-test check copies the workspace and uses its exact Rust integration test. It cross-checks compilation on `x86_64-pc-windows-gnu`. A host process then forwards to the same independent reference CLI in five modes. The test must accept native and indented JSON. It must fail on swapped task IDs, iteration numbers 10 and 20, and reversed rows. Each mode records that a two-row response reached the test. Use a Rust toolchain with the Windows target, its C compiler and archiver, and cached locked dependencies for both targets. The program cleans only the Spiral-Coder package in the supplied disposable caches.

To recreate the focused input, apply the archived [starting patch](../../../docs/evals/2026-11-18-issuer-scope/focused-from-baseline.patch) to the prepared original benchmark tree. Install [the visible reproduction](visible.py) at `scripts/status_first_interruption.py` in that tree. Supply the human task above. Keep the operator's independent cases, controls and reference outside the candidate workspace.
