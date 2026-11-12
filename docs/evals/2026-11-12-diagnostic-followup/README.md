# Repeated diagnosis follow-up

When the same required check and its diagnostic read both return equivalent
evidence twice, the coupled executor can return to Observer immediately after
the second read. Observer receives the two failure receipts and the completed
read, with a request for one different observation or a supported repair.

## Saved session replay

Both executors replayed three prefixes from the sealed S15 session for
[#82](https://github.com/RyoSpiralArchitect/Spiral-Coder/issues/82) without
provider calls or tool execution. At message 138, only one cycle was complete.
At message 154, the second failure still required reading
`scripts/status_visible.py`, and both executors rejected the synthetic
alternate read. At message 157, the new executor alone projected the repeated
diagnosis. Both executors could forward the alternate advice after the
required read, and all six exact checks were still pending.

## Paired live continuation

Both legs started with the sealed S15 candidate and the same session prefix
through message 154. The runner rebuilt the observation cache and session
summaries from the prefix, used the same workspace path, and restored the
candidate before each leg. Each leg ran Luna Coder with Luna Observer under
12 task requests, up to 16 wire requests, four reserved Observer requests,
four transport repairs, and a 240-second cap. The runner pinned both binaries,
inputs and the before-then-after order before any provider calls. Each condition
ran once.

| Observed | Before | After |
| --- | --- | --- |
| First requests | Coder → Coder → Coder | Coder → Observer → Coder |
| First fixture read | Lines 1–87 after a rejected check attempt and a successful binary test | Lines 60–87 as the next tool action after the required read |
| Executed repeats of the failing check | 1, failed | 1, failed |
| Candidate changes | Added `tests/coupled_status.rs` | None |
| Terminal state | Blocked, 6 exact checks pending | Blocked, 6 exact checks pending |
| Task / wire requests | 12 / 13 | 12 / 14 |
| Wall time | 131.544 s | 124.731 s |

The after trace records `observer_diagnostic_followup` on the mandatory
read's call ID, before the next model request. Observer identified the missing
fixture expectations, but proposed lines 80–160, past the `expected()`
function at lines 62–78. Coder read lines 60–87 and obtained the function, so
the native receipt correctly records a different range from the exact
suggestion. Coder then reran the visible check without isolating the
expected-versus-actual field difference.

The before leg later read the full fixture and repaired a compiler error in
its new integration test. Its earlier successful exact binary check preceded
that edit. The after leg's broad `cargo test 2>&1` passed. Both legs still
lacked current receipts for the six required checks.

This single seeded pair measures when the two roles change direction.
Request savings, repair success and fresh #82 qualification remain
unestablished. The two first attempts were retained with their candidate
inventories, provider reports and traces. [Recorded inputs and results](results.json)

The executor passed `scripts/check-local.sh`, including 1,184 Rust main
tests, six integration tests and 20 TUI replays. A mocked native Coder request
also covers the early yield and forwarding of different Observer advice.
