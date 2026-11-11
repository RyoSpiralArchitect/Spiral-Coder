# S8: restart acts, then misstates remaining allowance

Fresh original-tree execution with S7's exact input inventory and the UTF-8 digest
correction ran **314.502 seconds**, two rounds, **41 task / 47 wire requests**.
Round 1 uses 32/35 and round 2 uses 9/12. The command and npm route exist, but the
parser remains a placeholder that emits fabricated empty running status.

The second round receives the actual fresh 32-task / 24-Coder / 8-Observer
allowance. It executes the visible check and reads the fixture and parser rather
than immediately repeating the previous blocked report. It makes no new edit or
successful verification; repeated reads eventually hit the existing progress
gate. The frozen scheduler stops at `no_new_edit_or_verification`.

The final Coder explanation says only one native call remains. The durable
second-round ledger instead contains six Coder task requests and three Observer
task requests, leaving **18 Coder requests** after its blocked report. Three extra
Coder wires were charged to the transport pool. The displayed next-action
contract already carries the shared remaining count, but its ambiguous
`max_native_tool_calls: 1` may be confused with total allowance. That is a
hypothesis from this output, not a proven causal explanation.

Independent results are **2/24**, substantially below S6's 19/24 and S4's 21/24.
Only malformed-JSON and unsupported-version cases pass; the build passes, visible/npm/
guide fail, and stock checks stop at formatting. No final exact check is current,
no successful done is accepted, and required documents/tests are absent. No
candidate is adopted, and #82 remains open. Restart activity alone is not success.

[Results](results.json) · [Exact patch](candidate.patch) · [Context](context.json)

S9 separately tests an explicitly named per-response tool limit and a current
numeric shared allowance on every next-action contract. It leaves task caps,
Observer reservation, offered-tool phases and the no-progress stop unchanged.
