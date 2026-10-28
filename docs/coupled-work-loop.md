# One task through Coder and Observer

The compatible streaming decoder correlates indexed fragments by their index.
Providers that omit indices can supply explicit call IDs instead: distinct IDs
remain distinct calls in first-seen order, including separate one-call deltas.
Anonymous fragments after multiple calls, mixed encodings, conflicting indexed
IDs and reused IDs across indices fail closed. Provider thought signatures stay
with their original call and do not become execution proof. A failed coupled
driver loop retains its incomplete report, session and spent request ledger before
returning the error; transport failure must not disappear from an evaluation.

The executor still accepts one tool per turn. Its multiple-tool rejection uses
the normal `GOVERNOR BLOCK` display marker, so Observer rescue recognizes this
incomplete Coder output. Recognizing the blocker does not execute a rejected call
or relax plan, action, proof or report requirements.

Coder and Observer are roles in one task. An accepted suggestion is not a completed task. The unit of qualification is the original request, its preserved artifacts, current verification, authored final report, and the total request cap.

For new maintenance behavior, [explicit task requirements](task-requirements.md)
bind source/test/doc artifacts to caller-chosen checks and optional failing
baselines. Both roles see the pending obligations; editing the model's plan
cannot waive them. This contract also documents small JSON edits that preserve
the complete file without asking the model to reprint it.

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

A runtime `completed=false` outcome also admits the latest completed Coder reply
for assistance when no failure marker was printed, including a chunk spent on
unfinished authored-report retries. It does not admit an older ordinary reply,
an unknown/successful outcome or an in-flight stream. Packet construction and
the normal native-proof and advice-validation gates still apply.

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

Multiple failed checks retain separate recovery obligations. The newest failure
owns the immediate diagnostic read shown to both Coder and Observer, while older
checks remain pending until their own successful native results. For example, a
visible baseline test may fail first and a full-suite automatic test may later
fail in a different supplied test: the next read follows the new test location,
and passing the full suite does not discharge the original exact check. A new
failure with no confirmed location asks for broader diagnosis instead of another
read of the old test. These states survive role handoff, save/resume and pruning;
read progress does not become verification evidence.

`agent/native_arguments.rs` rejects malformed JSON and non-object native tool
arguments before any tool dispatch or workspace-recovery transition. The whole
turn remains unexecuted, with the original call IDs, arguments and provider
metadata paired with refusal results; the attempted provider request stays spent.
The hint asks for a complete object and smaller edits when output was truncated.
The runtime never substitutes an empty object or treats unparsed exec arguments
as a shell command. The browser uses the same boundary in `core/work-loop.js`
and persists the refusal before retrying. Valid JSON still passes the existing
tool, plan, approval, evidence and verification gates.

`agent/provider_turn.rs` also retains rejected multi-call batches before other
protocol gates. Every original call gets a correlated `GOVERNOR BLOCKED` result;
none executes. IDs, argument strings and opaque provider signatures survive
autosave and resume. Rejections cannot become successful proof. This makes the
rejected proposal visible on the next request instead of repeating a bare hint
against a history that never contained the proposal.

`agent/next_action.rs` derives a wire-level next-action contract from the shared
request budget and native requirement status. Each response allows one native
function call. For tasks with an explicit `task_requirements` contract, the final
shared request offers only `done` for an authored report
or truthful incomplete handoff. In the last three requests, report-only mode also
applies when explicitly declared requirements, baselines and exact checks have
current proof. An undeclared or still-pending requirement cannot select this early
path. Undeclared interactive tasks retain their action tools, including deliberate
one-request repair/resume steps. A shorter Coder chunk alone does not exhaust the
shared task budget. Calls
outside the offered report-only tool set are retained and refused without
execution. No additional requests, automatic report text, acceptance repair or
completion authority are introduced; all existing done gates still apply.

When the native recovery focus requires a current diagnostic read, the same
contract offers only `read_file` with that exact path and `done` for an honest
blocked report. It rejects edits, checks and other paths before execution even
if a provider ignores the offered schema. This is the existing diagnosis
obligation exposed at action selection, not an automatic read or weakened check.
A correlated native read (or actual read failure) releases the narrow tool set;
the failed check remains pending. Prior reads and advisory Observer overrides
cannot release it. The final shared report slot still takes precedence.
`pending_artifact_paths` carries explicit artifacts without native edit receipts
into the compact next-action view, so source repair does not hide a remaining
documentation obligation. It does not claim semantic completion of edited files.

`agent/final_instructions.rs` owns bounded explicit English report clauses:
“final answer/report/response/summary must” followed by a supported positive verb
such as include, name, state or disclose. It preserves literal case and sentence
boundaries for both roles and the done gate. Recognized clauses require an authored
report, even when existing tests pass. This parser is not a general interpretation
of arbitrary requests; an independent task oracle is still needed to establish
that the requested change was actually made.

An authored-report task also keeps control of its remaining diagnostic and
repair steps after an intermediate automatic test passes. The legacy eager
closeout shortcut does not force such a task into `done`; normal recovery,
progress, approval and budget gates still apply.

`agent/explicit_checks.rs` recognizes positive inline `Run exactly:` directives
(also `Also/Then/Please run exactly:` or after a sentence boundary) for commands
in the existing verification catalog. Quoting/case remain exact; duplicates are
removed. Negated text, quoted examples and fenced blocks do not create checks.
These commands use the same native-proof ledger as approved benchmark checks:
automatic tests do not satisfy them, each needs a separate successful exec, and
later mutations invalidate earlier proof. Arbitrary shell mutations or custom
commands do not become verification merely because they follow this phrase.

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

`coupled-run` defaults to the same resolved provider, actual model, endpoint and
credential for both roles. To run different families, keep the normal global
Coder options and supply the Observer provider/model together:

```sh
spiral --provider openai --model gpt-5.6-luna coupled-run \
  --tool-root /path/to/isolated-checkout --prompt-file /path/to/task.txt \
  --out-dir /path/to/fresh-external-run --max-requests 24 \
  --observer-provider mistral --observer-model mistral-large-4 \
  --observer-api-key-env MISTRAL_API_KEY --observer-max-tokens 8192 \
  --coder-chunk-requests 10
```

Explicit Observer configuration resolves only its own vendor credential variable
(Gemini accepts `GEMINI_API_KEY` or `GOOGLE_API_KEY`) or the one named with
`--observer-api-key-env`. It never borrows the Coder credential or the general
`SPIRAL_CODER_API_KEY` fallback. A non-default `--observer-base-url` requires an
explicit credential variable; URLs with user information, query or fragment are
rejected. The HF Observer requires no credential variable. Incomplete role
settings, missing Observer credentials and conflicts with `--without-observer`
fail before network I/O or creating run output. Credential values are not CLI
Observer arguments or role receipts.

`--observer-max-tokens` optionally sets an explicit output cap from 1 through
65536, requiring the explicit Observer provider/model pair. The value is
independent of the Coder's `--max-tokens`; it applies to both initial assistance
and the single permitted repair. Without this option, the existing default is
`min(Coder max_tokens, 1200)`. A larger cap can accommodate provider reasoning
before visible JSON, but cannot guarantee valid advice or add wire requests.

`report.json` and the trace's `agent_config` record `roles.coder` and
`roles.observer` (null when disabled), each with resolved `provider`, `transport`,
`model`, `endpoint_origin`, `temperature`, `max_tokens` and `timeout_seconds`.
Only the URL origin is retained, excluding custom paths and secrets.
Sampling fields include the actual next-action Observer settings (temperature
0.2 and the explicit output cap or existing default, also for repair); provider compatibility can
transform or omit those options on the wire. `same_model_both_roles` is true only
when both roles have identical transport, model and complete endpoint identity;
it is false for disabled Observer. `role_requests` counts actual wire receipts
for each role. Configuring an Observer does not establish that it participated;
qualification must inspect these receipts and the original task's evidence.

The optional `--coder-chunk-requests` limits every Coder slice, including
compatibility retries and continuations after Observer advice. It must be at
least four and leave two requests within the initial shared cap. With Observer
enabled, the default slice ceiling is eight. At eight or more remaining requests,
reserve rules preserve capacity for checks, repair and reporting. An explicit
slice also reserves one Observer request and one Coder continuation while three
to seven requests remain. Small default sessions keep their existing Coder
report-retry allowance; a new verification failure can still yield early.
The last one or two requests remain available to Coder.
An ephemeral allowance on the Coder budget handle
enforces this boundary without modifying or refunding the durable task limit.
The option does not force an Observer call after successful early completion,
nor guarantee that advice, verification or repair fits the remaining budget.

The interactive TUI selects the next Observer intervention from the typed
`CoderYieldReason::VerificationFailure` outcome, even with auto-observe disabled
and no failure keyword in the displayed Coder text. This transient reason clears
on the next Coder send, workspace change or error. Duplicate-assist and remaining
budget guards still apply. Continuation restores a pending reflection and its
exact operation identity from the latest failed native verification exec; invalid
reflection cannot bypass the pre-tool gate by crossing a role boundary. A later
diagnostic result clears this immediate-failure restoration, and successful edits
with failed automatic tests keep their existing diagnosis/impact requirements.
Nonexecuted governor/user refusals do not discharge the original reflection
obligation, including an identical retry rejected for contradicting `adjust`.
They also cannot re-trigger early yield for the same historical failure.

`agent/observer_handoff.rs` recognizes a new, correlated native verification
failure before the next Coder provider request. Explicit failed checks and failed
automatic tests after a successful edit yield while at least two shared requests
remain. A local monotonic tool counter excludes historical failures on resume;
blocked/awaiting-approval tools, read output, unknown outcomes and unrelated shell
failures cannot trigger this return. The native session and pending proof survive
the handoff. `observer_verification_handoff` records the failure and the outcome
stays incomplete; it neither marks a repair nor clears a check. Standalone CLI
and `--without-observer` executions retain their existing continuation behavior.

Observer suggestion parsing can recover only a missing initial `{`, or `{"`,
when the first field is an exact known envelope key and all remaining bytes form
one complete JSON object. It never appends a closing delimiter or supplies a
value, tool, argument or confidence. The 64 KiB bound and recursive duplicate-key
checks still apply; ambiguous, truncated and trailing content remains invalid.
TUI retains the original response in `raw_content` and records the recovery;
the shared Web evaluator exposes `format_recovery`. Tool schema, confidence,
pending-diagnosis, scope and native-completion gates remain unchanged. An
unrecoverable response can still use the existing one explicit repair request.

`work_budget.rs` owns durable `WorkState`: task identity, workspace, original request, request limit, request receipts, assist receipts, and explicit replay-preservation baselines. It reserves a slot before every supported provider HTTP attempt, including compatibility retries and Observer repair. Interrupted attempts remain spent after restart.

Anthropic non-streaming chat reserves at this same boundary, including TUI
meta-diagnose and Web Observer calls. An exhausted scope prevents network I/O;
HTTP errors and invalid responses remain spent. Successful replies record the
provider's usage object without inventing missing usage.

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

Before forwarding a confident `done` suggestion, both surfaces validate its
acceptance rows against the current native plan and current recorded commands,
using Coder's acceptance validator. Missing, invented, shortened or stale
citations request the same single action-contract repair. Display packet fields
cannot supply receipts. Low-confidence and no-tool advice remain advisory.
If Coder has not yet established a native plan, citations are still checked;
the actual Coder completion must establish and satisfy its plan before finishing.

`web/core/work-loop.js` owns browser-native task state under `spiral-coder.work.v1.<thread>`. Request metadata selects the server ledger `.spiral-coder/web-budgets/<task-id>.json`; changing the limit/root or reloading cannot refund wire attempts. Native history and assist IDs survive reload. A stale revision or previously consumed suggestion cannot be executed again.

Ordinary Coder chat, streaming chat and its HTTP fallback retain the completed
conversation from the Coder pane. This plain-text conversation is separate from
native execution evidence: a new human task starts with empty native history,
and only a continuation restores that task's native calls and results.

Visible thread history still debounces ordinary saves, but `pagehide` flushes
the latest snapshot and selected thread before navigation/reload. This keeps
the conversation reachable when native receipts were already saved but the
350 ms display-save timer had not fired. It does not promise durability after
a browser/process crash that prevents lifecycle events or a failed storage write.

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

Use repeatable `--case CASE_ID` flags to rerun only selected cases. Selection
keeps suite order and ignores repeated IDs; omitting the flag runs the full suite.
Unknown IDs fail before output creation or model execution. `metadata.json` and
`summary.json` both record `selected_case_ids`, including for unfiltered runs.
The saved spec remains the full original suite, with its original hash and caps.
Always choose a fresh output directory to preserve earlier receipts. For example:

```sh
python3 -S scripts/run_coupled_pilot.py \
  --binary target/debug/spiral-coder --repo . --out .tmp/pilot-selected \
  --provider mistral --model mistral-small-2603 \
  --case coupled-approved-benchmark-plan-tui-replay-smoke
```

Deterministic tests, mock HTTP/UI exercises, and bounded live results are separate evidence classes. See [the current evaluation record](evals/2026-10-14-baseline-and-review/README.md), [repair-continuation rounds](evals/2026-10-14-repair-continuation/README.md), [task-obligation rounds](evals/2026-10-14-task-obligations/README.md), and [earlier rounds](evals/2026-10-14-coupled-loop/README.md) for candidate hashes, fixed caps, failure ledgers, and limitations.

The [first real-repository dogfood task](evals/2026-10-19-real-repo-dogfood/README.md)
uses a separate frozen prompt and CLI oracle. Its nine live episodes do not pass
the task; the manually completed integration is recorded separately.
# Current evidence and authored closeout

The shared task projection supplies stable `acceptance N` references and current
commands to both roles. A malformed completion payload stays pending so the
Coder can correct the mapping without rerunning passed checks. The runtime no
longer reconstructs an authored payload by guessing which check proves each
criterion. Full shell commands must match recorded evidence, including case and
redirects. These identities establish provenance; they do not judge the semantic
sufficiency of every human-language claim.

Each Coder request also binds the `done` schema to the current plan's stable
criterion references and currently citable commands. Object-shaped evidence uses
separate reference/command choices; Google's flattened schema offers complete
`acceptance N | exact command` rows. The model still chooses which criteria are
complete and which receipt supports each one. No missing report claim is filled in,
and existing baseline, freshness, completeness and authored-summary gates remain
authoritative. Empty or oversized choice sets retain the generic schema rather
than truncating commands or emitting an invalid empty enum. Choices are rebuilt
on every request and are not persistent state.

While declared checks remain pending, both roles receive a compact `repair_brief`
with human-declared source paths, correlated read bounds/freshness, and the latest
successful native edit identity. It reminds the model to trace failing assertions
to existing source branches, preserve working defaults/guards, and avoid copying
test-local bookkeeping or inventing dependencies. It neither guesses a repair nor
requires a new read when retained observations suffice. This is advisory context,
not a semantic correctness gate or permission to edit tests. Native proof and an
independent artifact check remain necessary to assess an attempted repair.
`recent_check_failures` retains up to two consecutive failures of the latest
pending exact check, in native result order, with call IDs and observed unittest
case names. Automatic checks require the configured check identity; rejected,
unmatched and prose-only results cannot enter this projection. A successful check
separates failure sequences. Changed failure names do not prove an earlier case
was fixed. Compact Python diagnostics retain the failing case, innermost frame
and exception through history pruning and resume.

The `done.summary` field description includes the same current report clauses
and before/after rows as the shared task projection, formatted as plain text
instructions. A command cited in an
evidence row does not replace the requested report text. Unknown or invalidated
outcomes remain `NOT RECORDED` / `NOT VERIFIED`; no summary is generated. An exact
leading echo of the current report-context object (optionally fenced) is removed
before validation and display. Only the remaining authored prose can satisfy the
report gate. Other JSON, stale context, ambiguous objects and surrounding authored
text are preserved. Original native arguments remain unchanged in history.
Exact outcome rows can also follow the bounded labels `Recorded outcomes:` or
`Verification results:`, or `Verified with <exact command> before and after edits:`
at a sentence/line boundary. This accepts authored fields in a paragraph without
rewriting native calls or creating report rows. Arbitrary colon prefixes, negative
qualifiers, stale commands, missing outcomes and unrendered fields still fail.

Protocol blocks (`plan`, `think`, `reflect`, `impact`, `evidence`) may also contain
one authored JSON object, optionally surrounded by prose or Markdown fences.
The existing field catalog defines aliases and types. String arrays preserve
their items; text fields can join authored string arrays; a positive integer field
can parse an exact decimal string. Unknown metadata is discarded. Missing fields,
wrong types, duplicate keys or aliases, competing objects, truncated JSON and
excess criteria are not repaired. A JSON fixture inside an existing text/XML
field stays field content. Existing count, enum, task alignment, tool matching and
proof validation remain authoritative. Native tool arguments are never rewritten
by this protocol adapter.

Observer uses the same bounded (64 KiB), duplicate-rejecting object extractor.
An explicitly authored `suggested_args` JSON string can be decoded once into an
object; its tool name, argument schema and current evidence still must validate.
Missing actions, tool names, confidence or proof are not invented. Neither format
normalization nor an Observer suggestion executes a native call. Original Coder
text and native arguments remain in its transcript. Successful TUI Observer
answers retain `Message.raw_content` before display formatting; coupled-run reports
include it beside formatted `observer_display` content. Rejected answers retain
their original display text. Raw text is not added to provider history or used as
proof. There is no additional model call for deterministic recovery.

After a protocol refusal, the first exact human-declared baseline check may run
without an unrelated diagnostic detour. This narrow recovery exception requires
no prior mutation, no previous native attempt of that check, and no pending
localized failure. It cannot unlock a retry of a failed check or manufacture a
baseline receipt; normal command approval and execution still apply.

Observer prompts prioritize current correlated native results and omit old tool
outputs and duplicate display digests. The selected failure is explicitly
historical context. Raw audit packets retain the full history for diagnosis.
This reduces contradictory input but cannot guarantee that a model will draw
the correct conclusion from the current evidence.

JSON reads expose bounded append recipes for a root array or top-level array
fields. Each recipe supplies a unique exact `search` and a prefix/suffix around
the insertion point; the Coder must supply the new JSON item and invoke the
existing `patch_file` tool. No item is generated or executed by the recipe.
Malformed candidates blocked by the existing preservation policy report the
candidate's error location and recipes for the unchanged original. Unknown
fields, nested evidence, and old assertions remain covered by the same complete
preservation check. Recipes are intentionally omitted where a bounded unique
anchor cannot be supplied. They are diagnostic text, not verification evidence.

## Report repair advice

The derived `task_obligations.report_repair` is present only when the latest
correlated native action is a rejected `done` with an authored-report error,
its current acceptance citations remain valid, no acceptance is left pending,
and no exact check, human requirement or diagnostic recovery remains pending.
It retains the rejected call ID and recomputed missing-report feedback. A later
native action or a new human task makes that narrow state inapplicable. Quoted
errors and browser packet fields cannot create it.

Both roles receive this projection. The Observer prompt labels the selected
older failure separately and uses the report feedback as its current blocker.
High-confidence tool advice that restarts reading, editing or verification is
withheld for the existing one-attempt repair path. Corrected `done` advice must
still pass the shared acceptance and authored-report validators; uncertain or
no-tool clarification remains available without a confidence-forcing retry.
No final answer, passing result or completion is generated by this projection.

`coupled-run` places generated state and Python bytecode under its external
output directory; see [storage ownership](state-schema.md#coupled-run-storage).
This removes harness-created workspace changes without exempting them from the
benchmark's strict scope inventory. Normal interactive TUI storage is unchanged.

## Shared progress and action-level assistance

The [work-progress projection](observer-work-progress.md) exposes the original
human task separately from declared proof requirements, native edit activity,
repeated observations and authored acceptance claims to both roles. A docs/check
requirement does not stand in for the source behavior requested by the human.
[Observation pressure](observation-progress.md) bounds identical repeated reads
without forcing an edit or overriding required diagnosis.

Assist receipts retain their legacy continuation-wide execution fields and add
`suggestion_execution`: the proposed tool/arguments, exact matching native call
IDs, and their own execution/outcome. An accepted decision is not execution;
an overridden suggestion can still execute later in the continuation. Matching
is deliberately exact and conservative. Neither receipt establishes that the
Observer caused an improvement or that the requested repair passed independent
validation. See [state ownership and receipt semantics](state-schema.md).

[Successful command-output classification](success-output-classification.md)
ignores only complete passing Rust test rows when scanning for error markers.
Real errors elsewhere remain failures, including when a command exits zero;
preview/history shortening preserves this distinction across handoff.


### Bounded repair pools, Observer cadence and truthful stopping

For self-dogfood experiments, a 32-task-request run can use:

```text
--max-requests 32 --coder-chunk-requests 8 \
  --observer-reserved-requests 8 --transport-repair-requests 4
```

The maximum is 36 wire attempts: up to 32 task requests and four wholly rejected
framing/type responses. All attempts and their classification remain in the
ledger. HTTP errors, interrupted bodies, semantic errors and failed verification
are not free retries. Local opening-brace recovery costs no extra request. These
options default to zero reservation/repair credit for compatibility; a benchmark
must freeze them explicitly before calling providers.

The eight Observer-only task slots cannot be borrowed by Coder. Remaining work is
paced over those opportunities, normally three Coder requests then an Observer
intervention. A new failed check returns earlier. Stop on success, exhaustion or
an explicit blocked report instead of consuming calls just to reach a quota.

A blocked Coder uses native `done` with remaining criterion references, a concrete
blocker summary and `next_steps`. Completed criterion claims still need valid
native citations. Runtime retains all pending checks and diagnosis, records
`OK: blocked`, asks Observer for one no-tool diagnosis, and ends incomplete. This
path is not a successful completion and cannot qualify a candidate. Provider
stream failures are surfaced in the report rather than hidden behind a clean
process exit.

The `recovery-handoff-rhythm` benchmark is a new prompt variant: it gives the exact
single-test reproducer, semantic failure-boundary invariants, a required docs
edit, and the full native test command after the final mutation. Its visible test,
independent oracle, reference patch and original-regression floor are unchanged.
Do not pool it with earlier prompt variants as a runtime-only comparison.
Development milestones show reproduction, measured behavior change, visible
pass, control non-regression, fresh full native check, and docs/receipts; the final
qualification gate remains binary and independent.

### Unchanged edits and exact check selection

Both Coder and Observer receive an explicit `unchanged` outcome when a native
edit would write identical content. Such an operation leaves repair, failure,
verification and acceptance obligations intact. It does not trigger another
automatic test or claim an edit completed. The next action should address the
missing difference, or report a concrete blocker if no valid repair is available.

Required exact checks are available as `exec` selections such as
`{"check_id":"required-1"}`. Optional reproducer names can be declared in the
human task:

```text
<task_checks>
{"version":1,"checks":[{"id":"reproduce","command":"cargo test --locked regression_name -- --exact"}]}
</task_checks>
```

Selection runs the exact command at the task root through normal approval and
recovery. Supply only `check_id`; use full `command` when no selection is offered.
A selected check is still pending until its real execution succeeds, and edits
invalidate earlier verification as before. The optional reproducer is not an
extra completion requirement. See [state ownership and limits](state-schema.md).

### Explicit account exhaustion is not temporary throttling

The OpenAI-compatible chat and completions routes stop retrying HTTP 429 when
the structured `error.code` or `error.type` is `insufficient_quota` or
`credit_balance_exhausted`. Unknown 429s and transient server failures retain
their existing bounded backoff. Error-message prose is not used for this decision.
If a later retry cannot reserve a request, the error chain preserves both the
previous provider failure and the typed budget refusal.

This remains an interrupted, incomplete task. No HTTP failure receives transport
repair credit, no Observer response or completion is invented, and no request is
refunded. Restore provider availability or start a separately declared model
pair before another episode. The native Anthropic route is unchanged.
