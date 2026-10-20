# Task proof, small JSON edits, and Gemini Web parity

## Scope and evidence boundary

Coder and Observer are one work unit: each episode uses the same selected model
for both roles and shares its original request cap. No model was upgraded and no
historical fixture, assertion, or cap was relaxed.

The implementation adds an opt-in `<task_requirements>` contract from the human
root request. Each requirement names source/test/documentation artifacts and
exact native checks, optionally with a failing baseline before the first edit.
Both roles consume the same projection. Completion cannot be inferred from a
model-edited plan, an old passing test, or a missing output file. A first edit is
blocked when required baseline proof is missing. This is a receipt contract;
it does not automatically infer requirements from arbitrary prose or prove that
a generated test checks the intended behavior.

`patch_file` now supports small JSON-pointer append/compare-and-replace operations.
Runtime and approval preview parse the complete file and preserve other values,
including unknown/nested fields. Invalid, stale, mixed-mode and duplicate-ID
requests fail without writing. Existing replay preservation checks remain active.

The Web Coder and Observer catalogs use Gemini's flat evidence rows consistently;
repair examples and parsing accept the same format. Native current commands and
task obligations feed the GUI. Successful stderr output is retained in native
history (Python unittest prints its result there).

## Frozen live runs

Two adaptive rounds, three models, two panels: **12 episodes, 177 total wire
requests, 0 fully qualified tasks, 0 runtime completions**. There were no false
runtime completions in this sample. These are small regression observations, not
a success-rate estimate or evidence of general coding improvement.

| Model (both roles) | Real repository r1 / r2 | JSON replay r1 / r2 | Shared requests r1 / r2 (repository; JSON) |
| --- | --- | --- | --- |
| mistral-small-2603 | CLI oracle fails / fails | replay oracle passes / fails | 18 / 18; 12 / 12 |
| gpt-4.1-mini | CLI oracle fails / fails | replay oracle fails / fails | 18 / 18; 9 / 12 |
| gemini-3.5-flash-lite | CLI oracle fails / fails | replay oracle fails / passes | 18 / 18; 12 / 12 |

A replay-oracle pass means old and new cases pass independently; both such
episodes still failed full qualification because the native task did not finish.
Every JSON episode preserved all old cases and assertions. Several native
structured JSON appends succeeded; malformed values, mixed edit modes and
incorrect expected-value fields still caused model retries.

Candidates:

- r1: `1023e88668c263b7ba102f15da841895d1c1eb57`.
- r2: `74073063c984b7d95596f7bbfc70566841702324` adds the pre-edit baseline gate,
  explicit baseline guidance, stricter Observer edit-shape validation, and Web
  successful-stderr retention.
- The later truthful-partial-handoff correction was verified deterministically:
  a report with remaining acceptance criteria may stop but remains incomplete.
  The live counts above belong to the pinned r1/r2 candidates, not that follow-up.

The repository panel reuses the previous `--case` task and unchanged independent
CLI oracle, with three task files restored from `a9b03f9` before the manually
implemented feature. The current harness is retained. The prompt appends explicit
requirements and the external oracle command. All three untouched workspaces
failed the oracle; the known implementation passed it before each round. The
oracle's SHA-256 stayed
`f3b108e044b333993210d05ce5964912820e3f190068c1bbc924e102b3f1e79c`.
Because the prompt/contract differs from the old panel, this is not a controlled
comparison with the earlier 0/9 result.

The JSON panel uses the unchanged
`coupled-approved-benchmark-plan-tui-replay-smoke` case and pilot runner. Its
fixture is smaller than the display limit; the 28 KB edit path is covered by
separate deterministic and native HTTP checks, not claimed as live-model proof.
Each repository episode is capped at 18 requests, each JSON episode at 12;
temperature 0, output cap 2048, request timeout 90 seconds, wall cap 600 seconds.
Observer and repair calls consume the same cap. No task result was retried in
place or overwritten.

## What the failures establish

1. An edit-time gate is needed in addition to a completion-time gate. In r1,
   Mistral and GPT edited before recording the required failing baseline. In r2,
   attempted early edits were refused and all three models recorded the failing
   oracle before their successful file edits.
2. Green existing unit tests still do not establish the requested CLI behavior.
   The independent oracle caught incomplete selection/receipt wiring after native
   edits and passing old tests. No repository episode updated all three required
   artifacts; documentation remained missing.
3. Long-file access and protocol transitions still spend much of the shared
   budget. Repeated `read_file` calls expose the same truncated prefix; searching
   for a later section does not provide a bounded range read. Plan/think/diagnosis
   repairs and Observer continuation can leave too little budget for final proof
   and reporting. Structured JSON editing removes whole-document generation from
   one path, but does not solve this broader control-flow problem.

Next work should keep the pair as the unit: add bounded file ranges and precise
search context, distinguish protocol repair from workspace failure recovery, and
exercise a small complete task with protected verification/report budget. Keep
first-attempt failures and the independent oracle unchanged when measuring it.

## Offline and rendered verification

`scripts/check-local.sh` passed on the final implementation: 788 Rust tests
(1 ignored), 1 integration test, TUI replay 3/3, Python 20, Web 79, repo-map 14/14,
JavaScript/shell syntax checks and Lite asset/exec smoke.

Chrome was driven through bundled Playwright against the native Rust server
because the Browser plugin was unavailable. Only provider SSE was mocked. The
flow submitted a task, ran a real Python unittest, rejected missing acceptance
evidence, accepted a corrected flat row, and persisted completion/native tool IDs
and three local request receipts across reload. Desktop 1440x1000 and mobile
390x844 had no horizontal overflow, browser errors, HTTP failures or framework
overlay. This verifies rendering and protocol wiring, not a live Gemini judgment.
`browser.json` records the checks. Screenshots remain in the local raw directory.

The native HTTP JSON check used a file with over 28 KB of existing content,
verified unchanged nested values after an append, and rejected duplicate IDs and
stale expectations without changing the file. See `json-http.json`.

## Receipts and reproduction

- `receipts.json`: bounded per-episode checks, action kinds and request telemetry.
- `*-predeclared.json`: source, binary, archive, prompt, overlay and oracle hashes.
- `task.txt` and `oracle.py`: exact repository-panel inputs. The task includes the
  local frozen oracle path; use a new manifest when relocating it.
- `raw-manifest.json`: SHA-256 and sizes for local logs, native histories, traces,
  patches, binaries and browser evidence. Raw provider continuation metadata stays
  local in `~/.cache/spiral-coder/dogfood-20261021`.

The local `prepare.py`, `launch.py`, `launch_json.py`, `collect.py` and
`publish_receipts.py` reconstruct and record the runs. Credentials were read as
literal environment assignments without sourcing `.zshrc`; values were never
printed or published. Reproduction requires fresh output directories and a
separately authorized provider budget.
