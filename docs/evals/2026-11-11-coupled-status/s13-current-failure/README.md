# S13: current failure guidance and fixture cardinality

The first fresh Luna/Luna terminal is **unqualified and not adopted**. Three
rounds used 66 task requests / 74 wires in 487.371 seconds. Rounds one and two
made source/routing edits and accepted blocked reports. Round three spent one
Coder request repeating a blocked report and one terminal Observer diagnosis,
with no new edit or verification, then the native no-progress rule stopped the
chain. The six-iteration cap was not exhausted and was not extended.

[Plan](plan.json) was committed at `b64d8ff` before calls; runtime `548b923` and
its binary are frozen. The baseline inventory is identical to S12, and only
prompt.txt changes among input files. The runtime combines current-failure
priority with the prior post-S12 warning/panic correction. This combined
condition is not a matched causal comparison. No candidate was seeded or edited
by the operator.

The independent oracle is **22/24**, below S12's 24/24. `test_running` and
`test_unknown` fail with `manifest totals mismatch`. The public boundary matrix
also has three failing missing interrupted-path subcases. Native build and real
npm routing pass. Guide and visible behavior fail; stock checks stop at rustfmt,
so later stock suites are unrun. The required authored test is absent and the
four-fault candidate audit is not run. All six native checks remain pending.

This run did not reach the newly authored-test repair/cardinality stage, so it
provides no live evidence of improvement there. The native red/green regression
separately verifies that a correlated authored-test failure suppresses older
module guesses before/after diagnosis and session round-trip, keeps the current
cause/check in shared obligations, and resumes ordinary discovery only after
actual check successes. Edit scope, protected tests, failure status and evidence
gates are unchanged. [Runtime validation](runtime-validation.json).

Remaining failure modes include invalid single-file patch framing, recovery
reads after rejected edits, and a fresh iteration immediately repeating the
previous blocked report despite remaining budget. These are follow-up work,
not grounds to weaken the no-progress or completion gates. #82 stays open.
Merging the harness PR does not adopt this candidate or qualify the feature.

[Full results](results.json) · [Diagnostics](diagnostics.json) · [Exact sealed
patch](candidate.patch) · [Context measurements](context.json). Token estimates
are not provider usage receipts.
