# Native accounting at an interruption

The new task starts with two saved observations. The preceding candidate passed all 24 original CLI cases, and failed 11 of the next 27 writer-state cases. Its public two-iteration test rejected valid pretty-printed JSON and accepted all three output faults. It also used Unix-only setup in unconditional tests.

The operator comparison uses the same reference CLI for every output mode. With a portable semantic assertion, the Windows compile check passed and the three faulty outputs each failed the two-iteration test. The reference passed all 27 new cases and read three genuine archives without changing any files. The new visible reproduction failed on the starting candidate and passed on the reference. [Calibration](calibration.json) · [Operator test-only positive control](operator-positive-tests.patch)

Both live conditions use the same reviewed runtime and Luna for both roles. The original task retains six iterations and its original 26 qualification gates. The focused repair has three iterations and seven required post-edit commands. Both are evaluated against the new controls after their first terminal. [Fresh plan](plan.json) · [Focused plan](focused-plan.json) · [Reproduction instructions](../../../benchmarks/self-dogfood-v2/interrupted-run-accounting/README.md)

## First live outcomes

| Starting point | Native terminal | Original CLI cases | Original check groups | Additional writer states |
| --- | --- | --- | --- | --- |
| Original task, empty conversation | No new edit or verification in round 4, 82 task / 90 wire requests, 837.412 s | 23/24 | 5/7 | 15/27 |
| Saved 24/24 candidate, focused task, empty conversation | Three-round limit, 95 task / 101 wire requests, 825.070 s | 24/24 | 6/7 | 18/27 |

The fresh candidate reached 19/26 original qualification gates. It accepted an interrupted manifest whose aggregate disagreed with its ledger and changed existing state-schema documentation. Its authored tests passed on the reference and detected three of the four frozen faults. They missed the symlink fault. The chain stopped with an accepted blocked report. Its exact authored-test source compiled for Windows. None of its tests exercised a two-row response. [Fresh terminal and independent scores](fresh-results.json) · [Candidate patch](fresh-candidate.patch)

In the focused condition, native action 1 reproduced the public failure before the first edit. The candidate gained two writer-state cases, preserved all 24 original CLI cases and passed four of seven declared commands after the final edit. The Rust unit and integration commands and the path check still lacked current receipts. Nine new cases failed across optional counts, known and per-iteration limits, and unknown charge values. Its authored tests failed on the reference before the four original mutation trials could run. Its Windows compilation failed on unconditional Unix symlink calls. [Focused terminal and independent scores](focused-results.json) · [Test and validator delta](focused-delta.patch)

Both candidates read the three earlier native archives with the correct counts and unchanged file inventories. We also exercised the actual writer with an unreadable session and with a local failure writing its first report. It persisted unknown totals in the first case and one task / one wire in the second. Both candidates read both records correctly. The preceding candidate rejected both. These two supplemental readings generated zero external requests. [Native writer controls](calibration.json)

The fresh patch re-created all 1,669 terminal file entries from the original prepared tree. The focused delta re-created all 1,670 terminal file entries from its saved starting tree plus the one public fixture. Both candidates remain experimental.

## A wrong fixture changed the validator

The focused Coder started its new empty-ledger test from the existing two-iteration interrupted fixture. It removed the finalized rows and wrote an empty ledger in `iteration-01`. Its manifest continued to point at `iteration-02` and to report nonzero usage. The independent reference rejected that input. While diagnosing the test, the pair changed the validator to accept an interrupted directory number read from the directory string itself.

A separate control starts with one finalized row and a valid interrupted `iteration-02`. It changes only the pending directory and its manifest pointer to `iteration-03`. The reference, preceding candidate and fresh candidate all accepted the control and rejected the gap. The focused repair accepted both. This observation was made after the two measured runs and is outside the 27-case score. [Paired public outputs](directory-gap.json) · [Reproduce the check](check_directory_gap.py)

A test-only operator correction starts from a real first-iteration fixture, keeps its directory and task ID, and sets both interrupted, manifest and ledger counts to zero. The same seven authored tests then pass against the focused binary and the independent reference. The measured candidate files, failed native test and qualification receipts remain archived. This supplies a concrete next handoff for the Observer. Compare the fixture's directory and counts to the writer, correct the test input, and verify the valid and skipped-directory cases together before proposing a validator change. [Test-only correction](operator-empty-fixture.patch) · [Seven-test runs on both binaries](empty-fixture-control.json)

## Attribution of authored-test failures

The frozen output scorer could mark an individual altered-output invocation as red when an unrelated test was already failing on the unchanged reference. Its overall gate required the positive modes and was false for both candidates. The focused suite failed the same empty-ledger test on all three altered outputs. The fresh suite made zero calls that returned two rows. Neither candidate demonstrated sensitivity to those three output faults.

The current scorer grants fault detection only after the same test suite has passed on both native and pretty-printed two-row output. An offline rerun on all four test sources reproduced the original exits and named failures for both candidates. The operator's positive control still distinguished all three altered outputs. The original plans and qualification scores retain the frozen scorer digest. [Paired rerun](output-attribution.json) · [Frozen scorer](https://github.com/RyoSpiralArchitect/Spiral-Coder/blob/6878d2d1c7c3075940fdcfd1823a3f5d2e273ef1/benchmarks/self-dogfood-v2/interrupted-run-accounting/authored_controls.py)
