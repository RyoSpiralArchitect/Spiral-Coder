# Integrated task continuation — 2026-10-14

Historical rounds 1–4 are retained below. See [the follow-up record for rounds 5–7](../2026-10-14-task-obligations/README.md) for the current candidate and qualification.

Coder and Observer now continue one native task with correlated execution
evidence and one durable request cap. The five planned changes are implemented:
native continuation, assist receipts, shared accounting, full JSON preservation,
and the browser continuation/approval path. **Live qualification remains
incomplete.** Four adaptive rounds produced 2/9, 2/9, 5/9 and 3/9 strict passes.
The latest result does not establish improved reliability.

## What changed

- TUI continuation retains the original human request, native call/result IDs,
  provider continuation metadata, current proof and pending obligations. Runtime
  feedback does not create a new human task. Request generations discard old
  tokens after cancellation or root changes.
- Observer packets include actual actions and results, pending exact checks,
  artifact preservation failures and previous assist outcomes. An accepted
  suggestion, an executed action and task completion are separate receipts.
- A durable task budget charges each HTTP attempt, including compatibility
  retries and one Observer repair. Reload does not refund attempts. Missing
  provider usage remains null; repeated input estimates are counted per request.
- An explicit replay-preservation obligation captures the complete old JSON.
  Direct file edits are checked before writing, shell edits again at completion,
  and the independent evaluator uses a baseline outside the writable fixture.
  Typed replay validation no longer discards unknown fields when appending.
- The Rust browser endpoint shares the TUI action validator, repair prompt,
  native exchange repair and final-report validation. Continue with advice is
  explicit. Native calls with `finish_reason=stop` are dispatched. Command and
  file approvals return their actual results to the original native call.

See [the state and contract guide](../../coupled-work-loop.md) for ownership,
persistence and the deliberately bounded preservation trigger.

## Deterministic and browser checks

Final runtime commit `3f3a5a58fd4e703d50194b76bfd0d54c9d6b7ac7` passes `bash scripts/check-local.sh`: **712 Rust tests (1 ignored),
1 integration, TUI replay 3/3, Python 15, Web 61, repo-map 14/14**, syntax checks,
11 local Lite assets and the Lite end-to-end smoke test. Cancellation regressions also reject late budget writes after task replacement
or same-task reload and stop a nested provider request with its parent. The final binary and
check-log hashes are in [verification.json](verification.json).

The actual HTTP/TUI regression retains an earlier native read, obtains Observer
advice, continues the original task and produces its required final report.
A second version starts with successful verification at a chunk cap and only
two requests remaining; Observer and final reporting use those two requests.
Other regressions cover delayed old events, replaced tasks, retry exhaustion,
approval result correlation, stale edit previews, full JSON preservation and
the independent oracle's timeout receipts.

Computer use exercised the real Rust server against a deterministic local HTTP
provider. These are product-flow checks, separate from live model qualification:

1. Coder native reads arrive with `stop`; a deliberate stall leads to a valid
   Observer card. Continue with advice queues a command approval. Approval runs
   the actual Node test, resumes Coder, and ends with `Executed: true`,
   `Completed: true`, no pending checks and the required final phrase. Reload
   preserves the completed task and disables consumed advice. The server ledger
   contains 8/8 requests: 7 Coder and 1 Observer.
2. `patch_file` waits for approval while `note.txt` remains unchanged. Approval
   applies the edit, a second approval runs `node --test note-edit.test.mjs`,
   and Coder completes with the required report. The server ledger contains
   4/8 requests. Tests separately cover `write_file` and `apply_diff` approval.

The GUI exercises exposed and repaired command approval correlation, an explicit
test request being mistaken for read-only inspection, missing `node --test`
verification classification, file approval correlation, patch/diff approval
bypass, and desktop sidebar scrolling that moved the working panes off screen.
An edit preview now fails if the file changed before approval. The desktop
viewport was 1280×900; at 390×844 the document had no horizontal overflow.
No console warnings/errors were observed. Temporary viewport overrides were
reset. The raw manifest hashes both screenshots and the local provider fixtures.

## Fixed live conditions and scoring corrections

Both roles use the same configured model: `mistral-small-2603`, `gpt-4.1-mini`, or
`gemini-3.5-flash-lite`. Each round runs all three cases for all three models:
damaged-exchange resume, exact proof/final reporting, and replay addition with
preservation. Shared wire caps remain **10 / 8 / 12**, temperature 0, output cap
2048, request timeout 90 seconds and per-episode wall limit 600 seconds.
There are **36 retained episodes and 262 wire requests**, with no episode
timeout. There is no Observer-disabled comparison in this record.

The new spec's hash is
`3c3c24ddec901a769158b6cab126797bdb2427243387b0b9b315e5e3182e4752`
in every round. The historical runtime spec, fixtures, `patch_file` requirements
and case-sensitive final-answer requirements are unchanged. The independent
replay fixture adds a nonempty secondary `based_on` and an extension field
before each run so lossy rewrites are observable.

Strict qualification requires all copied fixed case checks, runtime completion,
current exact proof, the shared cap, successful process termination and the
independent artifact checks. Runtime completion alone is insufficient.

- Round 1 initially reported 5/9 using only completion/artifact checks. That
  was incomplete scoring. Retained native sessions/traces were re-adjudicated
  with the same fixed evaluator: **2/9**. Original summaries remain intact and
  their old booleans are explicitly labeled in the published receipts.
- Rounds 1–2 omitted `verification_config` from the new driver's trace. All
  resume fixtures passed the external Rust tests, but the configured automatic
  test command could not satisfy the fixed trace check. These are retained
  instrumentation failures, not retroactively granted passes. Round 3 records
  that configuration before running the loop.
- Round 1's replay oracle could run only the old case without establishing the
  requested new case. Round 2 adds an explicit new-case-exists check. The round-1
  receipt separately records whether its retained artifact contains that ID.

| Model | Round 1, strict re-adjudication | Round 2 | Round 3 | Round 4 |
|---|---:|---:|---:|---:|
| Mistral Small | 1/3 | 0/3 | 0/3 | 1/3 |
| GPT-4.1 mini | 1/3 | 1/3 | 3/3 | 2/3 |
| Gemini Flash Lite | 0/3 | 1/3 | 2/3 | 0/3 |
| Total | **2/9** | **2/9** | **5/9** | **3/9** |

| Candidate | Binary SHA-256 | Main change before this round |
|---|---|---|
| 1 | `2af6c1e8f152ea71087be10c3624a765067a5e869ba5c87128c5f2230ca8756b` | Integrated loop and shared receipts |
| 2 | `50b5c1dbf26ae2ac2cd81d4638738e5895cd5b5639823f5962cdd6e6aa7d22e7` | Preservation enforcement and complete fixed-check scoring |
| 3 | `32a15035cdca5969d46273843160a657d8c696ab38182836fde2260711deccb0` | Trace configuration, replay format guidance and explicit-run intent |
| 4 | `e2eb420aa2b92419cf0a5e5045ca97d1db2d886fb96e475358074fad483b01ef` | Bound final-report clauses to their sentence; browser approval fixes |

Round 3 completed 7/9 at runtime but only 5/9 strictly: Mistral omitted the
required `patch_file` in two otherwise completed tasks, and failed the exact
`fresh exec proof` phrase in another. Gemini's replay reached valid artifacts
but not exact native checks and final reporting within 12 requests. All three
replay artifacts preserved the old JSON and passed old+new replay in that round.

Round 4's replay artifacts still preserved the complete old JSON (3/3), but only
Gemini created the requested new case, and no model passed all replay cases.
Mistral and GPT struggled with patch anchors/recovery; Observer sometimes
redirected them toward a source path mentioned as benchmark subject rather
than the actual replay artifact. Gemini's resume exhausted its cap before the
edit; its replay lacked correct checks and proof. Mistral's exact-proof case
had current verification but no accepted final report.

Gemini's round-4 exact-proof case exposed a runtime defect: its initial chunk
ended with a successful tool, and the failure selector ignored the incomplete
task despite four remaining requests. **The final patch recognizes the runtime
chunk-cap marker as `incomplete_task` and covers its real Observer→Coder flow
deterministically. This last fix and the subsequent cancellation/budget persistence repair have not received another live round.** No
round-4 result is rewritten as passing because of that repair.

## Remaining work and evidence boundaries

- Improve Observer's choice between the artifact to change, source paths
  mentioned as data, missing verification and final reporting. A valid contract
  can still recommend an unhelpful action.
- Reduce repeated unsuccessful edits and protocol/recovery turns within the
  existing shared caps; preserve the original assertions and proof obligations.
- Keep the required tool-use failures visible independently of valid artifacts.
  A `write_file` success does not satisfy the frozen `patch_file` criterion.
- Validate the final chunk-cap fix live, then run a predeclared repeated panel.
  Any Observer ablation must use the same total budget and include repair costs.

These adaptive, small-fixture samples do not establish model rankings,
Observer's causal contribution, general nonregression or provider reliability.
All caps were respected; boundedness is not task efficacy. Passing deterministic
tests is separate evidence from the unresolved live failures.

[receipts.json](receipts.json) contains whitelisted per-request usage, estimates,
durations, per-case fixed checks, independent oracle outcomes and assist receipts.
It omits model thought metadata and credentials. [raw-manifest.json](raw-manifest.json)
hashes the retained sessions, traces, checks and artifacts under local
`coupled-20261014-r1` through `r4` directories; raw files are retained locally,
not embedded in the repository. `LOCAL_RAW` resolves to `~/.cache/spiral-coder` on the evaluation host. [failure-modes.json](failure-modes.json) tracks
repairs and open failure classes. Historical failed receipts are not replaced.
