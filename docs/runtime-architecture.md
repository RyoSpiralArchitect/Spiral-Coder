# Spiral-Coder Runtime Architecture (WIP)

Spiral-Coder is not "a chat app that sometimes runs commands".
It is a **controlled execution runtime** for LLMs, with human gates and a safety governor.

This document describes the target structure we are converging on:

```text
CLI
 │
 ▼
Session Manager
 │
 ▼
Task Graph
 │
 ▼
Agent Loop
 │
 ▼
Safety Governor
 │
 ▼
Tool Router
 │
 ▼
Tools (exec / file ops / approvals / ...)
```

The point of the layering is to make failures legible and prevent the classic agent drift:
- repeating the same command
- nested git repo disasters
- "Done" without verification
- phase drift (polish advice during core failures)
- approvals and actions getting entangled

The new rule of thumb is:
- runtime overlays learn quickly
- promoted overlays require green eval proof
- source-contract promotion happens through a reviewable candidate artifact, not a silent live rewrite

Related docs:

- `docs/state-schema.md` — typed state ownership and persistence boundaries
- `docs/tui-agent-split-plan.md` — extraction plan for `src/tui/agent.rs`
- `AGENTS.md` — contributor replay/eval policy and high-touch file rules

## Mapping To Current Code (2026-03)

The runtime is already present, but parts are still "in one file". This mapping makes it explicit.

### 1) CLI

- `src/main.rs` (subcommands: `agent`, `tui`, `serve`, `review`, ...)
- `src/tui/mod.rs` (TUI entrypoint, tool_root isolation)
- `web/app.js` (Web UI, longrun agent loop client-side)

### 2) Session Manager

Responsibilities:
- load/resume session
- atomic save + crash resume
- track tool_root / checkpoint / cwd
- (optionally) trace output
- refuse git checkpoint commits when an eval sandbox is only a subdirectory of the real repo

Current code:
- `src/agent_session.rs` (`AgentSession`, `SessionAutoSaver`)
- `src/project.rs` (project scan: stack/git/test_cmd)
- `src/progress_state.rs` (`.spiral-coder/progress.json`, repo-level objective / artifact / verification snapshot used by the progress bridge)
- `src/tui/agent.rs::git_create_checkpoint` (checkpoint creation is limited to tool roots that are themselves the git top-level)

### 3) Task Graph

Responsibilities:
- represent "what work exists" and "what is done"
- allow routing to Coder vs Observer tasks
- persist across runs

Current code:
- TUI tasks tab + TaskRouter: `src/tui/app.rs`, `src/tui/events.rs`
- Web UI tasks list: `web/app.js` (thread tasks)

### 4) Agent Loop

Responsibilities:
- iterative model calls
- append messages (OpenAI tool-call format)
- execute tools and feed results back
- carry required docs/replay/runtime-eval follow-up edits before verification
- enrich verified final handoffs with requested artifact paths and non-command required literals the model omitted
- prefer the fixture/project configured verification command during automatic goal checks
- pause repeated-observation mutation pressure after a malformed file-tool patch so recovery can re-read the target snippet safely
- synthesize a narrow fix-existing mutation when a no-tool turn stalls after enough implementation evidence is already present
- stop conditions + goal checks

Current code:
- TUI/CLI agentic loop: `src/tui/agent.rs::run_agentic_json`
- Required follow-up edit coercion: `src/tui/agent/followup_requirements.rs`
- Final handoff artifact repair: `src/tui/agent/final_handoff.rs`
- Evaluator/meta-harness pressure gates: `src/tui/agent/evaluator_loop.rs`, `src/tui/agent/harness_evolution.rs`, `src/tui/agent/task_harness.rs`
- Streaming adapter: `src/streaming.rs`
- Web longrun loop: `web/app.js::runCoderAgentic`

### 4b) Observer Critique Layer

Responsibilities:
- summarize transcript/tool risk after the coder loop
- surface repo-rule follow-ups such as required docs or replay/eval proof
- emit a typed Coder diagnostic packet with mutation anchor, required follow-ups, verification command, final-handoff literals, and one concrete next action
- emit a typed benchmark plan packet with the next smallest regression lane, case id hint, checks, and success criteria
- expose the same diagnostic packet to TUI and GUI so humans can approve sending it back to the Coder inside explicit `<observer_...>` handoff tags
- route approved `<observer_benchmark_plan>` handoffs through the `benchmark_plan` Task Harness lane so runtime-eval and TUI-replay proposals first land on their matching `.spiral-coder/*.json` spec before patch/verify
- repair malformed benchmark-plan spec patches by synthesizing the smallest JSON update from `case_id_hint` and `src/...rs` evidence when the model drifts after reading the spec
- keep UI proposal truncation separate from diagnostic generation so lower-displayed but required follow-ups do not disappear from the Coder packet
- feed proposal recurrence memory into analyzer-stage risk generation so repeated unresolved findings become first-class risks, not just score bumps
- keep "next improvement" suggestions small enough to feed back into dogfood work

Current code:
- deterministic critique engine: `src/observer/engine.rs`
- Coder handoff diagnostic contract: `src/observer/coder_diagnostic.rs`
- Benchmark planning contract: `src/observer/benchmark_plan.rs`
- transcript/event analysis: `src/observer/detector.rs`, `src/observer/analyzer.rs`
- recurring critique memory: `src/observer/memory.rs`
- repo-rule follow-up heuristics: `src/observer/repo_rules.rs`
- TUI/GUI rendering: `src/tui/ui.rs`, `web/observer/logic.js`, `web/app.js`

### 5) Safety Governor

Responsibilities:
- stop repetition loops (same cmd / same output / same error)
- classify failures to route recovery strategy
- detect suspicious "success" (exit=0 but error markers)
- sandbox constraints (cwd/tool_root)
- phase gating (core/feature/polish)

Current code:
- `src/tui/agent.rs` (AgentState + FailureMemory + error classifier + stuck hints)
- `src/exec.rs` (dangerous command checks, cwd validation)
- `web/core/exec.js` (bash→PowerShell normalization, dangerous command guard)
- `web/app.js` (loop governor + goal_check probes + recent-runs memory)

Command classification:

- `src/tui/agent/exec_classification.rs` recognizes command words rather than
  lowercased/truncated substrings. A filename such as `verification_receipt.txt`
  cannot match the diagnostic `cat` command, and quoted argument text cannot
  turn `echo` into `cargo test` verification.
- The supported shell subset is simple commands, literal quotes, leading
  environment assignments, `env` assignments, `cd` prefixes, `&&` chains, and
  stderr-to-stdout `2>&1`. Every element of an inferred verification chain must
  be a recognized diagnostic or verification command. Git branch/remote queries
  and numeric `sed -n` print ranges have restricted read-only forms.
- File redirects, substitutions, multiline scripts, pipelines, status masking,
  and unknown compound commands are conservatively actions and do not earn
  inferred verification credit. This is not a shell parser or sandbox; unsupported
  benign commands may require an additional explicit verification step.
- An exact configured test command can authorize custom scripts and `&&`
  chains. Matching preserves case and quoted whitespace. It does
  not override unsupported shell syntax or known write flags. Approved benchmark
  `required_checks` retain their separate explicit command-evidence contract.

- `src/tui/agent/exec_proof.rs` shares generic verification timestamp accounting
  between live exec results and resumed transcripts. Any possibly executed action,
  including failure or timeout after partial writes, advances the mutation step.
  An executed verification failure clears both prior verification levels, so an
  older behavioral success cannot mask a newer failed build. Explicit pre-execution
  blocks and user rejections preserve evidence. Resume accepts only the strict
  runtime success header (including the pruning suffix); live execution uses its
  effective return status. A fresh applicable check restores verification credit.

### 5b) Harness Evolution Overlay

Responsibilities:
- persist trace-derived runtime policy overlays outside the ephemeral loop
- feed deterministic `MetaHarness` / `EvaluatorLoop` findings back into later turns
- keep "overlay first, source-contract later" promotion boundaries explicit
- connect green eval runs to merge-ready evidence and failed runs to rollback evidence

Current code:
- `src/tui/agent/harness_evolution.rs` (`ContractPatchProposal`, `HarnessEvolutionQueue`, runtime overlay prompt)
- `.spiral-coder/policy_patch_queue.json` (project-local overlay queue)
- `src/tui/agent.rs` (load/save wiring, telemetry, prompt injection)
- `.spiral-coder/governor_contract.overlay.json` (eval-gated promoted overlay rules)
- `src/main.rs::run_eval` (promotion step from passing eval case to promoted overlay)
- `src/eval_merge_gate.rs` + `.tmp/runtime_eval_*/merge_gate.json` (generated merge readiness / rollback / promoted-overlay evidence)
- `src/main.rs::run_merge_gate` / `spiral-coder merge-gate` (CLI reader for merge readiness, CI status, and rollback previews)
- `src/merge_gate.rs` + `.spiral-coder/runtime_eval.merge_gate_review.json` (human approve / hold review state layered over the latest generated merge gate)
- `src/runtime_eval.rs` (benchmark reports now include agent config plus approximate transcript token telemetry for dogfood/example docs)
- Runtime evaluation completion, command freshness, auto-test provenance, exact artifact checks, and promotion prerequisites are defined in [evaluation.md](evaluation.md). New reports carry `metrics.evaluator_revision: "outcome-proof-v2"`; historical evidence remains unchanged.
- `src/runtime_eval.rs` checks can assert copied tool-root files exist and contain expected literals, so regression specs can prove real artifact mutation instead of relying only on the final assistant text
- `src/runtime_eval.rs` checks can require proof-level verification with `verified_command_seen` and `auto_test_passed`, allowing benchmark-plan cases to distinguish artifact mutation from verified PR-ready closeout
- approved benchmark-plan eval fixtures now cover single-spec updates and compound docs+spec updates, exercising PR-ready artifact sets rather than isolated file edits only
- `src/tui/agent/benchmark_proof.rs` owns the approved plan's command evidence: every distinct `required_checks` item needs a successful, tool-call-correlated `exec` result before closeout. The harness selects the first pending item in plan order. Command case, quoting, and internal whitespace are significant; only surrounding whitespace and Markdown backticks around plan entries are ignored.
- Successful `write_file`, `patch_file`, and `apply_diff` results invalidate earlier benchmark-plan command evidence, including edits whose appended auto-test fails. Attempts to execute commands outside the approved `required_checks` also invalidate all prior checks when the runtime classifies them as `ExecKind::Action` (for example, `sed -i`). Nonzero exits and timeouts may follow partial writes, so only explicitly blocked or user-rejected actions retain earlier evidence. Benchmark gates and eval reports use the same `execution_evidence::exec_may_have_run` predicate. Explicit required checks remain verification commands even when they use custom scripts. Diagnostic commands and additional recognized verification commands retain prior proof. A failed rerun revokes that required command's earlier success. Proof comes from the runtime's exit-status header, not command output or final-answer claims; external edits and shell mutations misclassified by the runtime remain outside this transcript gate's coverage.
- The compound docs+spec benchmark-plan eval lists and asserts both verification commands separately, so success on the first command cannot satisfy the second requirement.
- Output pruning may append the runtime's `[pruned NL]` suffix to a successful status header; this retains the original command proof. Message-window compaction preserves the active plan's verification exchanges, file-mutation exchanges, and action-exec exchanges together, so trimming history cannot erase success or revive success invalidated by a later edit. The context limit remains a soft cap for these protected exchanges.
- `approved-benchmark-plan-exec-proof-resume` seeds two successful required checks, a later successful shell edit, and a rerun of only the first check. The second check must run again and refresh a stale receipt in the copied fixture. This bounded runtime eval complements deterministic proof/compaction regressions; its seeded transcript is test input, not a record of live model performance.
- `src/tui/agent/benchmark_replay.rs` generates typed TUI replay cases with seeded coder messages, a recorded Observer response, and checks for suggestion parsing and the queued Coder hint. The generated cases run through the actual replay parser and runner in offline tests. This verifies handoff plumbing for the named path; it does not establish live-model performance or test the named source file's behavior.
- The TUI benchmark-plan smoke now runs the generated case using `spiral-coder --provider openai tui-replay --spec .spiral-coder/tui_replay.json --filter review-panel-replay-sensitive`, in addition to the path check. The `spiral-coder` binary must be on PATH when launching that runtime eval; a development run can prepend its `target/debug` directory. The replay command itself uses the recorded response and makes no provider call.
- `src/harness_promotion.rs` + `spiral-coder promote-harness` (reviewable promotion candidate artifact for GUI/TUI or human approval)
- `src/harness_gate.rs` + `.spiral-coder/governor_contract.promotion_gate.json` (human-gated approve / hold / apply-to-contract state shared by TUI and GUI)
- `src/server.rs` + `web/app.js` + `src/tui/promotion_gate.rs` + `src/tui/merge_gate.rs` (review surfaces that consume the same board artifacts and gate files)

### 6) Tool Router

Responsibilities:
- map tool names to tool implementations
- enforce sandbox + approvals
- normalize OS-specific command behavior

Current code:
- `src/tui/agent.rs` (routes tool_calls to exec / read/write/patch/apply_diff / glob / search)
- `src/exec.rs` (exec runner)
- `src/file_tools.rs` (file tools)
- `src/approvals.rs` + `src/pending_*` (approval gating)

### 7) Tools

Current code:
- exec: `src/exec.rs`
- files: `src/file_tools.rs`
- approvals: `src/approvals.rs`, `src/pending_commands.rs`, `src/pending_edits.rs`

## Next Refactor Milestones

1. Introduce `src/engine/` module to make the layers explicit in code.
2. Move session + trace concerns behind a single `SessionManager` API.
3. Promote TaskRouter outputs into a DAG (dependencies + statuses), persisted into the session.
4. Formalize governor decisions as structured events (so UI can visualize "why we stopped").
5. Keep tools stable (their safety properties are the foundation), improve orchestration above them.
