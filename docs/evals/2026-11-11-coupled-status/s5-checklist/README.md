# S5: explicit checklist, unsuccessful fresh run

S5 uses the original baseline, unchanged evaluator and six-round cap, the
post-S4 report fallback, and an explicit validation/documentation checklist.
Only prompt.txt changed in the input inventory. This is not a matched causal
comparison. Baseline expected failures and all six reference groups passed.

The fresh Luna/Luna pair ran three rounds in **768.453 seconds**, spending
**95 task requests / 107 wires**, then stopped for no new edit or verification.
The candidate builds but fails visible behavior, package and guide checks.
Independent cases are **15/24**, below S4's 21/24; this is not improvement.
Stock checks stop at formatting, so later stock suites did not execute.
No final accepted report, documentation delivery or candidate adoption occurred.

The custom ledger declares numeric request IDs as strings, but the pair
repeatedly diagnoses the resulting integer/string error as a charge problem.
The missing pending ledger also fails through an incompatible typed shape.
Several negative cases pass by rejecting otherwise valid inputs; they do not
establish the intended validation behavior. Do not treat the 15 passes as a
capability floor.

The third-round plan also joins four admitted criteria into one paragraph.
The parser therefore sees one criterion, losing the original references. The
[retained rejection](plan-format-rejection.json) shows why copying words alone
was insufficient. Post-run feedback now states the exact admitted count and
requires separate numbered lines. It does not silently split, approve or
otherwise change the authored plan. This feedback change is model-free tested;
no post-change provider run is claimed here.

[Results](results.json) · [Original candidate](candidate.patch) ·
[Context](context.json) · [Frozen prompt](prompt.txt).

Next fresh condition should make native input field types and criterion
formatting unambiguous, while keeping all behavior and preservation gates.
A continuation of this candidate must be separately declared as assisted or
seeded; it cannot retrospectively qualify S5.
