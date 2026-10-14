# One task through Coder and Observer

Coder and Observer are roles in one task. An accepted suggestion is not a completed task. The unit of qualification is the original request, its preserved artifacts, current verification, authored final report, and the total request cap.

## State and continuation

`AgentSession` owns the native tool exchanges. `tui/events/coder_session.rs` builds a continuation from that session and marks automatic feedback as `origin=runtime`. Rendered pane history is not replayed as execution evidence. New human steering establishes a new task boundary; old completion cannot satisfy it. Managed system context is replaced on each UI request. Local `origin` and `ui_context` fields are removed before provider requests. Google continuation metadata remains attached to its original tool call.

TUI autosaves `.spiral-coder/tui-session.json`; the companion `.spiral-coder/tui-work.json` owns the budget and assist receipts. Missing or mismatched budget state cannot silently allocate fresh requests for a resumed task. Changing `/root` cancels both roles, invalidates their generations, and resets project-local context. Ctrl+L clears the displayed pane only; Ctrl+K cancels the request and continuation restores the latest native autosave.

Each pane has a request generation. `request_stream.rs` publishes the native snapshot before Done/Error and rejects delayed events from canceled requests. Observer repair remains bounded to one attempt, tied to the source task revision.

A runtime iteration-cap marker is an incomplete task even when the last tool succeeded. It remains eligible for Observer handoff while the shared budget allows a continuation. Final-answer requirements stop at their own sentence; separate preservation instructions remain artifact obligations rather than fabricated report literals.

## Receipts and budget

`work_budget.rs` owns durable `WorkState`: task identity, workspace, original request, request limit, request receipts, assist receipts, and explicit replay-preservation baselines. It reserves a slot before every supported provider HTTP attempt, including compatibility retries and Observer repair. Interrupted attempts remain spent after restart.

Request receipts separate estimated input size, nullable provider usage, nullable request duration, and status. Estimated input is a character-based estimate of each transmitted payload, not the retained transcript length. Sum of request durations differs from task wall time. A missing provider usage record is unknown, not zero. Streaming telemetry is best effort; the durable budget is authoritative for the shared cap.

Budget saves carry a task identity and monotonic persistence revision. Stale handles cannot replace a new task's ledger or remove requests recorded after same-task reload. In-process writes are serialized; separate TUI processes should not share the same active session files. Canceling the agent also aborts its nested provider task. Reservations already made remain spent.

Assist receipts connect the suggestion generation, continuation generation, native call IDs, actual results, pending exact checks, and completion. A pending approval is not executed. A returned read/search result does not automatically establish success. A valid suggestion can be accepted, overridden, or deferred; those decisions do not change execution evidence.

The first Coder chunk leaves room for Observer, outstanding exact checks, and reporting. Near the cap, the runtime supplies the remaining exact commands and avoids optional periodic reflection. Existing mandatory gates remain in force. A bundled command or passing automatic test cannot replace a separately requested exact native exec. Exhaustion leaves the task incomplete.

## Structured artifact preservation

An explicit request mentioning `tui_replay.json`, preservation, and existing cases captures the current full JSON as a baseline before execution. File writes, patches, and diffs reject removal or modification of any old case, including nested `based_on`, assertions, and extension fields. Shell edits are checked again when observing and finishing. This is a correctness guard, not a filesystem security boundary.

The evaluator retains a separate baseline outside the model's writable fixture. It checks full old JSON values, unique IDs, the requested new case, and an actual replay of all cases. A valid replay format example helps distinguish a case ID from its selected Coder message. The example is neither proof nor a replacement for running checks. Typed validation must not serialize old cases back through a lossy struct.

## Browser integration

The Rust server exposes `POST /api/observer_action` and `features.observer_action=true`. It shares the TUI packet projection, JSON validator, confidence/no-tool forwarding behavior, single repair prompt, native exchange repair, and final-report checks. The endpoint never executes advice.

`web/core/work-loop.js` owns browser-native task state under `spiral-coder.work.v1.<thread>`. Request metadata selects the server ledger `.spiral-coder/web-budgets/<task-id>.json`; changing the limit/root or reloading cannot refund wire attempts. Native history and assist IDs survive reload. A stale revision or previously consumed suggestion cannot be executed again.

Browser next-action continuation is explicit: **Continue with advice** uses the Coder's normal file/command approval policy. An approved command result is correlated back to its original native call before resuming. Pending approval, execution, and completion are displayed separately. Native tool calls are dispatched even when accompanied by `finish_reason=stop`. Usage trailers after stop remain observable. Node's `node --test` is a behavioral verification command in the shared catalog.

`write_file`, `patch_file`, and `apply_diff` all queue previews when edit approval is required. The server retains the original content, prepared content and preservation budget until approval. If the file changed, it requires a fresh preview. Actual approved output is correlated by the native call, action, approval ID and queued path. Budget exhaustion before streaming returns a JSON error rather than silently closing the socket.

Lite serves the browser assets but does not implement the native task/Observer contract. The UI explains that this continuation requires `spiral-coder serve`; it does not fabricate equivalent evidence.

## Qualification

`coupled-run` drives the actual TUI producer and consumers without rendering a terminal. Both roles use the selected model and one wire cap. `--case-spec` evaluates the existing fixed check types against the resulting trace and native session. `--evaluate-only` re-adjudicates existing receipts without a provider call.

`scripts/run_coupled_pilot.py` runs `tests/fixtures/coupled-loop-v1.json` in isolated copies. Provider keys come only from environment variables and are not written to receipts. The original runtime suite and its `patch_file` requirement remain unchanged. The new suite includes a nonempty secondary provenance list and an unknown JSON field so lossy rewrites cannot pass unnoticed.

Deterministic tests, mock HTTP/UI exercises, and bounded live results are separate evidence classes. See [the evaluation record](evals/2026-10-14-coupled-loop/README.md) for the candidate hashes, fixed caps, failure ledger, and limitations.
