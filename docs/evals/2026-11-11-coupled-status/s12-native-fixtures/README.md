# S12: native fixtures and bounded uncertain-advice handoff

The same Luna Coder/Observer pair ran from the original feature-free source with
no operator edits or seed. [Plan](plan.json) and [preflight](preflight.json) were
committed at `d2e6fb8` before calls. The frozen executor is `ba0ad86`. The native
writer ledger template and explicit valid-base test obligation are new inputs;
the original 24-case oracle, path matrix, six exact checks, four sensitivity
faults and all caps are unchanged. This is a combined condition, not a causal
or matched efficiency comparison.

The first terminal is **unqualified and not adopted**: six rounds, 1,837.266
seconds, 192 task requests / 213 wires, stopped at `iteration_limit`.
Coder used 144 task + 20 transport requests; Observer used 48 task + one transport
request. Roles and child ledgers are verified. The first three rounds and fifth
round authored accepted blocked reports; rounds four and six exhausted their
budget. The final report cites stale verification and is rejected. All six
exact checks remain pending after the final mutation.

| Independent check | Result |
| --- | --- |
| Original status oracle | 24/24 pass |
| Public path boundaries | Three methods / 26 subcases pass |
| Visible behavior, real npm entry, measured guide | Pass |
| Stock regression run | Fails on the new authored test; original Rust main 1,148 pass / two ignored |
| Authored tests on a correct reference | Three pass / one fails before fault injection |
| Authored-test sensitivity | Fail; mutant test executions skipped, no detection credit |

The generated tests use the native fixture CLI, keep numeric IDs, and check a
valid base before duplicate-ID and parent-link negatives. But the parent test
assumes two finalized rows while the valid fixture contains one, and panics
before changing a parent. The wrong-target repair in round five modifies the
separate duplicate-request guard; it does not resolve the parent failure.
The test is retained unchanged for the independent audit. All four operator
fault controls work, so a failed reference prerequisite is not a broken audit.

The new low-confidence report path is **not exercised by this live run**: all
48 parsed Observer primaries have confidence at least 0.87, above 0.75. Native
HTTP/control tests separately cover a 0.74 response, unchanged threshold,
report-only tools, rejection of unsupported completion and mutations, approval
and stale-state stops, and a progressed blocked report entering the next
predeclared iteration. No live handoff improvement is claimed here.

[Diagnostics](diagnostics.json) and the [saved warning-only digest](warning-digest.txt)
show a further failure: four Rust warning locations occupy the compressed
failure's four slots, hiding the authored test panic. A post-run host fix keeps
warning locations out and retains a panic's immediate cause through pruning.
Its separate red/green and full-check receipt is
[post-run validation](../s12-post-run-validation.json). It does not alter this
executor, candidate, sealed receipt, or failed qualification, and no subsequent
provider run of that fix is claimed.

Remaining work is valid fixture cardinality, current failing-test target
retention in both-role guidance, then six fresh exact checks and an authored
current-evidence report. Passing 24 finite cases is not comprehensive correctness.
#82 stays open. [Full results and seal](results.json) · [Exact patch](candidate.patch)
· [Context measurements](context.json). Token figures are estimates; provider
usage receipts are absent.
