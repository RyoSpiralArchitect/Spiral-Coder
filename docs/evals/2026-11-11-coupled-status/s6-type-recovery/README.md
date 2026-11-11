# S6: input-type diagnosis progresses, delivery remains incomplete

The baseline, prompt, input inventory, protected paths, visible fixtures and
six-round budget exactly match S5. The executor additionally preserves concrete
deserialization causes in recovery summaries and requests numbered acceptance
items. All baseline expected failures and six reference groups passed before calls.
The new runtime passes 1,159 main Rust tests (two ignored), six integration tests,
20 TUI cases, 97 Python, 80 web, five npm, 14 repo-map cases and Lite smoke.

Fresh Luna Coder/Observer execution ran **931.021 seconds**, four iterations,
**98 task requests / 107 wires**. Rounds used 32/36, 32/33, 32/36 and 2/2
(task/wire). Round 4 immediately repeated the prior blocked report; without a
new edit or verification the frozen scheduler correctly stopped. No operator
edits or mid-run guidance occurred. No candidate is adopted and #82 stays open.

Input handling progressed beyond S5's integer/string failure: the pair repaired
request-ID reading, and the independent completed, two-round, stopped, old-default
charge and in-flight cases pass. Numbered-plan feedback was delivered and further
actions executed after the rejection. This single unpaired outcome cannot isolate
the causal effect of either runtime change.

Independent cases are **19/24**, versus S5's 15/24 and S4's 21/24. Remaining failures:

- duplicate request IDs are accepted;
- interrupted and unknown pending records fail with missing task_id;
- a running record fails manifest task count validation;
- a wrong ledger task ID is accepted.

The build passes, but visible, packaged and guide checks fail. Required documents
and focused tests were not delivered. Stock checks stop at formatting; subsequent
stock suites are unrun. Native done was not accepted and no final exact check was
current. **This is an incomplete, unqualified result, not a breakthrough.**

[Assessment](results.json) · [Frozen patch](candidate.patch) · [Context](context.json)

Next investigation: preserve the distinction between pending metadata and its
ledger (including absent ledger), validate identity rather than invent aliases or
fallback IDs, and make the first action of a new budgeted iteration inspect or
repair the concrete outstanding issue instead of echoing the previous blocked
terminal. User/approval barriers and no-progress stop rules must remain intact.
