# Longer additive self-dogfood: coupled-status

The next task after #49 is tracked in [issue #82](https://github.com/RyoSpiralArchitect/Spiral-Coder/issues/82).
The [benchmark contract](../../../benchmarks/self-dogfood-v2/coupled-status/README.md)
adds read-only native run inspection, honest unknown pending usage, npm routing
and a measured usage guide. The first launch S1 stopped before any provider calls at the old four-iteration CLI limit. [Original launch and correction](s1-launch/README.md)

S2 then stopped after 20 real requests on a pre-response connection closure. Its incomplete patch and independent failure are [retained](s2-interrupted/README.md).

S3 reached an accepted blocked report after 30 task requests / 31 wires, but the scheduler stopped with two Coder requests unused. Its build passes; behavioral and documentation gates fail. [Retained S3](s3-blocked/README.md)

The executor now permits fresh progress from an accepted blocked report to use the next predeclared iteration. Approval barriers, no progress, errors and completion still stop. [Native red/green controls](early-blocked-recovery.json) · [Send-recovery controls](send-recovery.json)

S4 will use the same original task, baseline and six-iteration envelope after local validation of the continuation correction.

[Plan](plan.json) pins the full candidate source at PR #81's
`7ad8be4`, the separate executor revision, exact binary, frozen input/driver declaration and wrapper hashes.
PR #81 was merged as `8e3cecf`; its Git tree is identical to the candidate pin.
[Merge verification](merge-proof.json) The pair is `gpt-5.6-luna` in both distinct roles.
Limits are six iterations, 192 task requests, 216 wires and 3,600 seconds.
The original baseline contains no solution or new benchmark/reference files.

[Preflight](preflight.json) records the required missing-command baseline
failures and all six passing reference groups. The visible check, 24 independent
cases, real assembled npm entry, guide check and all stock regressions were
executed without provider calls. The separate reference can read the real
R25 chain and recover its 73 task requests and 80 wires.
[Actual-record reference control](reference-real-r25.json)

The plan preserves the first terminal outcome and runs independent validation
on a disposable replay of the sealed candidate. Native completion, independent
qualification, authorship and adoption are reported separately. No candidate
patch is automatically merged or used to replace the running executor.
