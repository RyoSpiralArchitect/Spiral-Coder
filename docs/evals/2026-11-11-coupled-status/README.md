# Longer additive self-dogfood: coupled-status

The next task after #49 is tracked in [issue #82](https://github.com/RyoSpiralArchitect/Spiral-Coder/issues/82).
The [benchmark contract](../../../benchmarks/self-dogfood-v2/coupled-status/README.md)
adds read-only native run inspection, honest unknown pending usage, npm routing
and a measured usage guide. The first launch S1 stopped before any provider calls at the old four-iteration CLI limit. [Original launch and correction](s1-launch/README.md)

S2 then stopped after 20 real requests on a pre-response connection closure. Its incomplete patch and independent failure are [retained](s2-interrupted/README.md).

S3 reached an accepted blocked report after 30 task requests / 31 wires, but the scheduler stopped with two Coder requests unused. Its build passes; behavioral and documentation gates fail. [Retained S3](s3-blocked/README.md)

The executor now permits fresh progress from an accepted blocked report to use the next predeclared iteration. Approval barriers, no progress, errors and completion still stop. [Native red/green controls](early-blocked-recovery.json) · [Send-recovery controls](send-recovery.json)

S4 completed four iterations in 947.206 seconds, using 103 task requests / 118 wires. All four exact native checks and the guide passed, but duplicate-key Observer responses prevented the final report. Independent status tests reached 21/24; the existing-document preservation check also failed. The candidate is unqualified and not adopted. [Full S4 result](s4-four-rounds/README.md)

The terminal raw response pair is now a native regression fixture. The final executor correction offers one report-only Coder attempt after a malformed Observer repair when all native proof is current and no approval barrier exists. It keeps duplicate-key rejection and every report gate. [Replay controls](duplicate-closeout.json) This post-S4 fix has model-free validation; no fresh provider run of that final revision is claimed.

[Plan](plan.json) pins the full candidate source at PR #81's
`7ad8be4`, the separate executor revision, exact binary, frozen input/driver declaration and wrapper hashes.
PR #81 was merged as `8e3cecf`; its Git tree is identical to the candidate pin.
[Merge verification](merge-proof.json) The pair is `gpt-5.6-luna` in both distinct roles.
Limits are six iterations, 192 task requests, 216 wires and 3,600 seconds.
The original baseline contains no solution or new benchmark/reference files.

[Preflight](preflight.json) records the required missing-command baseline
failures and all seven passing S13 reference groups (six in S1–S10). The visible check, 24 independent
cases, real assembled npm entry, guide check and all stock regressions were
executed without provider calls. The separate reference can read the real
R25 chain and recover its 73 task requests and 80 wires.
[Actual-record reference control](reference-real-r25.json)

The plan preserves the first terminal outcome and runs independent validation
on a disposable replay of the sealed candidate. Native completion, independent
qualification, authorship and adoption are reported separately. No candidate
patch is automatically merged or used to replace the running executor.

S5 tested an explicit checklist with the final S4 executor. It stopped after
three rounds (95 task / 107 wires), with 15/24 independent cases passing and
no qualification. [Failed checklist condition and concrete diagnoses](s5-checklist/README.md).

S6 keeps S5 inputs fixed and tests cause retention plus numbered-plan feedback.
It reaches 19/24 independent cases in four rounds (98 task / 107 wires), then
stops without new progress. [S6 evidence and remaining contract failures](s6-type-recovery/README.md).

S7 adds actual-budget restart guidance and visible pending-state diagnostics.
It stops before the first round finalizes: an Observer digest cuts inside a
UTF-8 character and panics after nine requests. This is an executor failure;
no conclusion about the intended two changes is possible from this run.
[Preserved panic, pending ledger and incomplete patch](s7-utf8-panic/README.md).
The UTF-8 correction has a failing-before/passing-after native reproducer.

S8 survives the digest path and resumes actual reads/checks in round 2, but makes
no further edit and stops after 41 task / 47 wires. Independent cases are 2/24,
not an improvement. Its final explanation mistakes remaining allowance as one
call despite 18 Coder requests left. [S8 failure and evidence](s8-restart/README.md).

S9 explicitly separates per-response tool cardinality from current shared Coder
allowance. Four rounds reach the first native completed chain in this series:
855.001 seconds, 114 task / 128 wires, all four current exact checks, required
documents and a native done. Independent cases are 18/24, six invalid-record
checks fail, and the external standalone-line report requirement also fails.
Visible, npm, guide and original regressions pass. **Still unqualified and not
adopted; #82 remains open.** [S9 delivery progress and remaining gates](s9-shared-allowance/README.md).

S10 retains original behavior bullets as typed requirements and requires the
requested focused tests plus a fifth exact native check. The report recognizer
now covers the task's unquoted standalone Before/After instruction. Four rounds
reach native done in 985.451 seconds (103 task / 115 wires), with all five current
checks and a correctly shaped report. Independent cases reach **23/24**, including
all six S9 rejection failures; a missing active ledger file still fails. The
original Gemini environment test independently fails, so stock checks also fail.
**Unqualified, not adopted, #82 open.** [S10 evidence and limits](s10-validation-obligations/README.md).

S11 is prospectively pinned with an explicit test-only baseline correction, a
public 26-subcase path matrix, six exact checks and a separate authored-test
sensitivity gate. Before provider calls, a valid reference passes all seven
independent groups. Operator reference tests detect all four advertised faults;
a thin valid-state control misses three and fails the audit. S10's unchanged
authored tests fail on the valid typed reference because their success fixture
omits native required fields, so no mutation detection is credited. This is
diagnostic evidence, not a rerating of S10. [Sensitivity controls](s11-sensitivity-controls.json).
The new condition is not a matched comparison; the original 24 cases, original
visible check and provider budgets are unchanged. The first terminal is now retained: four rounds, 1,120.588 seconds, 124 task /
137 wires. Missing pending paths pass; six dangling-link subcases and three of
24 original cases fail. Authored tests fail on the reference, docs are missing,
and the chain stops when the primary Observer action at confidence 0.74 cannot
produce a hint under the existing 0.75 threshold. This is not an API outage.
**Unqualified, not adopted, #82 open.** [S11 evidence](s11-path-boundaries/README.md).


S12 pins the native work-ledger fixture and explicit valid-base, single-invariant
CLI-test obligation. The executor adds one report-only return for a current valid
Observer primary below 0.75; the advice is never adopted and all completion,
approval, progress and request-cap gates remain. Reference preflight passes all
seven groups. Reference-authored tests detect four faults, the thin control
misses three, and unchanged S11 tests fail both valid-reference cases before any
mutation credit. [S12 controls](s12-sensitivity-controls.json). Both roles retain
the new obligation through context projection. The full local suite passes
1,169 Rust main tests plus two ignored, six integration cases and 20 TUI replays,
with Python, web, npm, repo-map and Lite smoke checks passing. This is a newly
pinned combined condition, not a matched causal comparison. The first terminal is now retained: six rounds, 1,837.266 seconds, 192 task /
213 wire requests, stopped at the iteration limit. **All 24 original cases and
26 path subcases pass**, as do the visible, npm and guide checks. The generated
tests use the native helper but one assumes two fixture rows where only one
exists: three pass / one fails on both candidate and valid reference, so no
fault-detection credit is awarded. Stock checks fail there, final exact proof is
stale and the final report is rejected. All Observer primaries are at least
0.87, so the new low-confidence path has deterministic coverage but no live
invocation. **Still unqualified, not adopted, #82 open.**
[S12 evidence and post-run diagnostic correction](s12-native-fixtures/README.md).


S13 is prospectively pinned before calls: the baseline inventory is byte-identical
to S12, and only prompt.txt differs among frozen inputs. Fixture cardinality and
positive-scenario validation become explicit typed obligations. The executor
retains the current native failure ahead of old implementation guesses and
includes the post-S12 warning/panic correction. Local checks pass 1,171 main Rust
tests / two ignored, six integration cases and 20 TUI replays, plus all other
required suites. Seven reference groups pass; reference tests detect four
faults, the thin control misses three, and retained S12 tests fail their valid
reference prerequisite. [S13 controls](s13-sensitivity-controls.json). This is a
combined condition, not a causal comparison. No provider calls at this pin;
the first bounded terminal will be retained, without candidate adoption.
