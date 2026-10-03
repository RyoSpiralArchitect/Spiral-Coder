# Spiral-Coder restart: bounded runtime evidence

This report preserves historical baseline failures and every completed restart attempt. It describes frozen executed artifacts, not automatically the latest source commit. Attempts 01–06 are recorded below. The first corrected target-isolation run was attempt 04; the latest completed follow-up is attempt 06 (Mistral 4/6, OpenAI 4/6). Neither cleared the six-case gate. These are bounded, harness-assisted integration observations, not model rankings.

## Evidence qualification: historical Cargo target contamination

The baseline and attempts 01–03 reused one Cargo target directory across copied
live fixtures. A later controlled audit found that identical, untouched failing
Rust source exited 101 with a fresh target (0 tests passed, 1 failed), but exited
0 with the shared live target (1 passed, 0 failed). The source SHA-256 was
`0bb3788c33d63d3bc3d608753b6e022f20e0306c7b503e65618daef829c354c6`.

The PASS/FAIL labels below preserve the original reported outcomes. Cargo-based
success in those live receipts is **unreliable evidence of source correctness**;
it must not be presented as a verified repair. This qualification does not erase
the independent baseline HTTP 400 protocol failure. The main repository's local
suite used a separate target for one unchanged checkout and is not affected by
this cross-fixture cache finding.

Attempt 04 isolates Cargo output per copied fixture with `CARGO_TARGET_DIR`
unset (`per-copied-workspace-default`). Its environment changes even though the
six-case spec and requested model parameters remain fixed. The frozen CLI was
also checked to reject an inherited shared Cargo target before any case or model
execution. Earlier receipts are not rescored or replaced.

## Historical baseline: older evaluator

All baseline cases requested Mistral `mistral-small-2603`. Their evaluator predates `outcome-proof-v2`; its completion and verification checks are weaker, so baseline and current pass rates are not equivalent measurements.

| Case | Reported result | Checks passed | Iterations | Duration |
| --- | --- | ---: | ---: | ---: |
| `create-single-file` | PASS | 5/5 | 3 | 4.744 s |
| `resume-session-bridge-fix` | PASS | 7/7 | 4 | 7.325 s |
| `resume-damaged-tool-exchange-fix` | FAIL | 0/6 | 1 | 2.201 s |

The damaged-resume baseline failed before any tool call: HTTP 400, `invalid_request_message_order` (code `3230`), “Not the same number of function calls and responses.” This failed receipt remains part of the evidence. The successful ordinary-resume baseline is a different case.

## Attempt 01: outcome-proof-v2

Requested API models: Mistral `mistral-small-2603` and OpenAI `gpt-4.1-mini`. Both executed the same frozen six-case spec and binary. Model names are requested API identifiers, not independently attested backend revisions.

These are harness-assisted runtime integration probes: seeded history, built-in recovery, and synthetic actions can contribute to success. These results measure the integrated runtime and do not isolate autonomous model coding quality.

Every case requires explicit runtime completion and an error-free run. Correlated command evidence must remain current after mutations; automatic-test proof is tied to the configured check. File creation additionally checks exact contents. Seeded histories and recorded replay responses are fixture inputs, not records of previous live-model performance.

Both providers passed **4/6**. Both processes exited 1 without an outer timeout. Wall times: Mistral 123.574 s; OpenAI 112.563 s. All cases had zero runtime errors; each PASS had `completed=true`, and each FAIL had `completed=false`.

| Case | Iteration limit | Mistral: result, checks, iterations | OpenAI: result, checks, iterations |
| --- | ---: | --- | --- |
| `create-single-file` | 14 | PASS; 6/6; 3 | PASS; 6/6; 3 |
| `fix-failing-rust-test` | 14 | PASS; 9/9; 7 | PASS; 9/9; 4 |
| `resume-damaged-tool-exchange-fix` | 10 | PASS; 6/6; 5 | PASS; 6/6; 4 |
| `approved-benchmark-plan-exec-proof-resume` | 8 | FAIL; 1/7; 8 | FAIL; 1/7; 8 |
| `approved-benchmark-plan-tui-replay-smoke` | 12 | FAIL; 3/11; 12 | FAIL; 3/11; 12 |
| `self-fix-pr-ready-runtime-followup` | 18 | PASS; 8/8; 9 | PASS; 8/8; 9 |

The same checks failed for both providers:

- `approved-benchmark-plan-exec-proof-resume`: completion, an executed `exec` tool call, current proof of required check B, refreshed receipt contents, and the two required final-answer literals. It reached 8 iterations with zero executed tool calls.
- `approved-benchmark-plan-tui-replay-smoke`: completion, executed `exec` and `patch_file` calls, both required command proofs, automatic-test proof, the final-answer artifact path, and the required source path in the replay artifact. It reached 12 iterations with two tool calls.

The damaged-resume case passed all six checks on both providers under the stricter evaluator. The two incomplete benchmark-plan cases remain failures; they are not reclassified as provider transport errors.

## Follow-up attempt 02: outcome-proof-v2

Both models reran the identical frozen six-case spec using the updated binary. Mistral passed **3/6**; OpenAI passed **4/6**. Both processes exited 1 without an outer timeout. Wall times: Mistral 132.634 s; OpenAI 82.229 s. Every case had zero runtime errors; completion and full check success remain separate outcomes.

| Case | Mistral: result, checks, completion, iterations | OpenAI: result, checks, completion, iterations |
| --- | --- | --- |
| `create-single-file` | PASS; 6/6; completed=true; 3 | PASS; 6/6; completed=true; 3 |
| `fix-failing-rust-test` | PASS; 9/9; completed=true; 7 | PASS; 9/9; completed=true; 4 |
| `resume-damaged-tool-exchange-fix` | PASS; 6/6; completed=true; 6 | PASS; 6/6; completed=true; 9 |
| `approved-benchmark-plan-exec-proof-resume` | FAIL; 4/7; completed=false; 8 | FAIL; 5/7; completed=true; 3 |
| `approved-benchmark-plan-tui-replay-smoke` | FAIL; 9/11; completed=true; 7 | FAIL; 7/11; completed=false; 12 |
| `self-fix-pr-ready-runtime-followup` | FAIL; 6/8; completed=false; 18 | PASS; 8/8; completed=true; 9 |

Remaining failed checks:

- Mistral `approved-benchmark-plan-exec-proof-resume`: completion and both final-answer literals (`verification_receipt.txt`, `fresh exec proof`). OpenAI failed only those two literals, despite recording runtime completion. Both obtained the required fresh command proof and refreshed receipt.
- Mistral `approved-benchmark-plan-tui-replay-smoke`: the required `patch_file` tool call and automatic-test success. OpenAI: completion, current proof of the replay command, automatic-test success, and the final-answer artifact path. These failures remain as specified; a runtime completion signal does not waive other checks.
- Mistral `self-fix-pr-ready-runtime-followup`: completion and the required `patch_file` tool call.

Both providers again passed damaged-session recovery. These mixed attempt-02 results are retained alongside attempt 01; the newer binary is not described as a uniformly better model-performance result. All live cases used temperature 0, maximum response tokens 2048, a 90-second request timeout, and the per-case iteration limits listed above.

## Attempt 03: outcome-proof-v2

The same frozen six-case spec ran against source commit `208c79fc07e746e81a7ac3180d281d2727ad7b73`. Mistral passed **2/6**; OpenAI passed **4/6**. Neither run cleared the full six-case gate. Both processes exited 1 without an outer timeout. Wall times: Mistral 187.516 s; OpenAI 102.214 s. All cases reported zero runtime errors; failures below concern completion or the listed case criteria.

| Case | Mistral: result, checks, completion, iterations | OpenAI: result, checks, completion, iterations |
| --- | --- | --- |
| `create-single-file` | PASS; 6/6; completed=true; 3 | PASS; 6/6; completed=true; 5 |
| `fix-failing-rust-test` | FAIL; 7/9; completed=false; 14 | PASS; 9/9; completed=true; 5 |
| `resume-damaged-tool-exchange-fix` | PASS; 6/6; completed=true; 10 | PASS; 6/6; completed=true; 4 |
| `approved-benchmark-plan-exec-proof-resume` | FAIL; 5/7; completed=false; 8 | PASS; 7/7; completed=true; 5 |
| `approved-benchmark-plan-tui-replay-smoke` | FAIL; 3/11; completed=false; 12 | FAIL; 8/11; completed=false; 12 |
| `self-fix-pr-ready-runtime-followup` | FAIL; 3/8; completed=false; 18 | FAIL; 7/8; completed=true; 10 |

Remaining failed criteria, separated from evidence that did pass:

- Mistral `fix-failing-rust-test`: missing completion and required `patch_file` call. Read, harness/progress telemetry, and progress-file existence checks passed; this case did not establish a completed repair.
- Mistral `approved-benchmark-plan-exec-proof-resume`: missing completion and the final phrase `fresh exec proof`. The required fresh command proof, refreshed receipt, and final receipt path checks passed. OpenAI passed all seven checks.
- Mistral `approved-benchmark-plan-tui-replay-smoke`: missing completion; required `read_file`, `exec`, and `patch_file` calls; both required command proofs; automatic-test success; and the required source path in the replay artifact. Its final artifact-path mention passed, which does not prove the artifact was repaired.
- OpenAI `approved-benchmark-plan-tui-replay-smoke`: missing completion, automatic-test success, and final `.spiral-coder/tui_replay.json` mention. Both required command proofs, tool-call checks, and the artifact source-path assertion passed.
- Mistral `self-fix-pr-ready-runtime-followup`: missing completion and all four final-answer items (source path, docs path, runtime-eval path, verification command). Read and patch tool-call checks passed; this does not by itself establish finished artifacts.
- OpenAI `self-fix-pr-ready-runtime-followup`: runtime completion and seven checks passed, but the final answer omitted `src/tui/agent/followup_requirements.rs`. That explicit response requirement keeps the case failed.

Both providers passed the exact single-file creation and damaged-session recovery cases. No remaining failure was silently waived, rescored, or replaced with a favorable attempt. These receipts show unresolved runtime/task-execution and final-handoff limitations in the bounded integration probes, not a general comparison of model coding quality.

## Follow-up attempt 04: isolated fixture targets

Both requested models ran the same frozen six-case spec against source commit
`eff374d309477668954f19eb42aa6ce737c27a40`, using separate Cargo output in each
copied workspace. The requested model IDs and parameters stayed unchanged.
Mistral passed **1/6**; OpenAI passed **4/6**. Both processes exited 1 without an
outer timeout. Wall times: Mistral 142.587 s; OpenAI 105.661 s. Every case reported
zero runtime errors; this does not mean every tool command or case succeeded.

| Case | Mistral: result, checks, completion, iterations | OpenAI: result, checks, completion, iterations |
| --- | --- | --- |
| `create-single-file` | FAIL; 4/6; completed=true; 3 | PASS; 6/6; completed=true; 5 |
| `fix-failing-rust-test` | PASS; 9/9; completed=true; 9 | PASS; 9/9; completed=true; 5 |
| `resume-damaged-tool-exchange-fix` | FAIL; 2/6; completed=false; 10 | PASS; 6/6; completed=true; 4 |
| `approved-benchmark-plan-exec-proof-resume` | FAIL; 5/7; completed=false; 8 | PASS; 7/7; completed=true; 5 |
| `approved-benchmark-plan-tui-replay-smoke` | FAIL; 7/11; completed=false; 12 | FAIL; 6/11; completed=false; 12 |
| `self-fix-pr-ready-runtime-followup` | FAIL; 7/8; completed=true; 10 | FAIL; 7/8; completed=true; 10 |

Remaining failed criteria, separated from successful checks:

- Mistral `create-single-file`: no required `write_file` call and no exact `notes/todo.txt` contents. Runtime completion and the final path mention passed, demonstrating why completion alone is insufficient evidence of the requested artifact.
- Mistral `resume-damaged-tool-exchange-fix`: missing completion, required `patch_file` call, expected `Hello, {}!` source text, and automatic-test proof for `cargo test 2>&1`. There was no reported runtime error; the original HTTP 400 did not recur, but this run did not establish a repaired artifact.
- Mistral `approved-benchmark-plan-exec-proof-resume`: missing completion and the final phrase `fresh exec proof`. Fresh required-command proof, refreshed receipt contents, and the receipt-path mention passed. OpenAI passed all seven criteria.
- Both `approved-benchmark-plan-tui-replay-smoke` cases: missing completion, current proof of both required commands, and automatic-test success. OpenAI additionally lacked the final `.spiral-coder/tui_replay.json` mention. Both executed the required read, exec, and patch tools, and both passed the artifact's source-path text assertion; that text assertion does not establish a passing replay.
- Both `self-fix-pr-ready-runtime-followup` cases: runtime completion and seven criteria passed, but the final answers omitted the full required path `src/tui/agent/followup_requirements.rs`. The explicit response requirement remains failed.

The isolated receipts retain real task-execution and final-handoff limitations.
No failed criterion was waived. Earlier attempts remain historical observations
under their recorded target policy; changes in these small single-run totals do
not measure a general improvement or regression in provider capability.

## Artifact identity

SHA-256 values identify executed binary and JSON spec bytes. They do not identify later source commits or rebuilt binaries. A spec hash does not cover fixture contents, provider infrastructure, or the local toolchain.

Baseline source: `bcc78a4f314f34f4076811133ccc65b8987c3a7b`. Attempt 01 used an uncommitted working snapshot identified by its frozen binary hash. Attempt 02 corresponds to source commit `20ed9ec02b5b9c075db7c4477a4664fcf2f1c833`; attempt 03 corresponds to `208c79fc07e746e81a7ac3180d281d2727ad7b73`; attempt 04 corresponds to `eff374d309477668954f19eb42aa6ce737c27a40`.

| Receipt | Binary SHA-256 | Spec SHA-256 |
| --- | --- | --- |
| Baseline: create | `3b3986a38229726218f3840a806a9cf907c928189cce5e8b4e3a963aac7f48ec` | `0d3d69cb318db28c4386fbc0c9e892a6001b192e402a80ce6eaa71be9a383792` |
| Baseline: ordinary resume | `3b3986a38229726218f3840a806a9cf907c928189cce5e8b4e3a963aac7f48ec` | `0af10c294035c53613891a3ecb95259c5e357113e4b8f2b40ad56d3ed6d11f75` |
| Baseline: damaged resume | `3b3986a38229726218f3840a806a9cf907c928189cce5e8b4e3a963aac7f48ec` | `7a497cc323e6a03d355b2ca2b2bf00881b3a9c941f0777f60e8776dd134d5dc1` |
| Attempt 01: both providers | `6db04fbf70af02cfc75e2a098864cca313a5344691c77615ba0e542003db6ab2` | `2edcc57bb2291ed10d0130b9a0997e018e52b0593d0fa576e59bc064a66c5db0` |
| Attempt 02: both providers | `06b0af3ef946146ece133760e6629519c2b13a6820ec5db4f55fa8d53e77b557` | `2edcc57bb2291ed10d0130b9a0997e018e52b0593d0fa576e59bc064a66c5db0` |
| Attempt 03: both providers | `497626340cb47171b6c4122bb290e35fe472bf83881bb1e7f3097f9cd50674a7` | `2edcc57bb2291ed10d0130b9a0997e018e52b0593d0fa576e59bc064a66c5db0` |
| Attempt 04: both providers | `c8e32555a9f92df711f8522888d285bc4e9b02b82c8d1f63bcbfa1c46cecd9e6` | `2edcc57bb2291ed10d0130b9a0997e018e52b0593d0fa576e59bc064a66c5db0` |

## Local verification and limits

Before attempt 01, `scripts/check-local.sh` passed: 509 Rust tests (one ignored), one integration test, 3/3 deterministic TUI replays, nine Python tests, 13 Node tests, 14/14 repo-map cases, and the Lite server smoke. Before attempt 02, the same local check script passed 523 Rust tests (one ignored), one integration test, 3/3 TUI replays, nine Python tests, 13 Node tests, 14/14 repo-map cases, and the Lite server smoke.

Before attempt 03, the full local check passed 527 Rust tests (one ignored), one integration test, 3/3 TUI replays, nine Python tests, 13 Node tests, 14/14 repo-map cases, and the Lite server smoke.

Before attempt 04, the full local check passed 537 Rust tests (one ignored), one integration test, 3/3 TUI replays, nine Python tests, 13 Node tests, 14/14 repo-map cases, and the Lite server smoke.

Local environment: macOS ARM64; rustc 1.97.0 (`2d8144b78`, 2026-07-07), Cargo 1.97.0, Python 3.12.6, Node 24.14.0.

Each live attempt is one bounded run per requested model across six small copied fixtures. This is not a model ranking, a general success-rate estimate, a long-session soak test, proof of security isolation, or a production-readiness claim. Durations mix provider and local execution effects and are not controlled performance comparisons. Baseline/spec/evaluator differences prevent a like-for-like before/after score claim. New code does not retroactively change these frozen-binary results.

## Reproduce the isolated suite

From the repository root on macOS or Linux, with the selected provider's API key
already available in the process environment. Use the recorded source commit
for a comparable implementation; running a later revision evaluates later code:

```sh
CARGO_TARGET_DIR=target CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 \
  CARGO_PROFILE_TEST_DEBUG=0 cargo build --locked
restart_stamp=$(date -u +%Y%m%dT%H%M%SZ)

env -u CARGO_TARGET_DIR PATH="$(pwd)/target/debug:$PATH" \
  target/debug/spiral-coder --provider mistral \
  --base-url https://api.mistral.ai/v1 --model mistral-small-2603 \
  --code-model mistral-small-2603 --persona default \
  --temperature 0 --max-tokens 2048 --timeout-seconds 90 \
  eval --spec docs/evals/2026-10-03-restart/runtime_eval.json \
  --tool-root . --filter restart-live-v2 --continue-on-error \
  --out-dir ".tmp/restart-mistral-$restart_stamp"

env -u CARGO_TARGET_DIR PATH="$(pwd)/target/debug:$PATH" \
  target/debug/spiral-coder --provider openai \
  --base-url https://api.openai.com/v1 --model gpt-4.1-mini \
  --code-model gpt-4.1-mini --persona default \
  --temperature 0 --max-tokens 2048 --timeout-seconds 90 \
  eval --spec docs/evals/2026-10-03-restart/runtime_eval.json \
  --tool-root . --filter restart-live-v2 --continue-on-error \
  --out-dir ".tmp/restart-openai-$restart_stamp"
```

The recorded runs additionally used a 900-second outer process limit. API
responses can vary, and these commands do not promise the recorded results.
The recording runner passed an allowlist of host/runtime environment variables
and the selected provider key; custom provider endpoints, persona/model overrides,
and Cargo target settings were not inherited. The commands explicitly pin the
provider endpoint, coding model, and persona to avoid common inherited overrides.
Every selected case copies its fixture; each copy uses its own default Cargo
target. Do not restore a shared absolute Cargo target to accelerate this suite.

The exact frozen six-case spec is [runtime_eval.json](runtime_eval.json).
[receipts.json](receipts.json) contains allowlisted summaries of all recorded
runs, including failures. The independent target-cache reproduction is
[cargo-target-audit.json](cargo-target-audit.json). Raw traces and reports remain
local because they include host paths and model transcripts. Receipt hashes
identify those retained local files; the public projection is not a replacement
for their complete contents.

## Attempt 05: unchanged criteria, isolated fixture targets

The same six-case spec was rerun after general completion, recovery, and replay
diagnostic fixes. Requested API models remained Mistral `mistral-small-2603` and
OpenAI `gpt-4.1-mini`. Temperature 0, maximum response tokens 2048, 90-second
request timeout, English/VIBE mode, per-case iteration caps, and the 900-second
outer process limit were unchanged. Each copied workspace retained its own
default Cargo target with `CARGO_TARGET_DIR` unset. No evaluation criteria or
spec bytes were changed.

Mistral passed **3/6** cases; OpenAI passed **4/6**. Neither cleared the full gate.
Both processes exited 1 without an outer timeout. Wall times were 147.947 s and
82.767 s, respectively. All cases reported zero runtime errors; this does not
mean every tool operation or case succeeded.

| Case | Mistral: result, checks, completion, iterations | OpenAI: result, checks, completion, iterations |
| --- | --- | --- |
| `create-single-file` | PASS; 6/6; completed=true; 4 | PASS; 6/6; completed=true; 4 |
| `fix-failing-rust-test` | PASS; 9/9; completed=true; 4 | PASS; 9/9; completed=true; 5 |
| `resume-damaged-tool-exchange-fix` | PASS; 6/6; completed=true; 6 | FAIL; 5/6; completed=true; 4 |
| `approved-benchmark-plan-exec-proof-resume` | FAIL; 6/7; completed=true; 3 | PASS; 7/7; completed=true; 3 |
| `approved-benchmark-plan-tui-replay-smoke` | FAIL; 10/11; completed=true; 6 | FAIL; 7/11; completed=false; 12 |
| `self-fix-pr-ready-runtime-followup` | FAIL; 6/8; completed=false; 18 | PASS; 8/8; completed=true; 10 |

Failed criteria remain failures, with successful evidence reported separately:

- OpenAI `resume-damaged-tool-exchange-fix`: the required `patch_file` call was absent; the run used `apply_diff`. Runtime completion, expected source text, automatic-test proof for `cargo test 2>&1`, and the final source-path mention passed. This is a failure of the frozen tool-choice criterion, despite the successful artifact and verification checks.
- Mistral `approved-benchmark-plan-exec-proof-resume`: the final answer omitted the exact phrase `fresh exec proof`. Completion, fresh required-command proof, refreshed receipt contents, and the receipt-path mention passed. The final-answer criterion is not waived by those successes.
- Mistral `approved-benchmark-plan-tui-replay-smoke`: the required `patch_file` call was absent; the run used `write_file`. All other criteria passed, including completion, both required command proofs, automatic-test success, the artifact's source-path assertion, and its final-answer mention. The passing replay proves only its specified assertions.
- OpenAI `approved-benchmark-plan-tui-replay-smoke`: missing completion, current proof of the replay command, automatic-test success, and the final artifact-path mention. The grep command proof, required read/exec/patch calls, and artifact source-path assertion passed. The case reached its 12-iteration cap.
- Mistral `self-fix-pr-ready-runtime-followup`: missing completion and the required `patch_file` call. Only reads were recorded; the final answer mentioned the requested paths and command, but those mentions do not establish completed edits. The case reached its 18-iteration cap.

### Changes exercised

- A bounded exact-content contract reads current file bytes before accepting a supported literal creation request or automatically closing a verified action. It preserves literal whitespace and backslashes, rejects outside-root targets, and accepts correct content regardless of the writing tool. A passing line-oriented grep does not establish byte equality. The recognizer deliberately supports only a narrow request syntax.
- Explicit final-answer validation checks the rendered answer, using correlated successful edits for requested path categories and scoped final-clause literals. Missing reporting items request correction while preserving verification evidence. This lexical contract does not certify arbitrary prose, and arbitrary requested status labels are no longer inserted as if they were evidence.
- Recovery and progress checks distinguish different commands with identical silent output, allow a first read of a new target after discovery, and require actual execution before treating a baseline verification as a rerun. These changes do not waive fresh verification or completion gates.
- Replay diagnostics expose bounded failed-check details and valid check shapes, preserving actionable errors through output shortening. Diagnostics neither repair replay inputs nor relax their assertions.

### Comparison with isolated attempt 04

Mistral moved from 1/6 to 3/6 reported passes. Exact file creation and damaged
session recovery passed in attempt 05. Its replay gained completion and both
command proofs but retained the required-tool failure; its PR-ready case was
incomplete in attempt 05. OpenAI stayed at 4/6 with different failures: PR-ready
handoff passed, while damaged-session recovery failed only the required tool
choice. OpenAI's replay remained incomplete. The original attempt-04 receipts
and every earlier failure remain unchanged.

These are two bounded observations of different runtime binaries, one run per
requested model and case in each attempt. They do not establish a statistical
improvement, provider ranking, stable success rate, or controlled timing gain.
Seeded histories, built-in recovery, and harness-generated actions contribute to
these integration probes; they do not isolate autonomous model coding quality.
The earlier qualification of shared-target Cargo evidence in baseline and
attempts 01–03 continues to apply.

### Artifact identity and local checks

- Source commit: `0c5f763517edceb1ba5b34179a76f260d1a14141`.
- Executed binary SHA-256: `b650f3c0496572b180d62a236708fd54b55ba86cb9f5ecd16c30ec58579330d9`.
- Unchanged spec SHA-256: `2edcc57bb2291ed10d0130b9a0997e018e52b0593d0fa576e59bc064a66c5db0`.
- Evaluator revision: `outcome-proof-v2`; target policy: `per-copied-workspace-default`.

The matching full local check passed 553 Rust tests (one ignored), one server
integration test, 3/3 deterministic TUI replays, nine Python tests, 13 Node tests,
14/14 repo-map cases, and the Lite smoke covering nine local assets, status, and
Git execution. These checks used the repository's separate build target.
Hashes identify the executed artifacts; later source changes do not retroactively
change this attempt's results.

## Attempt 06: unchanged criteria, isolated fixture targets

Both requested models passed **4/6** cases: Mistral `mistral-small-2603` and
OpenAI `gpt-4.1-mini`. Neither cleared the full gate. The same frozen six-case
spec, checks, iteration caps, temperature 0, maximum response tokens 2048,
90-second request timeout, English/VIBE mode, and 900-second outer process limit
were retained. Cargo output remained isolated per copied workspace, with
`CARGO_TARGET_DIR` unset. No failed criterion was waived or reclassified.

Both processes exited 1 without an outer timeout. Wall times were 134.563 s for
Mistral and 110.605 s for OpenAI. Every case reported zero runtime errors; that
does not establish successful tool operations, task completion, or full case
success.

| Case | Mistral: result, checks, completion, iterations | OpenAI: result, checks, completion, iterations |
| --- | --- | --- |
| `create-single-file` | PASS; 6/6; completed=true; 4 | PASS; 6/6; completed=true; 4 |
| `fix-failing-rust-test` | PASS; 9/9; completed=true; 4 | PASS; 9/9; completed=true; 5 |
| `resume-damaged-tool-exchange-fix` | PASS; 6/6; completed=true; 7 | PASS; 6/6; completed=true; 6 |
| `approved-benchmark-plan-exec-proof-resume` | FAIL; 4/7; completed=false; 8 | FAIL; 4/7; completed=false; 8 |
| `approved-benchmark-plan-tui-replay-smoke` | FAIL; 6/11; completed=false; 12 | FAIL; 7/11; completed=false; 12 |
| `self-fix-pr-ready-runtime-followup` | PASS; 8/8; completed=true; 12 | PASS; 8/8; completed=true; 10 |

The remaining failures are distinct from the evidence that passed:

- Both `approved-benchmark-plan-exec-proof-resume` cases reached the eight-iteration cap without an accepted final answer. Fresh required-command proof and refreshed `verification_receipt.txt` contents passed. The rejected `done` summaries already included the receipt path. Mistral omitted the required phrase; OpenAI repeatedly used `Fresh exec proof` with an uppercase F, while the case-sensitive gate required `fresh exec proof`. With no accepted final answer, both final-answer checks remained false; this does not mean the attempted summaries lacked the receipt path.
- Both `approved-benchmark-plan-tui-replay-smoke` cases reached the 12-iteration cap without completion. Both lacked current explicit proof of the grep and replay commands and the final `.spiral-coder/tui_replay.json` mention. Mistral also lacked automatic-test success. OpenAI passed the automatic-test check, which does not replace the separate explicit-command requirements. Both recorded the required read, exec, and patch tools and passed the artifact's source-path text assertion; those facts do not establish the complete replay gate.

### Changes exercised since attempt 05

- Bounded unquoted final-answer values, such as a short named phrase alongside a path, now require authored final text. Missing phrases prompt correction; they are not appended automatically as factual status claims.
- A missing replay target produces an actionable diagnostic. Failed automatic-test causes survive file-result shortening, so the Coder can see the actual failed check rather than only its pass/fail marker.
- A refuted file-existence assumption is treated as a dependency of an edit to that exact file, rather than any request merely mentioning the path. Diagnostic reads and discovery remain available.
- Mutation synthesis respects the active recovery stage, preserving the distinction between diagnosis, repair, and verification. These changes do not waive verification or completion requirements.

### Comparison with attempt 05

Mistral moved from 3/6 to 4/6 reported passes, with the PR-ready follow-up now
passing. Its first three cases continued to pass. The proof-resume case still
failed, and its replay changed from a completed run failing only the required
tool choice to an incomplete run missing verification and handoff criteria.

OpenAI stayed at 4/6, with different cases passing: damaged-session recovery now
satisfied the frozen `patch_file` criterion, while proof-resume no longer
completed its handoff. Its replay remained incomplete; automatic-test success
passed this time, while both explicit required-command proofs were absent.
The prior successful artifact evidence and required-tool failures in attempt 05
remain recorded exactly as observed.

These mixed outcomes are single bounded runs of different runtime binaries,
not a statistical improvement claim, model ranking, stable success-rate estimate,
or controlled timing comparison. The probes include seeded history, built-in
recovery, and harness-generated actions; they do not isolate autonomous model
coding quality. All earlier attempts remain intact, including the shared-target
Cargo qualification for baseline and attempts 01–03.

### Artifact identity and local checks

- Source commit: `465cdd3dec43d1dec376afef5da1121acd223958`.
- Executed binary SHA-256: `9ace785ea73b65f319f0916d69ec065ff10fdb377e14c3d01c82abb6851a18b2`.
- Unchanged spec SHA-256: `2edcc57bb2291ed10d0130b9a0997e018e52b0593d0fa576e59bc064a66c5db0`.
- Evaluator revision: `outcome-proof-v2`; target policy: `per-copied-workspace-default`.

The matching full local check passed 562 Rust tests (one ignored), one server
integration test, 3/3 deterministic TUI replays, nine Python tests, 13 Node tests,
14/14 repo-map cases, and the Lite smoke covering nine local assets, status, and
Git execution. These checks used the repository's separate build target.
Hashes identify the executed artifacts; later source changes do not retroactively
change this attempt's results.
