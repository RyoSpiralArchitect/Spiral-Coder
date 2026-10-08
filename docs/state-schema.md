# State Schema

This document defines the main state surfaces in `spiral-coder` and which layer
owns each kind of data.

The goal is simple: new features should not invent "just one more place" to
store state without first choosing the correct owner.

## State layers

| Layer | Owner | Lifetime | Backing store | Examples |
|---|---|---|---|---|
| Provider/runtime config | `src/config.rs` | process / launch | CLI args + env | `PartialConfig`, `RunConfig` |
| Project-local TUI prefs | `src/tui/prefs.rs` | cross-session | `.spiral-coder/tui_prefs.json` | `TuiPrefs`, `PanePrefs`, `coder_realize_preset`, pane model/provider/mode |
| Session persistence | `src/agent_session.rs` | resumable run | `session.json` | `AgentSession`, `ObservationCache`, recent reflections, `SessionBridge` |
| Native message provenance | `src/task_origin.rs` | resumable run | optional `origin` in session messages | human task versus runtime continuation/feedback; stripped before provider requests |
| Unresolved edit attempts | `src/tui/agent/edit_failure.rs` | current human task | reconstructed from correlated transcript exchanges | failed edit count, repeated arguments, last successful edit reset anchor |
| Localized verification failure | `src/tui/agent/recovery_focus.rs` | current human task, resumable | optional local tool-result metadata | failed check, confirmed workspace target, bounded diagnostic and observation state |
| Project-local repo progress snapshot | `src/progress_state.rs` | cross-session | `.spiral-coder/progress.json` | current objective, completed artifacts, verified commands, repo-level progress bridge memory |
| Project-local reflection ledger | `src/reflection_ledger.rs` | cross-session | `.spiral-coder/reflection_ledger.json` | recurring wrong assumptions, next minimal actions, reflection counts |
| Project-local harness evolution queue | `src/tui/agent/harness_evolution.rs` | cross-session | `.spiral-coder/policy_patch_queue.json` | trace-derived runtime overlay proposals, seen/applied counts, promotion readiness |
| Project-local promoted governor overlay | `src/tui/agent/harness_evolution.rs` | cross-session | `.spiral-coder/governor_contract.overlay.json` | eval-gated promoted harness policies, green case IDs, stable overlay defaults |
| Project-local contract promotion candidate | `src/harness_promotion.rs` | generated artifact | `.spiral-coder/governor_contract.promotion.json` | UI-ready candidate list, patch previews, promotion decisions for `shared/governor_contract.json` |
| Project-local contract promotion review gate | `src/harness_gate.rs` | cross-session | `.spiral-coder/governor_contract.promotion_gate.json` | human review decisions like approved/held/applied, GUI/TUI gate state for source-contract updates |
| Runtime eval merge gate | `src/eval_merge_gate.rs` | generated artifact | `.tmp/runtime_eval_*/merge_gate.json` | merge readiness, rollback availability, promoted overlay paths, checkpoint status |
| Project-local merge gate review state | `src/merge_gate.rs` | cross-session | `.spiral-coder/runtime_eval.merge_gate_review.json` | human approve/hold decisions for latest runtime eval merge-gate cases, shared by TUI and GUI |
| Observer/Coder diagnostic contract | `src/observer/coder_diagnostic.rs` | per Observer run | API/formatted Observer output | `CoderDiagnostic`, `MutationAnchor`, required follow-ups, verification command, next Coder action |
| Observer benchmark plan contract | `src/observer/benchmark_plan.rs` | per Observer run | API/formatted Observer output | `BenchmarkPlan`, case id hint, lane, required checks, success criteria |
| Observer critique memory | `src/observer/memory.rs` | per Observer thread / API caller | request/response payload, Web thread state | `CritiqueMemory`, proposal recurrence counts, analyzer-stage recurring risk bias |
| In-memory orchestration state | `src/tui/app.rs` + `src/tui/agent/task_harness.rs` + `src/tui/agent/meta_harness.rs` + `src/tui/agent/evaluator_loop.rs` | live TUI session / live coder loop | memory only | `App`, `pending_auto_fix`, `TaskHarness`, `TaskLane`, `ArtifactMode`, `MetaHarness`, `FailurePattern`, `PolicyDelta`, `EvaluatorLoop`, `EvaluatorFinding`, `PolicyPatch` |
| Intent state | `src/tui/intent.rs` | live session, optionally persisted later | memory only today | `IntentAnchor`, `IntentUpdateKind`, normalized constraints/success criteria |
| Web message provenance | `web/app.js` + `web/core/state.js` | Web thread lifetime | browser-local thread state | optional `message.origin` (`user` or `runtime`) for human messages versus runtime handoffs |
| Web recovery attempt history | `web/app.js` + `web/observer/logic.js` | Web thread lifetime | browser-local thread messages | existing `metaKind=observer_next_action` and `metaTargetId` identify assistance already attempted for a Coder message |
| Replay/eval fixtures | `.spiral-coder/*.json` + `src/runtime_eval.rs` + `src/tui_replay.rs` | versioned test input/output | repo files + `.tmp/` artifacts | runtime eval spec, TUI replay spec, reports, file-existence/file-content checks |

## Current ownership map

The Web recovery assistant uses its persisted message metadata to avoid retrying
the same automatic next-action request after a page reload. A recorded Observer
assistant entry for the same Coder target counts as an attempt even when the
request failed, was stopped, or was interrupted. A new Coder target can trigger
new automatic assistance; the explicit next-action button can still retry an
existing target. No separate retry ledger is stored.

Web provider presets update the Chat/Coder routing fields supplied by that
preset. They preserve workspace, approval policy, explicit Observer routing,
and other runtime preferences. Connection checks and dialog focus are transient
UI state; a failed status refresh clears stale server capabilities and can be
retried with Refresh.

Explicit Web API keys live only in the three pane input states. Legacy
`apiKey`, `chatApiKey`, `codeApiKey`, and `observerApiKey` configuration fields
are excluded when loading and persisting configuration. A routing change
clears a pane's explicit key when its effective provider or base URL changes,
including a URL inherited from Chat. Independent Observer routing and its key
remain intact when their destination is unchanged. Cross-pane key fallback is
allowed only for the same provider and base URL; an independently configured
Observer key must never be used for a newly selected Chat/Coder destination.

### 1. Runtime/provider config

Code:

- `src/config.rs`

Owns:

- provider/model/base URL selection
- mode/persona defaults
- temperature/max_tokens/timeout
- provider-specific defaulting and normalization

`PartialConfig::resolve()` retains its launch-time authentication checks. TUI
startup and provider selection use `resolve_for_interactive_setup()`, which
performs the same structural validation but defers missing Mistral/Anthropic
credentials until send time so `/keys` remains accessible. Every TUI provider
request validates the destination pane before starting; a blocked send preserves
its draft. This adds no persisted credential or readiness state.

Restoring a different TUI provider/base URL or changing `/base_url` resolves the
new target's environment credential; an explicit key from the previous target
is not carried across. Unchanged provider/endpoint settings retain their explicit
key. The preferences JSON format remains unchanged and stores no credentials.

Should not own:

- live session memory
- replay artifacts
- intent drift state

### 2. Persistent TUI prefs

Code:

- `src/tui/prefs.rs`

File:

- `.spiral-coder/tui_prefs.json`

Owns:

- per-pane preferences that should survive restarts
- global TUI knobs like language, auto-observe, realize preset, right tab

Examples:

- `TuiPrefs`
- `PanePrefs`
- `coder_realize_preset`
- `ui_lang`
- pane `provider/mode/model/base_url/persona/temperature`

Should not own:

- evidence from tool use
- stuck-case history
- observer suggestions from the current live run

### 3. Session persistence

Code:

- `src/agent_session.rs`

Owns:

- resumable chat/tool history
- recent reflection summaries
- observation-backed memory that is useful across resumes
- typed resume bridge memory such as last good verification and repeated dead-ends

Examples:

- `AgentSession`
- `ObservationCache`
- `ObservationReadCache`
- `ObservationSearchCache`
- `ObservationResolutionCache`
- `SessionBridge`
- `SessionVerificationMemory`
- `SessionAcceptedStrategy`
- `SessionDeadEnd`

This is the right home for typed operational memory such as:

- canonical path resolution
- evidence-backed observations
- recent successful commands used for `done` citation
- accepted strategies that were already matched to successful follow-up actions
- repeated dead-end commands that should not be retried first after resume

Resume repair:

- `src/session_resume.rs` checks tool-call/result pairing before the saved
  transcript is sent to a provider again. Call IDs must be nonempty and unique
  within an assistant's tool-call group; every result must match a pending ID.
- A missing, duplicate, or mismatched result discards the entire still-pending
  exchange and its following tail, keeping earlier complete exchanges intact.
  An orphan result after a complete exchange is discarded without removing that
  completed exchange. Repair is idempotent: its output needs no second repair.
- If repair changes history, `AgentSession` rebuilds reflection summaries and
  the session bridge from the retained prefix so discarded tail metadata cannot
  supply verification or recovery hints. Valid sessions keep their seeded state.
- Repair changes history, not the filesystem: interrupted tools may have already
  produced side effects. A missing result is not evidence that an action failed.

Native task provenance:

- `src/task_origin.rs` selects the latest nonempty human request as the current
  task contract. CLI resume, implied tool receipts, and
  goal-check feedback carry `origin: "runtime"` and do not replace that request.
- A new explicit human message starts a new task scope. Untagged legacy user
  messages and unknown origin values retain their human meaning. Text prefixes
  do not infer provenance, so historical untagged runtime messages cannot be
  retroactively distinguished from genuine human requests.
- The optional field stays inside the existing session message array; the
  session version is unchanged. Provider serialization strips local provenance
  before sending Chat Completions messages. Provider roles are unchanged.
- Root task inference, final-answer/exact-content requirements, and benchmark
  command classification share the same selector. This preserves the existing
  gates; it does not add enforcement to completion paths that lack those gates.

Unresolved edit attempts:

- `src/tui/agent/edit_failure.rs` reconstructs failed `patch_file`, `write_file`,
  and `apply_diff` attempts from correlated tool calls/results in the current
  human task. It stores no separate persistent counter.
- Diagnostic reads neither add to nor clear edit failures. A successful edit
  clears them even if its subsequent automatic test fails. A successful shell
  command classified as an action also clears the retry state, permitting a
  different repair strategy; this classification is not proof of a correct edit.
  Diagnostic and configured verification commands do not clear it. Verification recovery
  is owned separately by `RecoveryGovernor`. Rejected or blocked edits are not
  counted as attempted failures.
- Two unresolved edit failures require reflection and a bounded strategy hint.
  Identical decoded arguments are identified explicitly. The hint requests a
  minimal anchor from current contents and preservation of surrounding fields;
  it does not rewrite files or certify success. On resume, unresolved edit attempts
  restore the Diagnose stage, preserving benchmark repair ownership.
- The latest successful reset exchange is protected during context pruning so
  retained older failures cannot reappear as unresolved after resume. The legacy
  telemetry name `file_tool_consec_failures` now reflects unresolved edit attempts.

Localized verification failures:

- `src/tui/agent/recovery_focus.rs` owns the target and cause of a localized
  automatic-test failure. A successful edit and its first runtime automatic-test
  failure status must be correlated before a focus can be created. Diagnostic
  text is evidence to inspect, not an instruction or permission to run a command.
- Unrelated successful diagnostics do not satisfy inspection of the failed
  target. A read of the target permits repair while retaining the failed check.
  A failed target read must permit broader diagnosis without declaring the
  original cause fixed. Initially unlocalizable failures retain generic recovery.
  When the original pending check fails again, its current diagnostic replaces
  the old cause. Without a confirmed current location, the previous path becomes
  `last_confirmed` context and exact-path inspection is no longer required.
- Successful edits do not certify repair. The original automatic test or its
  exact configured command must pass when launched from the workspace root before this pending
  failure is cleared. A different configured check or another working directory
  cannot discharge the original obligation.
  Unrelated verification cannot authorize automatic closeout or explicit done.
- Local tool-result metadata preserves validated failure context across a
  save/resume and context pruning, including after the file has changed. This
  raw metadata stays out of provider requests and does not grant execution proof;
  bounded diagnostic hints are intentionally visible to the model.
  New human task boundaries discard the previous task's recovery focus.
- Tool messages may carry `recovery_focus: {version: 1, state: {pending: ...}}`.
  Pending data contains `path`, `diagnostic`, `check`, `observation`
  (`awaiting_read`, `observed`, `repair_attempted`, `unavailable`, or `unlocalized`),
  and `location` (`current` or `last_confirmed`; absent defaults to `current`).
  Reads of a last-confirmed path cannot turn it into current location evidence.
  `pending: null` records a clear, not successful execution evidence. This optional field
  does not change the session container version.
  Legacy snapshots without `location` can refresh a stale cause from their
  correlated failed automatic-test result when the stored and configured check
  match; missing or mismatched check identity is not guessed.
- Localization is conservative: a diagnostic must identify the edited workspace
  path, or a pathless JSON parse position must match a bounded parse of the
  current edited JSON inside the canonical workspace. An unrelated failure
  remains generic; the last edited file alone is not evidence of its cause.

Autosave ordering:

- The active agent task owns transcript saves until it has joined. The CLI may
  save its initial state before spawning and the returned end state after joining.
- Cancellation waits for the aborted task to join and retains its latest autosave.
  A failed or canceled task must not be overwritten by the CLI's pre-round copy.
- JSON/graph exports after failure or cancellation use that last saved session
  and are labeled as an incomplete run. Without a readable session, these exports
  are explicitly skipped instead of publishing a stale pre-round transcript.
- Save comparison and session/progress writes share one lock. Change detection
  compares message content, root, checkpoint, cwd, observation cache, and progress
  context; message count is not a revision, because compaction can shorten history.
- Each file is replaced atomically, but session and repo-progress files are not a
  single crash-atomic transaction. Current transcript/tool evidence remains primary.

Context-window compaction lives in `src/tui/agent/message_window.rs`. It removes
complete assistant tool-call/result exchanges, including multi-tool turns, as a
unit. Recent results protect their matching calls. Failed, observational, or
incomplete exchanges and the latest structured anchors are retained even when
this exceeds the target window. Session history is currently compacted in place;
separating the full audit transcript from provider context is a future change.

### 3b. Project-local repo progress snapshot

Code:

- `src/progress_state.rs`

File:

- `.spiral-coder/progress.json`

Owns:

- repo-level current objective for the active task lane
- completed artifact paths that were actually created or patched
- verified commands that already succeeded for this repo
- lightweight repo-progress bridge memory that can be reused across future runs

Examples:

- `task_summary: "Fix the failing test with the smallest code change"`
- `current_objective: "fix src/lib.rs"`
- `completed_artifacts: ["src/lib.rs"]`
- `verified_commands: ["cargo test 2>&1"]`

Important rule:

- this layer is repo-local operational memory, not a replacement for the live transcript
- it should help the runtime resume work without restarting discovery
- if current tool output contradicts it, current evidence wins immediately

### 4. Project-local reflection ledger

Code:

- `src/reflection_ledger.rs`

File:

- `.spiral-coder/reflection_ledger.json`

Owns:

- recurring wrong assumptions that have already been refuted
- previously effective next minimal actions
- lightweight cross-session reflection counts

Examples:

- `"broad search was unnecessary" => "read src/tui/prefs.rs"`
- `"cargo test was necessary first" => "run targeted tests"`

This layer is intentionally bias-only memory:

- it should guide the next probe
- it should not be treated as proof
- it must yield to current tool output when contradicted

### 4b. Observer critique memory

Code:

- `src/observer/memory.rs`
- `src/observer/engine.rs`

Owns:

- per-thread Observer proposal recurrence counts
- proposal status escalation (`new`, `[UNRESOLVED]`, `[ESCALATED]`)
- analyzer-stage bias risks for findings that appear again

Examples:

- `CritiqueMemory`
- `proposal_counts`
- `Recurring unresolved Observer proposal`

This memory is intentionally narrow. It can make repeated critique louder and
convert recurrence into a risk, but current transcript/tool evidence remains the
source of truth.

### 5. In-memory orchestration state

Code:

- `src/tui/app.rs`
- `src/tui/agent/task_harness.rs`
- `src/tui/agent/meta_harness.rs`
- `src/tui/agent/evaluator_loop.rs`

Owns:

- live UI state
- transient task handles
- pending one-shot actions between panes

Examples:

- `pending_auto_fix`
- `pending_observer_hint`
- `last_observer_suggestion`
- `coder_realize_state`
- running task handles
- `TaskHarness`
- `TaskLane` (`benchmark_plan` is used for approved Observer benchmark handoffs)
- `ArtifactMode`
- `MetaHarness`
- `FailurePattern`
- `PolicyDelta`
- `EvaluatorLoop`
- `EvaluatorFinding`
- `PolicyPatch`

This layer should stay transient. If a field must survive restart/resume, it
likely belongs in prefs or session persistence instead.

### 5a. Observer/Coder diagnostic contract

Code:

- `src/observer/coder_diagnostic.rs`
- `src/observer/engine.rs`

Owns:

- per-run Observer diagnosis that is safe to display in TUI/GUI
- the Coder-facing mutation anchor chosen from observed edits
- required docs/replay/runtime-eval follow-up paths
- suggested verification command and final-handoff literals
- one concrete next Coder action (`read_file` or `exec`) derived from evidence
- full-proposal diagnostic input before the UI trims displayed proposal cards

Examples:

- `CoderDiagnostic`
- `MutationAnchor`
- `RequiredFollowup`
- `CoderAction`

This is a transient typed contract, not persistent memory. If a diagnostic needs
to affect later runs, promote the underlying rule through the harness evolution
queue or store resumable facts in `AgentSession`.

### 5b. Observer benchmark plan contract

Code:

- `src/observer/benchmark_plan.rs`
- `src/observer/engine.rs`

Owns:

- the next smallest deterministic regression suggested by Observer output
- benchmark lane selection (`runtime_eval`, `tui_replay`, `observer_unit`, etc.)
- case id hints, target files, required checks, and success criteria
- trigger evidence such as recurring findings or missing follow-up diagnostics

Examples:

- `BenchmarkPlan`
- `case_id_hint`
- `required_checks`
- `success_criteria`

This is a planning contract, not an executable command queue by itself. When a
human approves the TUI/GUI handoff, the Coder receives
`<observer_benchmark_plan>` and `TaskHarness` classifies it as the
`benchmark_plan` lane, which focuses runtime-eval and TUI-replay plans onto the
matching `.spiral-coder/*.json` spec before mutation.

### 5c. Project-local harness evolution queue

Code:

- `src/tui/agent/harness_evolution.rs`

File:

- `.spiral-coder/policy_patch_queue.json`

Owns:

- trace-derived runtime policy overlay proposals that are not yet promoted into the source contract
- per-policy seen/applied counts and promotion readiness
- cross-session memory of which deterministic harness patches are repeatedly paying off

Examples:

- `fix_existing_files::force_mutation_after_observation_loop`
- `init_repo::advance_repo_scaffold_artifact`

Important rule:

- this layer may bias the live runtime with overlay prompts
- it must not directly rewrite `shared/governor_contract.json` during a normal run
- promotion into a source contract should stay gated by replay/eval health

### 5d. Project-local promoted governor overlay

Code:

- `src/tui/agent/harness_evolution.rs`

File:

- `.spiral-coder/governor_contract.overlay.json`

Owns:

- harness policies that already passed replay/eval gating
- stable per-lane defaults that should load before the next live run starts drifting
- green eval case IDs that justify each promoted overlay rule

Important rule:

- this layer is stronger than the raw patch queue, but still weaker than current contradictory tool output
- it is the bridge between runtime-learned policy and eventual source-contract promotion

### 5e. Project-local contract promotion candidate

Code:

- `src/harness_promotion.rs`

File:

- `.spiral-coder/governor_contract.promotion.json`

Owns:

- the reviewable candidate artifact that maps promoted overlays onto `shared/governor_contract.json`
- UI/TUI-friendly display cards, decision states, and patch previews
- the last generated promotion snapshot for humans or future GUI/TUI approval flows

Important rule:

- this file is candidate output, not live runtime policy
- it may be regenerated at any time from the promoted overlay plus the current source contract

### 5f. Project-local contract promotion review gate

Code:

- `src/harness_gate.rs`

File:

- `.spiral-coder/governor_contract.promotion_gate.json`

Owns:

- human review decisions for source-contract promotion candidates
- the durable gate between "candidate exists" and "write `shared/governor_contract.json`"
- GUI/TUI audit state such as approved, held, and applied timestamps

Important rule:

- this file may authorize source-contract updates, but it does not replace the candidate artifact itself
- runtime overlays and promotion candidates should remain derivable even if this review gate is reset

### 5g. Runtime eval merge gate

Code:

- `src/eval_merge_gate.rs`

File:

- `.tmp/runtime_eval_*/merge_gate.json`

Owns:

- whether an eval run is merge-ready from the runtime report summary
- per-case rollback status and the checkpoint hash that would make rollback possible
- promoted overlay artifact paths produced by passing eval cases
- a compact machine-readable bridge from "eval passed" to "safe to promote/merge"

Important rules:

- this file is generated evidence, not persistent repo state
- rollback commands are advisory and human-gated; the eval runner must not run destructive restore commands by itself
- copied eval tool roots under `.tmp/` must not create checkpoint commits in the parent repo

### 5h. Project-local merge gate review state

Code:

- `src/merge_gate.rs`
- `src/tui/merge_gate.rs`
- `src/server.rs` + `web/app.js`

File:

- `.spiral-coder/runtime_eval.merge_gate_review.json`

Owns:

- human approve/hold decisions for cases in the latest runtime eval merge gate
- GUI/TUI board state derived from `.tmp/runtime_eval_*/merge_gate.json`
- rollback preview visibility without executing destructive rollback commands

Important rules:

- approvals are scoped to both case id and gate path so stale eval runs do not carry approval forward accidentally
- failed cases can be held, but only passing cases can be approved
- rollback commands stay preview/copy-only in GUI/TUI until a separate destructive rollback flow is explicitly introduced

### 6. Intent state

Code:

- `src/tui/intent.rs`

Owns:

- normalized user intent, not raw conversation text
- scope-preserving updates such as `Replace`, `Refine`, `Continue`,
  `VagueModifier`

Examples:

- `IntentAnchor`
- `IntentUpdateKind`
- normalized `constraints`
- normalized `success_criteria`
- `optimization_hints`

Important rule:

- vague modifiers may refine quality but must not widen `goal` or `target`

Web thread messages may include `origin: "user" | "runtime"`. Runtime-generated
continuations and handoffs use `runtime`, so the current human intent can be
selected without mistaking an injected continuation for a new human request.
Legacy messages without `origin` default to `user` for backward compatibility.
This field records provenance in the Web thread; it does not grant instruction
authority or change the provider message role.

### 7. Replay and eval fixtures

Code:

- `src/runtime_eval.rs`
- `src/tui_replay.rs`

Files:

- `.spiral-coder/runtime_eval.json`
- `.spiral-coder/tui_replay.json`
- `.tmp/runtime_eval_*`
- `.tmp/runtime_eval_*/merge_gate.json`
- `.tmp/tui_replay_*`

Owns:

- repeatable behavior probes
- diagnostics and artifact capture
- per-case copied worktrees for mutation-oriented eval runs
- merge-gate reports that connect pass/fail outcomes to rollback and promotion evidence
- proof checks such as `verified_command_seen` and `auto_test_passed`, which keep eval cases accountable to actual verification evidence
- quality gates for changes to agent behavior

This layer should never become a substitute for runtime state. It is the place
to measure behavior, not to drive live orchestration.

Runtime eval cases may seed `session.json` when a regression only appears after
resume. Keep those seed sessions small, typed, and reviewable.

Runtime eval merge gates are written next to `report.json`. They should be used
as closeout evidence for self-dogfood changes, but they should remain disposable
and reproducible from a fresh eval run.

## Rules for adding new state

Before adding a field, answer these questions:

1. Is it user preference, resumable operational memory, or live transient state?
2. Does it need auditability or cross-session persistence?
3. Is it derived from observed tool results, or is it inferred policy?
4. Which existing struct should own it?

If the answer is not clear, document it here first.

## Immediate follow-ups

- Move more evidence-backed memory behind `ObservationCache` rather than ad hoc
  in `App`.
- Keep the reflection ledger project-local and bias-oriented; do not let it
  silently override current evidence.
- Keep `IntentAnchor` memory-first for now; only persist it after replay/eval
  proves the shape is stable.
- Keep replay/eval specs versioned and human-editable.

## Web server capabilities

`GET /api/status` owns explicit `features.harness_promotions`,
`features.merge_gate`, and `features.project_scan` booleans. The Rust server
provides these capabilities; the Lite server reports them as unavailable.
The Web UI waits for status before polling optional endpoints and shows an
explanation for unavailable review panels rather than an HTTP error.
