# Current evidence, JSON repair guidance and authored closeout

Two complete adaptive panels retain **18 episodes / 116 wire requests**. Each
scores **6/9 strict completions**. This is development evidence, not a controlled
comparison or a reliability claim. Cumulative coupled evaluation now contains
144 episodes and 987 wire requests across 16 rounds; no failed episode is dropped.

| Round | Runtime commit | Mistral Small | GPT-4.1 mini | Gemini Flash-Lite | Strict total | Wire requests |
| --- | --- | --- | --- | --- | --- | --- |
| 15 | `8c0ce46` | 2/3 | 3/3 | 1/3 | 6/9 | 55 |
| 16 | `6df2e78` | 1/3 | 3/3 | 2/3 | 6/9 | 61 |

Both roles use the same model in each episode: `mistral-small-2603`,
`gpt-4.1-mini`, or `gemini-3.5-flash-lite`. The frozen suite, all 13 fixture
hashes, assertions, native `patch_file` requirement, temperature 0, max tokens
2048, request timeout 90 seconds, episode timeout 600 seconds, and shared caps
10/8/12 remain unchanged. The suite SHA-256 is
`3c3c24ddec901a769158b6cab126797bdb2427243387b0b9b315e5e3182e4752`.
Each panel declares its candidate and binary hash before execution; source does
not change during a panel. Round 16 is an adaptive follow-up, not an unchanged
replicate of round 15. There is no Observer-disabled control.

## Repairs and observed limits

- JSON reads now supply bounded exact append recipes from the current file.
  Rejected preserved edits show the candidate syntax location and original
  insertion recipe. Tests exercise rejection without byte changes, then a valid
  patch preserving nested and unknown fields. Mistral/GPT pass all four replay
  episodes in these panels; Gemini never creates the requested case. All six
  preserve the old JSON. These observations do not isolate the recipe's effect.
- Observer prompts omit superseded tool outputs and duplicate UI digests,
  while raw packets retain them for audit. TUI/Web tests verify newer receipt
  content wins over older native results. Round 15 Mistral advice recognizes
  reporting as the remaining step, but its suggested acceptance rows are still
  malformed. Temporal provenance does not guarantee sound model reasoning.
- Native `done` no longer guesses replacement acceptance mappings. Stable
  references, current exact commands and preserved bulleted-list boundaries
  support an authored retry. Actual HTTP tests cover recovery and bounded
  repeated refusal without repeating verification. R16 Mistral corrects the
  mapping on its final request, then fails the separate case-sensitive report
  requirement (`Fresh exec proof` versus `fresh exec proof`). The gate remains
  strict and the episode remains incomplete.
- R15 Gemini exposed an additional runtime defect: after a failed baseline
  check and a fresh source read, handoff restored `Diagnose` from a retained
  failed-exec counter and blocked the correct patch. The real Cargo regression
  fails before the fix and passes afterward. HTTP continuation executes the
  diagnosed repair; a newer unrelated failure still retains diagnosis. R16
  Gemini completes this source task, a single observation rather than proof of
  reliable recovery.
- R16 Mistral source has a correct artifact and passing independent tests but
  omits the fixed native `patch_file` requirement. It is a strict failure.
  Gemini replay spends its cap on missing protocol fields and misplaced
  inspection/verification rather than editing. These remain dogfooding targets.

Runtime completion is **13/18**, while strict completion is **12/18**. Artifact
correctness, runtime guards, protocol compliance and task completion are kept
separate. No model ranking, general nonregression or Observer-causality claim is
made from these small adaptive panels.

## Verification and receipts

Panel 16 runtime `6df2e78` passes `bash scripts/check-local.sh`: **751 Rust tests**
(1 ignored), 1 integration, TUI replay 3/3, Python 16, Web 67, repo-map 14/14,
syntax checks, 11 Lite assets and the local server smoke. Frontend rendering was
not changed; prior real-browser QA remains in the
[previous record](../2026-10-14-baseline-and-review/README.md).

- [Sanitized per-episode receipts](receipts.json) include exact checks, caps,
  role/request counts, measured usage and failure outcomes.
- [Verification manifest](verification.json) binds the candidate and local checks.
- [Failure ledger](failure-modes.json) distinguishes regression-covered runtime
  defects from open model behavior.
- [Raw manifest](raw-manifest.json) hashes 278 retained files, including all
  failed episodes, frozen binaries, predeclarations and before/after regression
  logs. `LOCAL_RAW` denotes the operator's local Spiral-Coder cache; raw sessions
  and opaque provider continuation metadata are not published in this PR.

Dogfooding should begin with small reviewable tasks under the existing shared
caps, checking the actual diff, executed verification and authored final report.
The passing deterministic suite is a correctness boundary; weak-model task
completion remains a measured limitation.

## Post-panel fork-review corrections

A read-only reviewer without the parent conversation reviewed the full PR at
`9b84c6f` while project-local context files were temporarily hidden. It found two
P2 defects: ordinary Web Coder requests lost their preceding conversation, and
Anthropic non-streaming Observer requests bypassed the shared budget. Context
files were restored before reproducing and repairing either defect.

`4185d4a` separates plain conversation from native evidence and meters Anthropic
before network I/O. Regression tests fail before the fixes. The final local
suite passes **755 Rust tests** (1 ignored), 1 integration, TUI replay 3/3,
Python 16, Web 72, repo-map 14/14, syntax and Lite smoke. Its log SHA-256 is
`025006da29c3faf57d25ab2e659839446ec689c62cdc6a0b59cfad85e93a912d`.
Real Lite/Chrome checks reproduce the lost-history behavior, then confirm both
ordinary and streaming follow-ups preserve context after correction. Native
new-task isolation and correlated resume are separately regression-tested.
Anthropic HTTP fixtures verify pre-send exhaustion, persisted usage/duration,
HTTP errors and invalid JSON; no live Anthropic call is claimed.

A second fresh reviewer inspected the six-file correction and adjacent paths,
ran 19 Web and 8 Anthropic tests, and reported no actionable findings. This
follow-up review covers the correction, not another full PR review. Context
files were again restored without changes.

These review fixes affect Web history and Anthropic non-streaming calls,
outside the three selected TUI live-provider paths. R15/R16 candidate hashes
and receipts above remain unchanged; no final-revision live panel is implied.
GitHub Actions [run 38730470234](https://github.com/RyoSpiralArchitect/Spiral-Coder/actions/runs/38730470234)
could not start either platform job because the account was locked due to a
billing issue. Local success is not represented as a successful final CI run.
