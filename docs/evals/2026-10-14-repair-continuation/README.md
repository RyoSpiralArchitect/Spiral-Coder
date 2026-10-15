# Repair diagnosis through Coder and Observer

This record retains all 27 episodes from development rounds 8–10. Coder and
Observer use the same selected model and one shared wire-request cap. The unit
is the completed original task, including preserved artifacts, exact native
checks and authored final report.

## Changes and deterministic evidence

Round-7 replay failures exposed three runtime problems: the progress gate's
failed-edit reread exception covered only Rust source paths; empty-check errors
omitted the input spec identity; and a nullable Observer display field could
hide the actual invalid action kind until the only repair attempt.

The first candidate, `4b84ba6`, permits one diagnostic read of the failed artifact,
shares pending edit/automatic-test recovery with both roles, retains replay input
identity, and keeps optional presentation separate from strict action validation.

Rounds 8–9 then exposed a continuation defect: a successfully reread failed edit
returned to Diagnose after the Observer handoff. Candidate `fae3ea6` restores Fix
from that correlated read while retaining the unresolved edit and all proof
requirements. An actual HTTP TUI regression executes the repair in one Coder
request after Observer advice and still leaves verification incomplete. A later
failure retains its own recovery stage. The shared prompt also explains separate,
unchanged exact exec calls before a completion attempt.

Round 9 additionally revealed an exec location marker attached to newline-free
stdout. The wrapper now separates the marker; actual shell tests preserve exit
status and detect both in-root directory changes and an outside-root destination.

`bash scripts/check-local.sh` passes on the final runtime: **727 Rust tests,
1 ignored**, 1 integration test, TUI replay 3/3, Python 15, Web 61, repo-map 14/14,
syntax checks and the Lite smoke. See [verification.json](verification.json).
These are runtime-correctness checks, not model-quality evidence.

## Frozen and adaptive live panels

The unchanged suite is `tests/fixtures/coupled-loop-v1.json`, SHA-256
`3c3c24ddec901a769158b6cab126797bdb2427243387b0b9b315e5e3182e4752`.
Its three caps remain **10 / 8 / 12**. Temperature is 0, output limit 2048 tokens,
request timeout 90 seconds, and episode timeout 600 seconds. Model selection,
fixtures, case assertions and the `patch_file` requirement are unchanged.

| Model in both roles | R8 | R9 | R10 |
| --- | ---: | ---: | ---: |
| `mistral-small-2603` | 2/3 | 1/3 | 1/3 |
| `gpt-4.1-mini` | 2/3 | 2/3 | 3/3 |
| `gemini-3.5-flash-lite` | 2/3 | 1/3 | 1/3 |
| Total strict task success | **6/9** | **4/9** | **5/9** |
| Wire requests | 58 | 62 | 60 |
| Runtime-completed tasks | 7/9 | 5/9 | 6/9 |

Rounds 8–9 were predeclared as two unchanged panels on one binary: **10/18**
strict successes and 120 wire requests. Round 10 is a separate adaptive follow-up
after the additional repairs, not a third repetition of that candidate. It is
**5/9**, with 60 wire requests. No episode timed out or exceeded its shared cap.

- R8–R9 binary: `fb302d2b11be725d2ce38ff9da32b6c6c4bb04e198597a68b6af2bc191c03d59`
- R10 binary: `bd716703b9fd3ee0871b234e4e56b129330ddb9ede51bd88aef9313dc96b7340`

All **9/9** replay outputs preserve complete old JSON, including nested evidence
and extension fields. **7/9** contain a new case that passes full old+new replay.
Only **1/9** passes the complete frozen replay task: round-10 GPT, in 11/12 wire
requests, including its native exact checks and final report. Its trace includes
failed edits followed by actual repair and a completed Observer continuation.

All three Mistral replay tasks complete and pass artifact/proof checks, but use
`apply_diff` instead of the required `patch_file`; they remain strict failures.
Gemini's valid replay artifacts still exhaust the cap without separate exact
proof and reporting. In the final panel Gemini also fails the source-repair case
after a baseline test failure leads into unrelated Git diagnostics. Mistral's
proof/report case misses the exact case-sensitive final phrase. These failures
remain in [the ledger](failure-modes.json), not hidden behind artifact pass rates.

## Qualification boundary and receipts

The PR remains Draft. These small development panels do not establish improved
reliability, general nonregression, model rankings, or the Observer's causal
contribution. There is no Observer-disabled control in these rounds. Full task
success remains unstable despite the deterministic repairs and GPT's final 3/3.

[receipts.json](receipts.json) contains every fixed check, cap, wire receipt,
available numeric usage/duration, role count, assist outcome and Observer parser
failure. [raw-manifest.json](raw-manifest.json) hashes the local binaries,
predeclarations, launchers, logs, native sessions, traces and independent oracles.
Raw provider continuation metadata remains local and is not embedded here.

Earlier results are unchanged: [rounds 5–7](../2026-10-14-task-obligations/README.md)
and [rounds 1–4](../2026-10-14-coupled-loop/README.md). Across the PR this is now
**90 retained episodes and 631 wire requests**, with no discarded failed panel.
