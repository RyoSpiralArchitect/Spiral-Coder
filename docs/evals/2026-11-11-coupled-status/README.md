# Longer additive self-dogfood: coupled-status

The next task after #49 is tracked in [issue #82](https://github.com/RyoSpiralArchitect/Spiral-Coder/issues/82).
The [benchmark contract](../../../benchmarks/self-dogfood-v2/coupled-status/README.md)
adds read-only native run inspection, honest unknown pending usage, npm routing
and a measured usage guide. The first prospective run is S1.

[Plan](plan.json) pins the full candidate and executor source at PR #81's
`7ad8be4`, the exact binary, frozen input/driver declaration and wrapper hashes.
The live run begins only after that change is merged, with its Git tree checked
against the candidate pin. The pair is `gpt-5.6-luna` in both distinct roles.
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
