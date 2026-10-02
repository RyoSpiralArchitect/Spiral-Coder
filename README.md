# Spiral-Coder

![Rust](https://img.shields.io/badge/Rust-2021-orange?logo=rust)
![License](https://img.shields.io/badge/license-Apache%202.0-blue)
![UI](https://img.shields.io/badge/UI-web%20%2B%20TUI-2dd4bf)

> **One prompt box is not enough.**
> Spiral-Coder gives your AI a second brain — and makes them argue.

Languages: [English](README.md) | [日本語](README.ja.md) | [Français](README.fr.md)

---

Every AI coding tool has the same problem: the model that writes your code also reviews it.

That's not a review. That's a self-defense monologue.

Spiral-Coder fixes this by running Coder and Observer in **completely separate contexts**. The Observer does not share the Coder's live working context or hidden scratchpad. It critiques outputs, transcripts, and optional git diff snapshots after the fact. That's what keeps it honest.

---

## UI Preview

These screenshots come from the Playwright smoke flows that exercise the Web UI end-to-end.

| Overview | Harness review gate |
|---|---|
| ![Spiral-Coder web overview](docs/screenshots/web-overview.png) | ![Harness review flow](docs/screenshots/harness-review-flow.png) |

| Runtime approvals |
|---|
| ![Runtime approvals flow](docs/screenshots/runtime-approval-flow.png) |

---

## Dogfood Benchmarks

Spiral-Coder now keeps lightweight benchmark writeups for generated milestone repos without checking those repos into git.

- [Benchmark Examples](docs/examples/README.md)
- [Existing Repo Bugfix (Rust)](docs/examples/existing-repo-bugfix-rust.md)
- [Maze Game (Rust)](docs/examples/maze-game-rust.md)
- [Maze Game (pygame)](docs/examples/maze-game-pygame.md)

Each page records the eval prompt shape, provider/model, rerun command, runtime snapshot, and approximate transcript token usage from a green `runtime_eval` report.

---

## Current Benchmark Milestone

The current milestone is simple but important: Spiral-Coder can now cover both fresh-repo scaffolds and an existing-repo bugfix from `runtime_eval`, keep generated state out of git, and still document the result in a reproducible way.

- `maze-game-rust-repo` proves the scaffold lane can create a fresh Rust repo, keep gameplay logic in `src/lib.rs`, keep `src/main.rs` runnable, and close with a real `cargo test`.
- `maze-game-pygame-repo` proves the same closeout path works for a non-Rust repo, including a headless `pygame` verification command.
- `resume-session-bridge-fix` shows the runtime can resume an existing Rust bugfix with both seeded session memory and repo-local `.spiral-coder/progress.json` memory, then push the agent back toward the smallest safe patch.
- The medium-plus self-dogfood stretch case `self-fix-observer-repo-rules-review-panel` now passes end-to-end: the runtime patches `src/observer/repo_rules.rs`, carries the required `docs/runtime-architecture.md` and `.spiral-coder/tui_replay.json` follow-ups, and closes with the exact passing verification command in the final handoff.
- The PR-ready self-dogfood case `self-fix-pr-ready-runtime-followup` now passes end-to-end too: the runtime patches `src/tui/agent/followup_requirements.rs`, carries the required `docs/state-schema.md` and `.spiral-coder/runtime_eval.json` follow-ups, and repairs the final handoff when a verified artifact path would otherwise be omitted. Latest green run: `.tmp/runtime_eval_1777229175/report.json` (`tools=7`, `messages=22`, approx `3.85k` transcript tokens) with merge readiness captured in `.tmp/runtime_eval_1777229175/merge_gate.json`.
- The merge-approval self-dogfood case `self-fix-pr-ready-merge-approval` now exercises a fuller PR-ready change set: code fix, docs follow-up, runtime-eval follow-up, exact configured smoke command, and a final handoff that includes the required status label `PR-ready merge approved`. Latest green run: `.tmp/runtime_eval_1777491446/report.json` (`tools=9`, `messages=28`, approx `4.40k` transcript tokens) with merge readiness captured in `.tmp/runtime_eval_1777491446/merge_gate.json`.
- The rollback-guard stretch case `self-fix-merge-gate-rollback-guard` raises the self-dogfood difficulty again: after a source read, the runtime can synthesize the smallest safe mutation for a stalled no-tool turn, carry docs/runtime-eval follow-ups, and close with the required `rollback guard enforced` handoff. Latest green run: `.tmp/runtime_eval_1777493520/report.json` (`tools=6`, `messages=20`, approx `3.83k` transcript tokens) with merge readiness captured in `.tmp/runtime_eval_1777493520/merge_gate.json`.
- Runtime eval closeout now writes a generated `merge_gate.json` next to `report.json`; `spiral-coder merge-gate`, the TUI Merge tab, and the Web GUI merge-gate panel can inspect readiness, approve passing cases, hold cases, and copy rollback previews without executing destructive rollback.
- The benchmark reports now carry provider/model metadata plus approximate transcript token telemetry, which makes it easier to compare runs without pretending those numbers are billing-accurate.

This matters because it turns "the agent made something cool once" into "the runtime can reproduce a milestone case and explain how it did it."

---

## Next Growth Opportunities

The next stretch is less about making prettier demos and more about making the runtime trustworthy over longer work.

- **Long-running durability**: session resume is much better now, and repo-local progress snapshots are in place, but true multi-hour work still wants stronger context compaction, drift-triggered replanning, and more checkpoint-aware recovery.
- **Broader benchmark mix**: fresh repo scaffolds plus one existing-repo bugfix are a good start; the next useful cases are small web apps, medium refactors, and docs/config tasks where the right verification floor is easier to get wrong.
- **Merge-ready self-dogfood loops**: PR-ready fixture work now has branch/checkpoint safety, a generated merge gate, shared TUI/GUI human review state, and a green merge-approval handoff benchmark. The next useful step is replay/eval-backed promotion from approved gate state into an actual PR-ready merge workflow.
- **Stronger verification pressure**: passing one test command is good, but adversarial evaluator loops, rollback/checkpoint restore, and quality-oriented checks would raise the floor a lot.
- **Patch recovery under pressure**: malformed or truncated patches now stop overpowering the evaluator loop, but the next step is a stronger exact-snippet repair path so retries recover faster instead of relying on another model turn.
- **Better telemetry**: transcript token estimates are useful for comparison, but provider-native usage accounting and benchmark trend history would make optimization much more concrete.
- **Measured harness evolution**: the repo now has overlay, promotion candidate, and human-gated apply flow; the next step is learning which promoted rules actually improve future runs and which ones just add prompt weight.

---

## Why Spiral-Coder Exists

Most LLM tools optimize for conversation.
Spiral-Coder optimizes for controlled execution loops: separate roles, approval gates, and critique that accumulates instead of resetting every turn.

This is not a chat client.
It's a development control engine.

---

## Three Roles. Three Contexts. Zero Conflicts.

| Role | What it does | What it never does |
|---|---|---|
| **Coder** | Acts — files, shell commands, agentic loop (up to 12 steps), 5 built-in tools | Review or second-guess its own work |
| **Observer** | Critiques — scores every proposal, escalates what you ignore | Write code or share the Coder's live working context |
| **Chat** | Thinks with you — design, rubber duck, tradeoffs | Interrupt the execution loop |

Different roles. Different models if you want. Different contexts always.

---

## What Spiral-Coder Knows Before You Say Anything

When you set `tool_root`, Spiral-Coder automatically scans the project:

```
[Project Context — auto-detected]
stack: Rust, React (no bundler)
explore:
  - Rust: read Cargo.toml first, then src/lib.rs or src/main.rs, then tests/ or examples/ before editing.
git:   branch=main  modified=2  untracked=1
recent: "fix(observe): require all 4 blocks" · "feat(agent): error classifier"
tree:
  src/          12 files  (Rust source)
  web/           4 files  (JS/CSS)
  scripts/       8 files  (PowerShell)
key:  Cargo.toml · web/app.js · README.md
```

This context is injected into the Coder's system message **before your first prompt**. The Coder already knows the stack, current branch, modified files, directory layout, and a small stack-aware exploration recipe when you start typing.

In the TUI header you'll see a live badge: `▸ Rust · React · git:main`
In the Web UI, the stack label appears below the toolRoot field in Settings.

**Stack detection** — Spiral-Coder looks for manifest files:
- `Cargo.toml` → Rust
- `package.json` → Node / React / TypeScript (inspects deps)
- `pyproject.toml` / `requirements.txt` → Python
- `go.mod` → Go
- `pom.xml` → Java
- `build.gradle*`, `Gemfile`, `composer.json`, `mix.exs`, `Package.swift`, `build.zig`, `*.tf`, `CMakeLists.txt`, `*.sln` / `*.csproj`, `deno.json*` → additional JVM / Ruby / PHP / Elixir / Swift / Zig / Terraform / C/C++ / .NET / Deno stacks

The scan runs once per session, takes under 200 ms, and silently skips anything it can't read.

### Deep Repo Map for Offline Code Navigation

Spiral-Coder now also ships a lightweight repo-map helper for codebase navigation and benchmarking:

```bash
python3 scripts/repo_map.py build --root .
python3 scripts/repo_map.py query "project context scan" --root .
python3 scripts/repo_map.py query "observer transcript critique" --root . --explain
python3 scripts/repo_map.py show --root . --file src/project.rs --symbol ProjectContext
python3 scripts/repo_map.py eval --root .
```

This is not wired into the runtime loop yet. It is a foundation layer for:
- better file/symbol targeting before full agent integration
- partial file loading instead of whole-file reads
- repeatable query benchmarks under `.spiral-coder/repo_map.eval.json`
- repo-local ranking cleanup via `.spiral-coderignore`
- score breakdowns and confidence/margin signals before runtime integration

The runtime now also detects when a repo-map is ready and can use it as a lazy fallback after a literal `search_files` miss or a `read_file` path miss, instead of paying the cost on every turn.

---

## What Makes Spiral-Coder Different

### The Observer Has No Skin in the Game

Other tools: same model writes code → same model reviews code → model defends its own choices.

Spiral-Coder: fresh context for every Observer run. The Observer doesn't know what it *would have* written, and it doesn't share the Coder's live scratchpad. It judges emitted outputs, transcripts, and optional diff snapshots after they exist.

Result: sharper feedback, honest risk assessment, no defensive hedging.

### Proposals Don't Disappear

When the Observer flags an issue and you don't act on it, the proposal escalates:

```
new  →  [UNRESOLVED] +10pts  →  [ESCALATED] +20pts, pinned to top
```

The Observer remembers what it said. If you ignore a `critical` warning twice, it becomes the loudest card on the board.

Repeated findings also become analyzer-visible risk. That means recurrence can influence `critical_path` and the deterministic `coder_diagnostic`, instead of living only as a UI score bump.

### Error Classification, Not Just Exit Codes

When a command fails, Spiral-Coder doesn't hand the model a raw `exit_code: 1` and hope for the best. It classifies the error first:

| Error type | Recovery hint injected |
|---|---|
| `ENVIRONMENT` | Fix the environment. Don't touch source code. |
| `SYNTAX` | Fix the exact file. Don't change unrelated code. |
| `PATH` | Verify paths first. Don't create until you confirm. |
| `DEPENDENCY` | Install the package first. Then retry. |
| `NETWORK` | Check service status and proxy vars. |
| `LOGIC` | Re-read the logic. Don't just re-run. |

PowerShell caveat: `exit_code` can be `0` even when it printed errors (non-terminating error records).
Spiral-Coder flags this as `SUSPICIOUS_SUCCESS` and treats it as failure to stop false-progress drift.

### The Coder Has Five Tools

The Coder isn't limited to shell commands. It has five purpose-built tools:

| Tool | When to use it |
|---|---|
| `exec(command, cwd?)` | Build, test, git, install packages — anything shell-based |
| `read_file(path)` | Read exact file content without shell quoting issues |
| `write_file(path, content)` | Atomically create or overwrite a file (parent dirs auto-created) |
| `patch_file(path, search, replace)` | Replace an exact snippet — fails loudly on ambiguity |
| `apply_diff(path, diff)` | Apply a unified `@@` diff (multiple hunks) — best for larger edits when `patch_file` is too small |

`write_file`, `patch_file`, and `apply_diff` use a temp-file → rename pattern, so a crash mid-write never leaves corrupt output.

`patch_file` requires the search string to appear **exactly once**. If it appears zero times, you get a preview of the file so the model can self-correct. If it appears more than once, you get the count. Ambiguity is an error, not a guess.

**Visual markers in the TUI** show which tool fired at a glance:
- `📄 READ` (teal) — file was read
- `✎ WRITE` (blue) — file was created or overwritten
- `⟳ PATCH` (magenta) — exact snippet replaced
- `✓` (green) / `✗` (red) — result OK or error

### The Coder Doubts Itself

Before the first real tool call, the Coder emits a `<plan>` with explicit acceptance criteria. Before every tool call, it emits a structured `<think>` block:

```
<plan>
goal:        what “done” means
steps:       1) ... 2) ... 3) ...
acceptance:  1) concrete done-check 2) concrete done-check
risks:       most likely failure modes
assumptions: what is being assumed
</plan>
```

```
<think>
goal:   what must succeed right now
step:   which plan step this belongs to
tool:   exec|read_file|write_file|patch_file|apply_diff|search_files|list_dir|glob|done
risk:   most likely failure mode
doubt:  one reason this approach could be wrong   ← the unusual field
next:   exact next action / command prefix
verify: how to confirm it worked
</think>
```

The runtime now validates `step` and `tool` against the current plan and actual tool call in both TUI and Web GUI. The `acceptance:` criteria also feed the verification requirement, so docs-only plans can stop at build/check/lint while behavior-changing plans are pushed to real tests. The `doubt:` field forces the model to surface one self-criticism before acting.

The scratchpad/governor protocol itself is now sourced from one shared contract file: `shared/governor_contract.json`. TUI reads it directly, and the Web GUI fetches the same contract from `/api/governor_contract` while also bootstrapping the same fallback contract from `/assets/governor_contract.js` before `app.js` runs. That shared contract now also drives block field mapping, field aliases, enum normalization, `done` schema requirements, shared `Done/Error Protocol` prompt sections, detailed `plan` / `think` / `reflect` / `impact` / `done` validator errors, gate/validator error messages, the verification heuristics themselves (intent terms, acceptance terms, path classes, and verification command signatures), the `goal_check` runner catalog used for test/build probes, the repo-goal probe requirements (`.git`, `HEAD`, `README.md`), the `goal_check` retry messages pushed back into the loop, the `goal_check` execution log/status lines shown in TUI/Web, and the `goal_check` execution policy itself (auto-run conditions, retry cap, probe order), which reduces prompt drift.

### State-Machine Loop (Planning → Executing → Verifying → Recovery)

Most "agent loops" are just a max-iteration timer. Spiral-Coder routes the Coder through a tiny state machine instead:

- `planning`  — restate the goal and pick the next concrete step
- `executing` — run tools (files/commands)
- `verifying` — run `goal_check` probes before declaring done
- `recovery`  — stuck detection triggers diagnostics + strategy shift

`git status` is diagnostics only. Completion verification must be a real test/build/check/lint command or your configured test command.

Verification is now acceptance-aware: docs-only work can stop after a real build/check/lint, but code/behavior changes are pushed to behavioral verification (`cargo test`, `pytest`, `npm test`, etc.) before `done`.

This makes long runs converge instead of drifting into README-polish loops.

Spiral-Coder also injects a compact `[Recent runs]` memory (commands + `write_file` / `patch_file` / `apply_diff`) so the Coder doesn't forget what it just did and repeat itself.

It also rebuilds a compact `[Working Memory]` from session messages: confirmed facts, completed steps, and known-good verification commands. That gives resumed runs positive memory, not just failure memory.

For code edits, the Coder now also carries three extra guardrails: an `[Evidence Gate]` that forces a short `<evidence>` block before `patch_file` / `apply_diff` on an existing file, a `[Task Contract]` derived from the root request so plans stay anchored to the actual task and verification floor, and an `[Assumption Ledger]` that tracks open / confirmed / refuted assumptions and blocks reusing refuted ones without new evidence. The TUI and Web GUI now enforce the same three layers.

The TUI and Web GUI Coder runtimes also inject an `[Instruction Resolver]` that makes the chain of command explicit: root/runtime safety > system/task contract > project rules > user request > execution scratchpad. `<plan>` / `<think>` / `<evidence>` / `<reflect>` / `<impact>` are treated as execution notes, never as authority. The authority order, prompt scaffolding, read-only conflict wording, and diagnostic exec classification now come from the shared governor contract in both runtimes.

After every successful mutation, the Coder is also forced to emit a short `<impact>` block before the next tool call, stating what changed and which acceptance criterion actually moved. The runtime now checks that `progress:` points to a real current plan step or acceptance criterion.
The TUI and Web GUI now both enforce the same `reflect` / `impact` runtime gates, so failed or stalled runs must self-correct explicitly before the next tool call.

The final `done` call is also acceptance-aware now: it must explicitly report which current plan acceptance criteria are already satisfied, which ones remain, and which successful verification command proved each completed criterion. Runtime validation checks that the coverage is complete and that every cited command is real.

### Goal Verification on Stop (No False "Done")

When the model returns `finish_reason=stop` without tool calls, Spiral-Coder can automatically run lightweight checks (repo init, tests, build) and push a `[goal_check]` message back into the loop if anything is missing or failing. That stop-path now uses the same shared policy and shared goal-check log format in both TUI and Web GUI.

The Web GUI also has a `/meta-diagnose` MVP now: run `/meta-diagnose`, `/meta-diagnose last-fail`, or `/meta-diagnose msg:<message-id>` in the Coder composer to send the last failure to the Observer as a JSON-only meta diagnosis. Failed Coder messages also expose a `Why did this fail?` button. Each run is saved under `.spiral-coder/meta-diagnose/` with the failure packet, observer prompt, raw response, parsed diagnosis, and parse status. The Observer pane also includes a lightweight `Meta` tab that lists saved artifacts, shows compact `primary_failure` counts, opens their details/raw JSON, and can re-run a diagnosis from the saved target or packet. The TUI also supports `/meta-diagnose`, `/meta-diagnose last-fail`, and `/meta-diagnose msg:coder-<index>` from either the Coder or Observer input, saving the same artifact set locally.

### @file References: Skip the Read Turn

Type `@path` anywhere in your message to inject that file's content as context before your prompt reaches the Coder:

```
@src/main.rs what does run_chat do?
@Cargo.toml @package.json show me the dependency versions side by side
fix the bug in @src/server.rs line 400
```

The TUI shows a notification for each injected file:
```
📎 injected: [src/main.rs] (276 lines, 8192 bytes)
```

The Web UI shows chips in the composer as you type:
```
📎 @src/main.rs   📎 @Cargo.toml
```

The Coder sees the file content immediately — no extra `read_file` round-trip needed. On a tight 12-iteration budget, skipping one read turn can be the difference between success and timeout.

### Phase Gating: Silence the Right Noise

Tell the Observer which phase you're in (`core` / `feature` / `polish`). Proposals that don't match are automatically dimmed. CSS tweaks don't interrupt you when your auth is broken.

### Health at a Glance

Every Observer response ends with a score:

```
--- health ---
score: 74  rationale: auth is solid, tests cover happy path only
```

❤ **74** → green (production-ready zone). The badge updates live as you build.

### Progress Checkpoints

At iterations 3, 6, and 9, the Coder pauses for a forced self-reflection **before the next tool call**:

```
<reflect>
last_outcome: success|failure|partial
goal_delta: closer|same|farther
wrong_assumption: <one short sentence>
strategy_change: keep|adjust|abandon
next_minimal_action: <one short sentence>
</reflect>
```

This is the difference between an agent that keeps circling and one that knows when it's lost.

### Cross-platform (Windows / macOS / Linux)

Spiral-Coder runs on Windows, macOS, and Linux.

It was originally built on Windows (so the annoying Windows edge cases are first-class), but the core runtime is OS-agnostic and the repo ships both PowerShell and bash entrypoints:

- Windows: `scripts/*.ps1` (`run-ui.ps1`, `run-tui.ps1`, …)
- macOS / Linux: `scripts/*.sh` (`run-ui.sh`, `run-tui.sh`, …)

Windows-specific hardening (still useful even if you mainly develop on macOS/Linux):
- WDAC-blocked binaries → Python Lite fallback server (pure stdlib)
- Automatic PowerShell syntax translation (bash → PS) for mixed transcripts
- Corporate proxy environments
- `sh.exe` Win32 error 5 on interactive git prompts

### Plugin Registry

Extend Spiral-Coder without forking it:

```js
registerObserverPlugin({ name: "my-plugin", onProposal, onHealth, onPhase })
registerPhase("security-review", { label: "Security Review", color: "#f97316" })
registerValidator(proposals => proposals.filter(p => p.score > 20))
```

Load your plugin via `<script>` before `app.js`. Done.

---

## The Observer Output Contract

The Observer doesn't free-write. It speaks a structured format that the UI parses into live cards:

```
--- phase ---
core

--- proposals ---
title: Input validation missing
toCoder: Validate length and character type before processing user input.
severity: critical
score: 88
phase: core
cost: low
impact: prevents crash on malformed input
quote: user_input = input()

--- coder_diagnostic ---
{
  "failure_mode": "missing_required_followup",
  "confidence": 90,
  "mutation_anchor": { "path": "src/tui/agent/task_harness.rs", "reason": "latest source file edited by the coder" },
  "required_followups": [{ "path": ".spiral-coder/runtime_eval.json", "required_literal": "eval --spec .spiral-coder/runtime_eval.json", "reason": "coder-loop changes need runtime eval proof before closeout" }],
  "verification_cmd": "cargo run --quiet -- eval --spec .spiral-coder/runtime_eval.json --max-cases 1",
  "next_coder_action": { "tool": "exec", "args": { "command": "cargo run --quiet -- eval --spec .spiral-coder/runtime_eval.json --max-cases 1" }, "reason": "produce the required replay/eval proof before closeout" }
}

--- benchmark_plan ---
{
  "case_id_hint": "runtime-eval-task-harness",
  "lane": "runtime_eval",
  "objective": "Add a runtime_eval regression for missing_required_followup so the Coder must mutate, verify, and close out with required artifacts.",
  "required_checks": ["cargo test -q tui::agent::tests::", "cargo run --quiet -- eval --spec .spiral-coder/runtime_eval.json --max-cases 1"],
  "success_criteria": ["runtime eval fails on the old behavior and passes after the harness fix"]
}

--- critical_path ---
Fix input validation before adding any new features.

--- health ---
score: 41  rationale: core logic works but injection surface is wide open
```

Every field is intentional. `quote` pins the exact offending line to the card. `cost` tells you how hard the fix is before you read the details. `phase` controls visibility. `coder_diagnostic` is generated from the full proposal set even when the UI displays only the top cards, so required docs/replay/eval follow-ups do not disappear behind truncation.

When a human sends an Observer proposal or diagnostic back to the Coder, the UI wraps it in an explicit `<observer_...>` handoff tag and asks the Coder to state accept / override / defer in the next plan. That keeps the external critic's voice distinct from ordinary user instructions.

`benchmark_plan` is the Observer's next-step testing instinct: it names the smallest regression lane and checks that should make the finding repeatable. When a human approves it from the TUI/GUI, the Coder receives it inside `<observer_benchmark_plan>` and the Task Harness switches to a dedicated `benchmark_plan` lane, first reading the matching `.spiral-coder/runtime_eval.json` or `.spiral-coder/tui_replay.json` spec before patching the smallest regression artifact. If the model emits a malformed spec patch after that read, the harness can synthesize the minimal JSON update from the approved `case_id_hint` and `src/...rs` evidence.

Runtime eval now has smoke cases for both approved `runtime_eval` and `tui_replay` benchmark plans. Those cases assert the copied tool-root spec actually contains the expected source path, so the dogfood loop proves artifact mutation instead of only checking the final assistant summary.

The harder `approved-benchmark-plan-docs-runtime-eval-smoke` case requires the same approved plan to update both a regression spec and a docs file. This keeps the dogfood loop honest for PR-ready change sets where code/spec/docs need to land together.

Benchmark-plan proof gates now require successful explicit verification commands via `verified_command_seen` and can assert patch-time auto-test evidence with `auto_test_passed`. After all approved benchmark-plan target files are satisfied, the Task Harness steers the Coder to the plan's `required_checks` command before allowing closeout; the approved benchmark-plan smoke lane now checks that proof for runtime-eval, TUI-replay, and compound docs+spec cases.

---

## Quickstart

### 0) Set your API key (TUI/CLI)

- OpenAI-compatible: `OPENAI_API_KEY` or `SPIRAL_CODER_API_KEY`
- Mistral: `MISTRAL_API_KEY` (or `SPIRAL_CODER_API_KEY`)
- Anthropic (Chat/Observer only): `ANTHROPIC_API_KEY`

```powershell
$env:OPENAI_API_KEY = "..."
# or: $env:MISTRAL_API_KEY = "..."
```

```bash
export OPENAI_API_KEY="..."
# or: export MISTRAL_API_KEY="..."
```

Web UI: paste keys in Settings (stored in your browser and sent only to your local server).

**Web UI (recommended)**
```powershell
# Windows (PowerShell)
.\scripts\run-ui.ps1
# → http://127.0.0.1:18080/
```

```bash
# macOS / Linux (bash)
bash ./scripts/run-ui.sh
# → http://127.0.0.1:18080/
```

In the Web UI: open Settings → choose Provider/Model/Base URL → paste API key → set `toolRoot` to your project path.

**TUI (terminal)**
```powershell
# Windows (PowerShell)
.\scripts\run-tui.ps1
```

```bash
# macOS / Linux (bash)
bash ./scripts/run-tui.sh
```

TUI defaults:
- UI language starts in English (`/lang ja|en|fr` changes it mid-session).
- The right pane opens on `Chat`; use `Ctrl+R` or `/tab observer|chat|tasks|promotions|merge` to switch tabs.
- The `Merge` tab reads the latest runtime eval `merge_gate.json`; `A` approves a passing case, `H` holds a case, `R` refreshes, and `Ctrl+Y` copies the rollback preview without running it.
- Run `/keys` to see which API key env var or CLI flag each pane expects.
- Typing `/` in the composer shows a lightweight slash-command picker.
- Typing exact `/provider` or `/model` opens an arrow-key picker (`Up/Down`, `Enter` to apply). `/provider` now exposes vendor presets such as `openai`, `gemini`, `anthropic-compat`, `mistral`, `anthropic`, and `hf`; `/model` follows the selected preset and still offers `other` for manual entry.
- If a pane is missing a required API key or model, send is blocked and the TUI shows a warning instead of running.

**Headless Coder (CLI)**
Install `spiral-coder` (optional):
- Windows (PowerShell): `.\scripts\install.ps1`
- macOS / Linux (bash): `bash ./scripts/install.sh`

Then run:
```bash
# (optional) generate .spiral-coder.md template (stack + test_cmd)
spiral-coder init -C .

# run the coding agent in your project
spiral-coder agent "fix the failing test" -C . --vibe

# persist and resume a session (default: .tmp/spiral_coder_session.json)
spiral-coder agent "fix the failing test" -C . --vibe --session
# resume later (omit prompt -> auto "continue")
spiral-coder agent -C . --vibe --session

# write machine-readable artifacts (trace + snapshot + execution graph)
spiral-coder agent "fix the failing test" -C . --vibe --trace-out .tmp/spiral_coder_trace.jsonl --json-out .tmp/spiral_coder_final.json --graph-out .tmp/spiral_coder_graph.json

# run the runtime eval harness against fixture cases
spiral-coder eval -C . --spec .spiral-coder/runtime_eval.json
spiral-coder eval -C . --spec .spiral-coder/runtime_eval.json --filter repo-map --continue-on-error

# inspect merge readiness from the latest runtime eval merge gate
spiral-coder merge-gate -C .
spiral-coder merge-gate -C . --json
spiral-coder merge-gate -C . --ci
# review the same gate in TUI/Web: TUI `/tab merge`, or Web GUI Settings -> Merge gate

# inspect repo/runtime inventory surfaces
spiral-coder inventory health -C .
spiral-coder inventory health -C . --json
spiral-coder inventory health -C . --ci --fail-on yellow
spiral-coder inventory manifest -C .
spiral-coder inventory parity -C .
spiral-coder inventory replay-status -C . --json

# auto-fix loop (Coder → Observer diff review → Coder)
spiral-coder agent "fix the failing test" -C . --vibe --autofix
spiral-coder agent "fix the failing test" -C . --vibe --autofix 3

# auto-approve tool actions (no prompts)
spiral-coder agent "fix the failing test" -C . --vibe -y

# review your current git diff with Observer
spiral-coder review -C .

# review changes since a checkpoint (hash printed by `spiral-coder agent`)
spiral-coder review -C . --base <checkpoint_hash>
```

**Python Lite (WDAC / no Rust binary)**
```powershell
# Windows
python .\scripts\serve_lite.py
# → http://127.0.0.1:18080/
```

```bash
# macOS / Linux
python3 ./scripts/serve_lite.py
# → http://127.0.0.1:18080/
```

---

## Key Concepts

### tool_root

Every agent action runs inside a working directory.

Defaults:
- **Web UI**: `.tmp/<thread-id>` (isolated per thread)
- **TUI**: `.tmp/tui_<epoch>` (isolated per session)
- **CLI**: current directory

To work on your actual project, set `tool_root` to your project path:
- **TUI**: `-C .` / `--tool-root .` flag, or `/root <path>` slash command at runtime
- **Web UI**: Settings → toolRoot field
- **CLI**: `spiral-coder agent "<prompt>" -C .`

When `tool_root` is set, Spiral-Coder scans it on first use to build the project context block (stack, git, tree). Subsequent sends in the same session skip the scan.

Path traversal is blocked: paths with `..` components are rejected at every tool boundary.

### Language

- **UI language**: TUI `/lang ja|en|fr` (also affects prompts).
- **Observer language (Web UI)**: `auto` (default) follows your last user message language even if the UI is English; `ui` follows UI; or force `ja`/`en`/`fr`.

### Sessions (CLI)

`spiral-coder agent` can save and resume the full conversation (including tool calls) with `--session[=<path>]`.

- Default path: `.tmp/spiral_coder_session.json`
- If `-C/--root` is set, relative `--session` paths are resolved under `tool_root`
- Autosaves during the run (after tool calls)
- Resume without a prompt: run `spiral-coder agent -C . --session` again
- Start fresh: add `--new-session` (overwrites the file)

Related artifacts:
- `--trace-out <path>` / `--trace_out`: JSONL trace (tool calls, checkpoints, errors, done)
- `--json-out <path>` / `--json_out`: final session snapshot JSON (messages + tool calls + tool results)
- `--graph-out <path>` / `--graph_out`: execution graph JSON (nodes + edges) derived from the final messages
- If `-C/--root` is set, relative output paths are resolved under `tool_root`

Session JSON may contain code and tool outputs — treat it as sensitive.

### Runtime Eval Harness

`spiral-coder eval` runs a fixture-driven suite against the headless Coder and writes per-case artifacts plus a final JSON report.

Example:

```bash
spiral-coder eval -C . --spec .spiral-coder/runtime_eval.json
```

What it writes:
- `<out_dir>/<case>/trace.jsonl`: JSONL trace from the run
- `<out_dir>/<case>/session.json`: resumable session snapshot
- `<out_dir>/<case>/final.json`: final snapshot copy for scoring
- `<out_dir>/<case>/graph.json`: execution graph
- `<out_dir>/report.json`: aggregated pass/fail report

Useful flags:
- `--filter <text>`: run only matching case ids/tags
- `--max-cases <n>`: cap selected cases
- `--continue-on-error`: keep running after a failed case
- `--out-dir <path>` / `--report-out <path>`: override artifact paths

Current v1 guidance:
- fixtures should be read-only or run against a disposable workspace
- eval runs non-interactively so cases must not rely on manual approvals
- scoring is artifact-based today: completion, errors, tool usage, assistant output, graph size
- the report also surfaces light telemetry from traces: iterations, repo-map fallback hits, governor/recovery pressure, and realize summary counters

### Inventory / Parity CLI

`spiral-coder inventory` prints lightweight repo/runtime inventory views for ops and parity work.

Examples:

```bash
spiral-coder inventory health -C .
spiral-coder inventory health -C . --json
spiral-coder inventory health -C . --ci --fail-on yellow
spiral-coder inventory manifest -C .
spiral-coder inventory commands -C .
spiral-coder inventory tools -C .
spiral-coder inventory state -C .
spiral-coder inventory replay-status -C . --json
spiral-coder inventory parity -C .
```

Current views:
- `health`: top-level green/yellow/red ops snapshot with next actions
- `manifest`: package/provider/mode/persona/spec/docs/state snapshot
- `commands`: top-level CLI commands plus TUI slash commands
- `tools`: built-in Coder tool definitions and required args
- `state`: typed state ownership / persistence / health snapshot
- `replay-status`: runtime-eval / TUI-replay spec counts plus latest reports under `.tmp/`
- `parity`: high-level implemented/partial/missing map for quality and runtime surfaces

`health` is the intended entry point for automation:
- `--json`: stable machine-readable output for scripts/CI
- `--ci --fail-on red|yellow`: exit non-zero when health crosses the chosen threshold

Today `health` rolls up:
- latest runtime-eval proof
- latest TUI-replay proof
- typed state readiness
- split-module progress
- surface parity status
- proof freshness (`fresh/recent/stale/old/missing`)

### Approvals

- **Web UI**: edits/commands queue as pending items in **Settings**. A header badge jumps straight to the pending approvals list, where you can approve/reject from the browser.
- **CLI (`spiral-coder agent`)**: prompts before running `exec` and applying file edits (`write_file` / `patch_file` / `apply_diff`). Use `-y/--yes` or `--no-approvals` to skip prompts.
- **TUI**: currently auto-approves tool actions.

### Providers

Spiral-Coder supports these providers today:

| Provider | `--provider` | Default `base_url` | Key env var(s) | Tool-calling Coder |
|---|---|---|---|---|
| OpenAI-compatible | `openai-compatible` | `https://api.openai.com/v1` | `SPIRAL_CODER_API_KEY` or `OPENAI_API_KEY` | ✅ |
| Mistral | `mistral` | `https://api.mistral.ai/v1` | `MISTRAL_API_KEY` (or `SPIRAL_CODER_API_KEY`) | ✅ |
| Anthropic | `anthropic` | `https://api.anthropic.com/v1` | `ANTHROPIC_API_KEY` | ❌ (Chat/Observer only) |
| HF local (subprocess) | `hf` | `http://localhost` | *(none)* | ❌ (Chat/Observer only) |

Notes:
- The Coder agent loop (`spiral-coder agent`, TUI Coder, Web agentic mode) requires an OpenAI-compatible **Chat Completions** API with tool calling (`tools` / `tool_calls`) → use `openai-compatible` or `mistral`.
- `openai-compatible` means the OpenAI Chat Completions API (`/v1/chat/completions`) with Bearer auth. In the TUI, the provider picker offers concrete hosted presets (`openai`, `gemini`, `anthropic-compat`) and sets the matching `base_url` + default model for you.
- You can list built-ins with `spiral-coder list providers` / `spiral-coder list modes` / `spiral-coder list personas`.

Set a different model per role: fast model for Coder iteration, powerful model for Observer analysis. In the TUI you can also split providers per pane (Coder still must be `openai-compatible`/`mistral`): `spiral-coder tui --observer-provider anthropic --observer-model claude-3-5-sonnet-latest`. Common gotchas: `401` (bad key), `429` (rate limit), `max_tokens` vs `max_completion_tokens` mismatch.

### Chat Personas

Five chips above the Chat composer — switch anytime, independent of Coder/Observer:

| Chip | Style |
|---|---|
| 😊 Cheerful | Upbeat and encouraging |
| 🤔 Thoughtful | Checks assumptions, answers carefully |
| 🧙 Sensei | Guides with questions, not answers |
| 😏 Cynical | Points straight to the uncomfortable truth |
| 🦆 Duck | Never answers — just asks "Why?" to unblock your thinking |

### Chat = Companion (Not an Agent)

Chat never executes tools. It's there to keep you thinking while the runtime (Coder/Observer) is busy.

In the Web UI, Chat has two optional helpers:
- **Attach runtime snapshot**: injects a small read-only runtime summary (cwd, last error snippet, pending approvals, open tasks) so you can ask "what's happening?" without leaving the chat tab.
- **Auto tasks**: a behind-the-scenes TaskRouter turns chat into concrete tasks for Coder/Observer (visible in **Tasks**). You still choose what to send.

### /slash Commands (TUI)

| Command | Effect |
|---|---|
| `/model <name>` | Switch the current pane's model mid-session |
| `/provider <name>` | Switch the current pane's provider mid-session (or show current; exact `/provider` opens picker) |
| `/base_url <url>` | Switch the current pane's base_url mid-session (or show; use `default` to reset) |
| `/mode <name>` | Switch the current pane's mode |
| `/persona <key>` | Switch the current pane's persona |
| `/temp <0.0–2.0>` | Adjust the current pane's temperature |
| `/realize <off\|low\|mid\|high>` | Set the Coder's realize-on-demand strength and persist it under `.spiral-coder/tui_prefs.json` (`mid` default in TUI) |
| `/root <path>` | Change tool_root for subsequent sends |
| `/lang ja\|en\|fr` | Switch UI + prompt language |
| `/tab <observer\|chat\|tasks\|next>` | Switch the right-side pane explicitly |
| `/keys` | Show per-pane API key status and setup help |
| `/autofix` | Toggle Observer → Coder auto-fix forwarding |
| `/find <query>` | Filter messages in the current pane |
| `/meta-diagnose [last-fail\|msg:coder-<index>]` | Send a selected Coder failure to Observer for JSON-only diagnosis |
| `/help` | Show all commands |

Most TUI knobs are stored per project in `.spiral-coder/tui_prefs.json`, including pane `provider/base_url/mode/model/persona/temp`, Coder `/realize`, `/lang`, `/autofix`, `Ctrl+A` auto-observe, and the last right-side tab.

### Engineering Docs

- [Runtime architecture](./docs/runtime-architecture.md)
- [State schema](./docs/state-schema.md)
- [TUI agent split plan](./docs/tui-agent-split-plan.md)
- [Repo-specific contributor rules](./AGENTS.md)

---

## Security

`127.0.0.1` only by default. Shell execution is real — keep approvals enabled.

File tool paths are validated against `tool_root` at every call: absolute paths outside `tool_root` and any `..` component are rejected with an error (never silently).

If you expose to a network, add authentication and harden tool execution.

---

## Troubleshooting

**"Failed to connect to github.com via 127.0.0.1"** — dead proxy in env vars:
```powershell
Remove-Item Env:HTTP_PROXY,Env:HTTPS_PROXY,Env:ALL_PROXY,Env:GIT_HTTP_PROXY,Env:GIT_HTTPS_PROXY -ErrorAction SilentlyContinue
```

**Push without interactive prompts** (WDAC / sh.exe Win32 error 5):
```powershell
$env:GITHUB_TOKEN = "ghp_..."
.\scripts\push.ps1
```

**Push via SSH over 443** (corporate network):
```powershell
.\scripts\push_ssh.ps1
```

**"access denied" on spiral-coder.exe** — binary still running:
```powershell
.\scripts\kill-spiral-coder.ps1
```

---

## License

Apache License 2.0
