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

Final runtime `6df2e78` passes `bash scripts/check-local.sh`: **751 Rust tests**
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
