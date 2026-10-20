# Diagnostic-prefixed baseline review follow-up

PR #31's P1 review identified a deadlock: a failed required baseline could begin
with `⚠ LOGIC ERROR` or another runtime hint, while the requirement gate only
accepted results beginning with `FAILED (exit_code:`. Every following edit then
remained blocked. A Web error digest can also push the status past the advisory
packet's 2,400-character display limit.

Candidate `f420ea43c3577dfd63e090d460c4db03be0c79b6` parses the confirmed
runtime outcome before shortening the result. The requirement gate uses that
correlated outcome. Known diagnostic envelopes are accepted; success/refusal,
approval, unknown errors, malformed headers, and stdout/stderr boundaries cannot
be skipped to find a convenient failure marker. Unrecognized exec results have
`succeeded: null` and cannot establish a failing baseline.

## Deterministic verification

Two new regression tests failed on `da4b087`: an actual Python AssertionError
wrapped by the native result builder left the baseline pending, and a malformed
failure-header suffix was accepted. Both pass with the correction. Coverage also
includes long Web digests, specific recovery hints, correlated IDs, the last
pre-edit attempt, resume, pruning, mutation permission, and the shared native
Coder/Observer/Web projection. Passing a baseline gate alone never completes a
task or supplies current passing verification.

`scripts/check-local.sh` passes: 791 Rust tests (1 ignored), 1 integration test,
TUI replay 3/3, Python 20, Web 79, repo-map 14/14, syntax checks and Lite smoke.
See `verification.json` for the candidate and local log hashes.

## Bounded live regression

This is a **new small task**, not a matched rerun of the earlier repository or
JSON panels. It changes one incorrect greeting suffix, preserving two supplied
unittests and the configured command. The original request explicitly requires
a failing baseline, a native source edit, the exact passing check, and a report
of the before/after outcomes. All initial fixtures failed both assertions; a
known one-character reference fix passed before launching the models.

Coder and Observer share the same selected model and 18-request cap in each run;
Observer and repair calls count against that cap. Temperature 0, output cap 2048,
90-second request timeout, 600-second wall cap. One first attempt per model; no
retries, cap changes, stronger models, or fixture edits after declaration.

| Model for both roles | Requests / assists | Prefixed baseline recognized | Independent original tests | Runtime complete | Full requested handoff |
| --- | --- | --- | --- | --- | --- |
| mistral-small-2603 | 18 / 1 | yes | fail | no | no |
| gpt-4.1-mini | 9 / 0 | yes | pass | yes | no: initial failure omitted |
| gemini-3.5-flash-lite | 18 / 1 | yes | fail | no | no |

**45 requests, 3/3 baseline recognition, 1/3 runtime completions, 0/3 fully
qualified handoffs.** No run hit the requirement gate after recording its actual
baseline failure. Only GPT reached a successful native edit and fresh exact
verification. All three preserved the supplied tests and configuration. The
independent check copies the candidate source into a fresh directory with the
original tests, preventing a candidate test rewrite or stale import cache from
granting a pass. This does not estimate general coding quality.

## Remaining failures

- Mistral repeatedly read the implementation after the failure while recovery
  requested a fresh read of the assertion site in `test_greet.py`. Its Observer
  continuation did not resolve that mismatch; no edit executed.
- Gemini reached the required test-file read, then attempted `write_file` on an
  existing source without a current confirmed read and spent the remaining
  budget in protocol/recovery continuation. No edit executed.
- GPT repaired and verified the source, but its accepted report omitted the
  explicitly requested initial failing outcome. Its structured acceptance rows
  and final text show the post-fix pass. The runtime marked completion; manual
  inspection of the authored report does **not** qualify the whole request.

The review fix removes one false block. It does not solve diagnosis-target
selection, Observer continuation, or semantic coverage of all final-report
clauses. Keep those failures separate from the baseline recognition result;
neither bypassing recovery nor counting a green test alone is a qualification.

## Receipts

- `predeclared.json`: source/binary/prompt/fixture hashes and limits frozen before
  the calls. It includes no inherited metadata from another panel.
- `task.txt`, `greet.py`, `test_greet.py`, `config.txt`: exact original inputs.
- `receipts.json`: per-action outcomes, request usage when supplied by providers,
  assists, independent checks, and the manual authored-report assessment.
- `raw-manifest.json`: hashes of local histories, traces, logs, candidate patches,
  the binary and reproduction scripts. Opaque provider continuation metadata is
  retained locally rather than copied into the published receipts.

Raw files and `prepare.py`, `launch.py`, `collect.py` remain in
`~/.cache/spiral-coder/dogfood-20261021-review-baseline`. Reproduction needs a fresh
output directory and an authorized provider budget. The preceding panel remains
unchanged in [task-proof-json-web](../2026-10-21-task-proof-json-web/README.md).
