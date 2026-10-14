# One task through Coder and Observer

Coder and Observer are roles in one task. An accepted suggestion is not a completed task. The unit of qualification is the original request, its preserved artifacts, current verification, authored final report, and the total request cap.

When the original task explicitly requires a final report, an empty or text-only
provider turn cannot complete it through automatic stop checks. The runtime
retains native proof and requests an authored `done` summary without rerunning
passed checks for reporting. The retry consumes the existing shared budget; if
the report never arrives, the task remains incomplete. Runtime goal-check success
also has no authority to replace that report requirement.

## State and continuation

`AgentSession` owns the native tool exchanges. `tui/events/coder_session.rs` builds a continuation from that session and marks automatic feedback as `origin=runtime`. Rendered pane history is not replayed as execution evidence. New human steering establishes a new task boundary; old completion cannot satisfy it. Managed system context is replaced on each UI request. Local `origin` and `ui_context` fields are removed before provider requests. Google continuation metadata remains attached to its original tool call.

TUI autosaves `.spiral-coder/tui-session.json`; the companion `.spiral-coder/tui-work.json` owns the budget and assist receipts. Missing or mismatched budget state cannot silently allocate fresh requests for a resumed task. Changing `/root` cancels both roles, invalidates their generations, and resets project-local context. Ctrl+L clears the displayed pane only; Ctrl+K cancels the request and continuation restores the latest native autosave.

Each pane has a request generation. `request_stream.rs` publishes the native snapshot before Done/Error and rejects delayed events from canceled requests. Observer repair remains bounded to one attempt, tied to the source task revision.

New human tasks clear the prior rollback checkpoint before startup and capture
the current workspace as their own checkpoint. Runtime continuations retain the
same checkpoint. Changing workspace also clears the UI rollback handle, so a
failed startup cannot expose the previous task's rollback target.

A runtime iteration-cap marker is an incomplete task even when the last tool succeeded. It remains eligible for Observer handoff while the shared budget allows a continuation. Final-answer requirements stop at their own sentence; separate preservation instructions remain artifact obligations rather than fabricated report literals.

Runtime-validated plans, including compatibility-rescued plans that were never
present in the model's text, are recorded as local `accepted_plan` metadata on
the admitted assistant action. Resume revalidates the plan against the current
task contract, scanning past rejected invalid revisions to the last valid plan.
Pruning preserves the latest snapshot; new human steering starts
a new plan boundary. This metadata is removed from provider wire messages.

Recovery restoration skips commands that were blocked or rejected before
execution. A progress-policy refusal must not invent a new workspace diagnosis
after an Observer handoff. Earlier actual command or automatic-test failures
remain visible through those refusals; separate plan, approval, evidence and
verification gates still apply. Refusal text never supplies successful proof.

`task_scope.rs` reads explicit `target_files` from a complete approved benchmark
plan. Those paths override the heuristic first-mentioned path in the intent
anchor and Coder's artifact routing. Paths in the objective remain referenced
subjects, without inferred existence or edit permission. Plans without explicit
targets retain the existing fallback; this is not a general prose interpreter.

`agent/task_obligations.rs` projects the same artifact scope, exact checks still
missing native proof, final-answer clauses/literals, latest action feedback,
and pending diagnosis/repair requirements
for Coder, TUI Observer and the browser endpoint. The final-answer projection
uses the done gate's parser. A projection neither executes advice nor certifies
artifact completion; all existing proof and acceptance gates remain active.
The Observer output description is a JSON Schema, avoiding a hard-coded read
action and made-up example scores that could bias the selected next action.
Both UI paths preserve concrete JSON parser diagnostics in the one permitted
repair request, including invalid enum values and their allowed variants.
Optional display fields such as `fallback: null` cannot mask those actionable
errors or discard a valid primary action. Tool arguments and suggestion kinds
remain strictly validated. Replay loader errors identify the input file and
explain that checks must be a nonempty array beside `observer_response`, so
file-localized recovery can survive a role handoff. A failed edit permits one
diagnostic reread of its target even outside `src/`; unrelated diagnostics do
not substitute for that read, and repeated unchanged reads remain gated.
Resume restores the completed diagnosis as Fix, so an Observer handoff cannot
force another read before the same repair. A later failure still takes precedence.
The first explicit failing verification can establish the same shared recovery
obligation before an edit. A unique, existing workspace file from a Rust diagnostic
or innermost Python traceback determines the diagnostic read; ordinary path
mentions and ambiguous locations do not. Unrelated diagnostics cannot satisfy
that read, and only the original failed check can clear the obligation. Both
roles receive this focus after pruning and continuation. Unlocalized recovery
asks for evidence relevant to the error instead of assuming a Git repository.
The shared Coder prompt names individual, unchanged exec calls as the exact-proof
contract before completion is attempted, rather than only after a rejected report.
The exec wrapper places its location marker on a fresh line even after output
without a trailing newline. Directory changes and root escapes remain observable,
and the wrapped command's exit status is retained.
Historical governor blocks remain in native evidence; they are not labeled as
human approval rejections. The latter list is derived from actual user-rejection
results in both UI paths.

## Receipts and budget

`work_budget.rs` owns durable `WorkState`: task identity, workspace, original request, request limit, request receipts, assist receipts, and explicit replay-preservation baselines. It reserves a slot before every supported provider HTTP attempt, including compatibility retries and Observer repair. Interrupted attempts remain spent after restart.

The HF subprocess provider reserves at the same provider boundary before starting
inference, including initial Observer assistance and its repair. Failed attempts
remain charged; elapsed time and supplied usage are recorded, while absent usage
stays null. Canceling or timing out the call drops and terminates its child process.

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

Every browser read rechecks localStorage. A durable write increments
`persistenceRevision`, separately from the native-message `revision` used by
advice. Handles whose persisted snapshot changed in another tab cannot reserve,
save, claim advice or process a late request result. Coder tool requests check
the active handle before dispatch and after completion. These boundary checks
do not undo server work already dispatched and do not make localStorage an
atomic transaction system for simultaneous writers.

Browser next-action continuation is explicit: **Continue with advice** uses the Coder's normal file/command approval policy. An approved command result is correlated back to its original native call before resuming. Pending approval, execution, and completion are displayed separately. Native tool calls are dispatched even when accompanied by `finish_reason=stop`. Usage trailers after stop remain observable. Node's `node --test` is a behavioral verification command in the shared catalog.

`write_file`, `patch_file`, and `apply_diff` all queue previews when edit approval is required. The server retains the original content, prepared content and preservation budget until approval. If the file changed, it requires a fresh preview. Actual approved output is correlated by the native call, action, approval ID and queued path. Budget exhaustion before streaming returns a JSON error rather than silently closing the socket.

Lite serves the browser assets but does not implement the native task/Observer contract. The UI gates next-action assistance on `features.observer_action`. Ordinary meta diagnosis still uses `/api/chat` and remains available on Lite.
Lite also serves the shared governor contract JSON used by diagnostic packets.
The configuration fingerprint is shared by meta diagnosis and Coder, so saving
a diagnosis does not depend on entering the Coder execution closure first.
`features.meta_diagnose_artifacts` distinguishes Rust's file archive from Lite's
conversation-only diagnosis. The UI never calls absent archive endpoints on Lite
and releases the busy state after displaying the diagnostic result.

## Qualification

`coupled-run` drives the actual TUI producer and consumers without rendering a terminal. Both roles use the selected model and one wire cap. `--case-spec` evaluates the existing fixed check types against the resulting trace and native session. `--evaluate-only` re-adjudicates existing receipts without a provider call.

`scripts/run_coupled_pilot.py` runs `tests/fixtures/coupled-loop-v1.json` in isolated copies. Provider keys come only from environment variables and are not written to receipts. The original runtime suite and its `patch_file` requirement remain unchanged. The new suite includes a nonempty secondary provenance list and an unknown JSON field so lossy rewrites cannot pass unnoticed.

Deterministic tests, mock HTTP/UI exercises, and bounded live results are separate evidence classes. See [the current evaluation record](evals/2026-10-14-baseline-and-review/README.md), [repair-continuation rounds](evals/2026-10-14-repair-continuation/README.md), [task-obligation rounds](evals/2026-10-14-task-obligations/README.md), and [earlier rounds](evals/2026-10-14-coupled-loop/README.md) for candidate hashes, fixed caps, failure ledgers, and limitations.
