# TUI Agent Split Plan

`src/tui/agent.rs` is the current orchestration center for the Coder loop.
That is useful for iteration speed, but it is no longer the right long-term
shape for enterprise-grade maintenance.

This document tracks the next extraction targets.

## Why split now

`src/tui/agent.rs` currently owns too many concerns:

- provider compatibility repair
- plan/think/done gate validation
- read-only rescue and tool-first coercion
- evidence scoring and `done` auto-finalize
- resolution memory integration
- runtime loop orchestration

Those concerns change at different rates and should not all compete in one file.

## Extraction targets

### 1. `src/tui/agent/done_gate.rs`

Move:

- `done` validation helpers
- acceptance/evidence canonicalization
- read-only auto-finalize logic
- evidence scoring and `read -> done` landing

Reason:

- this is a coherent policy surface
- it changes with eval feedback
- it has many direct tests and should own them

### 2. `src/tui/agent/read_only.rs`

Move:

- read-only task detection
- tool-first policy
- plan/think rescue for observation tasks
- task-specific search/read coercion

Reason:

- this logic is now a substantial policy subsystem
- it is provider-agnostic and should be testable in isolation

### 3. `src/tui/agent/provider_compat.rs`

Move:

- Mistral/OpenAI compatibility repair helpers
- malformed tool-call normalization
- synthetic plan/think repair for provider-specific drift

Reason:

- provider quirks should not be interleaved with core runtime logic
- this area is likely to grow as more providers are added

### 4. `src/tui/agent/memory.rs`

Move:

- resolution memory plumbing
- observation-backed canonical path helpers
- future evidence-memory and command-memory glue

Reason:

- typed memory is now a first-class subsystem
- it should stay auditable and not be buried in loop code

### 5. `src/tui/agent/loop.rs`

Keep as the orchestration entrypoint:

- drive the model/tool loop
- call gates, rescue, and compat helpers
- own turn sequencing and stop conditions

This file should become thinner over time and mostly coordinate modules.

### 6. `src/tui/agent/task_harness.rs`

Move:

- task-lane inference such as `fix` / `create_file` / `init_repo`
- artifact-shape hints that tell the runtime what “progress” looks like
- progress-gate policy for repeated observation loops in action tasks

Reason:

- this is typed runtime state, not provider compatibility
- it should stay auditable and testable without growing the main loop
- action-task harnessing is now a distinct policy surface with its own eval cases

### 7. `src/tui/agent/meta_harness.rs`

Move:

- trace-derived failure pattern detection
- typed policy deltas such as `mutate_now` / `create_artifact_now`
- runtime-updated harness prompts that tighten behavior after loops drift

Reason:

- this is the bridge between replay/eval evidence and live orchestration policy
- it should stay deterministic and auditable before any future evaluator-agent loop
- it lets the runtime change its own guardrails without growing `agent.rs`

### 8. `src/tui/agent/evaluator_loop.rs`

Move:

- deterministic `keep / problem / try_now` findings derived from live trace policy
- evaluator-side policy patching such as blocked repeated observations vs preferred tools
- evaluator prompt / telemetry that closes `trace -> finding -> runtime rule update`

Reason:

- this is the next layer above `MetaHarness`, not a provider compat concern
- it keeps the self-critique loop typed and auditable before any future multi-agent orchestration
- it lets the runtime rewrite its own harness behavior without stuffing more branches into `agent.rs`

### 9. `src/tui/agent/session_bridge.rs`

Move:

- prompt/telemetry rendering for persisted resume memory from `session.json`
- typed bridge summaries such as last good verification, accepted strategy, repeated dead-end
- resume-specific guidance that turns prior-session operational memory into live harness context

Reason:

- session persistence owns the data, but the live prompt/rendering layer should not grow `agent.rs`
- it keeps resumable operational memory explicit and auditable
- it is the foundation for deeper session-to-session context bridging later

### 10. `src/tui/agent/harness_evolution.rs`

Move:

- cross-session storage for runtime-only harness patch proposals
- overlay prompt rendering that turns deterministic failure findings into durable policy bias
- promotion-readiness bookkeeping plus eval-gated promoted overlay files for contract updates later

Reason:

- this is persistent policy memory, not ephemeral orchestration state
- it lets the runtime carry forward learned guardrails without editing the source contract on the fly
- it keeps the self-evolution loop explicit and testable outside `agent.rs`

## Suggested extraction order

1. `done_gate.rs`
2. `read_only.rs`
3. `provider_compat.rs`
4. `memory.rs`
5. `task_harness.rs`
6. `meta_harness.rs`
7. `evaluator_loop.rs`
8. `session_bridge.rs`
9. `harness_evolution.rs`
10. optional `telemetry.rs` if loop instrumentation keeps growing

This order minimizes risk because the first two already have strong eval/replay
pressure and the clearest ownership boundaries.

## Status: first wave complete, second wave landed

All first-wave targets above now exist as modules under `src/tui/agent/`
(`done_gate`, `read_only`, `provider_compat`, `memory`, `task_harness`,
`meta_harness`, `evaluator_loop`, `session_bridge`, `harness_evolution`, plus
many more). `agent.rs` remains the orchestration entrypoint planned as item 5.

Second-wave extractions completed:

- `src/tui/agent/tests.rs` — the large inline `#[cfg(test)] mod tests` block
  (~4.2k lines) now lives beside the other `agent/` test modules.
- `src/tui/agent/verification.rs` — `VerificationLevel`/`ExecKind`/`ErrorClass`,
  verification-requirement prompts, exec-kind/verify-level classification,
  error signatures, known-failure hints, goal-check order/runner/tracker, and
  exec-output parsing. `exec_may_mutate_for_evaluation` stays reachable at its
  previous `crate::tui::agent::` path via a `pub(crate) use` re-export.
- `src/tui/agent/git_guard.rs` — embedded-repo landmine detection
  (`should_block_git_landmines`), `.git` discovery helpers, and git checkpoint
  utilities (`git_create_checkpoint`, `run_git_cmd`).
- `src/tui/agent/governor_signals.rs` — text/command signature normalization
  (`command_sig`, `blocked_tool_call_signature`), blocked-tool fingerprinting,
  and missing `<plan>/<think>/<gate>` governor-block detection
  (`consecutive_missing_*` helpers).

These were extracted with `pub(super)` visibility and `use super::*;` so call
sites in `agent.rs` and sibling modules resolve unchanged.

## Third wave: sub-splitting the large extracted modules

The largest first-wave modules have been subdivided further:

- `src/tui/agent/task_harness.rs` (~5.0k lines -> ~400) keeps the `TaskLane` /
  `ArtifactMode` / `TaskHarness` core plus lane coercion glue, and now owns:
  - `task_harness/benchmark_plan.rs` — benchmark-plan approval detection,
    required-check bookkeeping, plan tool-call coercion/synthesis
  - `task_harness/fix_existing.rs` — fix-existing/repo-goal mutation coercion
    and repair, blocked-mutation validation, focus inference, observation
    history, literal member patch synthesis
  - `task_harness/scaffold_status.rs` — repo scaffold status inference and the
    next-scaffold-tool-call planner
  - `task_harness/tests.rs` + `task_harness/literal_list_recovery_tests.rs` —
    the inline test modules (~2.3k lines) moved beside the code they cover
- `src/tui/agent/done_gate.rs` (~2.7k lines -> ~830) keeps criterion/evidence
  scoring and acceptance validation, and now owns:
  - `done_gate/action_closeout.rs` — verified-action done preference, mutation
    path inference, repo scaffold artifact inference, and the verified-action
    auto-finalize/rescue/closeout surface
  - `done_gate/tests.rs` — the inline test module

Notes:

- Items that must stay reachable at the old `super::task_harness::X` /
  `super::done_gate::X` paths are `pub(crate)` in the new submodules and
  re-exported with `pub(super) use self::<module>::*`.
- `super::` paths inside moved code that referred to `agent`-level modules now
  use `super::super::`.

## Fourth wave: provider_compat/followup_requirements sub-splits and deeper agent.rs extraction

`provider_compat.rs` and `followup_requirements.rs` were subdivided:

- `provider_compat/mistral_tool_calls.rs` — Mistral name-embedded and nested
  payload normalization plus observation-tool classification
- `provider_compat/block_rescue.rs` — synthetic plan/think/impact/evidence
  block synthesis and missing-block rescue turns
- `provider_compat/tests.rs`
- `followup_requirements/transforms.rs` — followup patch synthesis and
  file-specific literal transforms
- `followup_requirements/evidence.rs` — followup satisfaction and successful
  literal mutation tracking
- `followup_requirements/tests.rs`

`agent.rs` itself dropped from ~13.4k to ~8.1k lines by extracting:

- `telemetry.rs` — autosave/telemetry emitters and blocked-tool exchange
- `repo_map_hints.rs` — repo-map fallback banners, hint builders, and
  search-file tool-call rewriting
- `realize.rs` — realize-on-demand preset/config/latent-plan machinery
  (`RealizePreset` stays reachable at its old path via `pub use`)
- `prompt_setup.rs` — tool schema defs, base system prompt, capability
  addons, and loop constants (`pub` tool defs re-exported)
- `output_compaction.rs` — truncation, token estimates, message pruning,
  exec output building, pwd/path helpers
- `protocol_state.rs` — typed block/state structs (`PlanBlock`, `ThinkBlock`,
  `ImpactBlock`, `EvidenceBlock`, `TaskContract`, instruction resolver,
  assumption ledger, `WorkingMemory`, `GoalDelta`, `StrategyChange`, ...)
- `state_prompts.rs` — working-memory/task-contract/verification prompt
  builders and plan validation
- `evidence_collect.rs` — observation evidence collection, evidence-block
  validation, acceptance reference resolution, successful-command collectors
- `block_parsers.rs` — tag block extraction, plan/think/reflection parsers,
  pseudo tool-call block text
- `plan_governor.rs` — plan adoption/validation, reflection/governor prompt
  state, local-action intent detection
- command canonicalization helpers were appended to `governor_signals.rs`

What remains in `agent.rs` is essentially the `run_agentic` /
`run_agentic_json` orchestration loop plus `AgenticStartState` /
`AgenticEndState` — the intended item-5 `loop.rs` shape.

## Fifth wave: loop-body consolidation started

`agent.rs` is now ~7.9k lines, almost all of it the `run_agentic_json`
iteration loop. The first inside-loop extraction landed:

- `governor_emit.rs` — `GovernorEmit` context struct +
  `emit_governor_block`, which absorbs the ~23 identical
  stream/record/autosave/hint/GovernorState sequences that were inlined at
  every governor-block callsite (~700 lines of duplication removed).

Next inside-loop targets: the ~430-line setup prologue (a `LoopSetup`
state struct), the ~110-line closeout epilogue, and per-iteration phase
helpers for the tool-dispatch block.

Landed since:

- `closeout.rs` — `Closeout` context struct + `run_closeout`, covering the
  post-iteration epilogue (verified-action auto-finalize, session-end
  latent-plan realization, outcome telemetry, `AgenticEndState` assembly).
- `loop_setup.rs` — `load_persisted_state` (reflection ledger, progress
  state, promoted harness overlay, harness evolution queue + telemetry),
  `create_or_reemit_checkpoint`, `inject_setup_context` (working-directory
  note, project context, AGENTS.md injection), and `has_system_prefix`.

- `loop_setup.rs` — `LoopSetup::init` also owns the rest of the setup
  prologue (~40 loop-scoped bindings: task contract inference, resume-state
  restoration, initial governor/telemetry emissions). `run_agentic_json`
  destructures the struct back into identical locals so the loop body is
  unchanged.
- `tool_dispatch.rs` — `ToolDispatchCtx` (borrowed loop state) + `ToolFlow`
  (continue/break mapping) + the apply_diff and observation-tool handlers
  (`run_apply_diff`, `run_list_dir`, `run_glob`, `run_search_files`) plus
  shared helpers (`governor_emit`, `complete_done`,
  `read_only_iter_cap_finalize`, `apply_diagnostic_transition`).
- `file_tool_dispatch.rs` — `run_file_tools` (read_file / write_file /
  patch_file incl. cache, approval, hash/auto-test, read-only finalize).
- `done_dispatch.rs` — `run_done` (focus-pending pre-gate plus all four
  done-gate blocks and the acceptance/final-handoff path).
- `exec_dispatch.rs` — `run_exec` (git landmines, repeat-command guard,
  approval, cwd tracking, proof recording, reflection trigger).

Remaining inside-loop targets: the pre-dispatch phases — provider
streaming/response parsing (~770 lines), reflection enforcement
(~1.7k lines), and the coercion/gate section between the done and
apply_diff dispatches.

Progress on the pre-dispatch phases:

- `turn_stream.rs` — `request_turn_stream` owns the wire-request spawn
  (work-budget scope, receipt cursor capture, abort-on-drop), the token
  drain incl. Mistral tool-call normalization, and the join/error mapping
  into `StreamedTurn` (~120 lines out of `agent_loop.rs`).
- `reflection_gate.rs` — `ReflectionGateCtx` (borrowed loop state) +
  `enforce_reflection` + `ReflectionGate` (pass/reject) own the full
  pre-dispatch reflection contract: `<reflect>` parse, compat rescue for a
  single-call turn, field validation, one-tool enforcement, rejection
  feedback, ledger remember/persist, assumption refutation, and strategy
  transitions (~240 lines out of `agent_loop.rs`).
- `plan_governor.rs` — `build_reflection_prompt_with_recent` composes the
  reflection system prompt with the two most recent reflection summaries.

Still inside the loop: the remaining prompt-assembly and response-parsing
phases around the stream call, and the coercion/gate section between the
done and apply_diff dispatches.

## Quality gate for each extraction

Each extraction should preserve:

- `cargo test -q ... tui::agent::tests::`
- relevant `runtime_eval` cases
- relevant `tui_replay` cases when Observer/soft-hint behavior is involved

Do not extract purely for aesthetics. Extract around ownership boundaries and
keep the tests close to the new module.
