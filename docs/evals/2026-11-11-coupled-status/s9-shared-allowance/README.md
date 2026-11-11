# S9: native completion across four rounds, independent qualification fails

S9 keeps S8's exact original baseline, input inventory, prompt, oracle, reference
and six-round budget. Its executor additionally names the tool limit
`max_native_tool_calls_per_response` and states current shared Coder allowance on
every next-action contract. No phase/tool, report, approval or progress gate is
relaxed. Local validation passes 1,162 main Rust tests (two ignored), six
integration, 20 TUI, 97 Python, 80 web, five npm, 14 repo-map and Lite smoke.

Fresh Luna Coder/Observer execution ran **855.001 seconds**, four rounds,
**114 task requests / 128 wires**. Rounds spend 32/34, 32/36, 32/36 and 18/22.
Round 2 edits the parser and guide; round 3 edits state-schema and obtains visible
and formatting proof; round 4 repairs, rechecks and authors an accepted native
done. All four exact commands are current after the last mutation and before
that done, required paths changed, and all ledgers, caps and seals verify.
No operator edits or steering occurred inside the candidate.

This is the first native completed chain in S1–S9, but **not a qualified result**.
The independent oracle passes **18/24**, below S6's 19/24 and S4's 21/24. The six
remaining failures accept invalid records:

- completion conflicts with a pending active iteration;
- duplicate task IDs;
- symlinked finalized ledgers;
- interrupted manifest/pending totals that conflict;
- wrong parent linkage;
- ledger task ID differing from the finalized row.

Completed/running/interrupted/unknown/stopped, absent pending ledger and duplicate
request-ID cases pass. The visible check, actual npm entry, documented example
and all original stock regressions pass independently. The native report's
1,148 passed / two ignored count describes the pinned candidate baseline, not
the separately developed 1,162-test executor. These test counts must stay separate.
Some negative probes alone are not proof of robust validation: for example, the
candidate rejects the budget-overflow fixture through subtraction failure and a
traversal fixture with no outside file through failed I/O. The unimplemented
identity, path and completion checks require semantic repair, not more reporting.

The independent authored-report gate also fails. The summary contains
`Before: FAILED (...)` and `After: PASSED (...)` inside one paragraph, rather than
the required standalone exact lines. Native done acceptance does not establish
this external formatting requirement or the independent semantic requirements.
Every other chain gate passes, but the conjunctive verdict remains false.
No candidate is adopted; #82 stays open. This single descriptive run cannot
isolate a causal effect of the allowance wording or establish repeatability.

[Results and exact call receipts](results.json) · [Frozen patch](candidate.patch) ·
[Context and ledger measurements](context.json) · [Predeclared plan](plan.json)

Next boundary: maintain a task-derived checklist for explicit negative-validation
requirements across repairs and planning, and preserve exact final-output shape
requirements in native report validation. New visible controls must be declared
as a new benchmark condition; do not reveal the held-out oracle mid-run or relax
it to accept this candidate. Repeated plan/diagnosis transitions still consume
requests and need separate replay-based investigation.
