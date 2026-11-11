# Context compression for repeated repair

Runtime `626cc02497fc9bed4f602850c2e1eb4d0df02cc4` compresses provider
requests while keeping the native transcript intact. Recovery, acceptance,
approval and budget gates continue to use the original exchanges. Coder
history projection and the shared lossless string pool are documented in
[the state schema](../../state-schema.md#3-session-persistence).

## Matched model-free controls

`capture.py` uses the same seed, task and canonical workspace with the previous
executor (`17ee4e6`) and the compressed executor. A loopback HTTP endpoint
captures one Coder request and deliberately returns HTTP 400. This is a request
capture, not a successful model run. There are **zero external provider calls,
zero new native tool actions and no candidate changes**.

| Surface | Previous bytes | Compressed bytes | Reduction |
| --- | ---: | ---: | ---: |
| Coder messages, report seed | 146,737 | 89,772 | 38.8% |
| Complete Coder request including schemas | 155,118 | 98,153 | 36.7% |
| Observer report-seed prompt | 62,164 | 43,256 | 30.4% |
| Observer R23 iteration 1 prompt | 67,607 | 49,154 | 27.3% |
| Observer R23 iteration 2 prompt | 70,857 | 51,813 | 26.9% |
| Observer R23 iteration 3 prompt | 68,744 | 50,619 | 26.4% |

All four native Observer packets and native-message projections are equal
across executors. Coder wire tool schemas are byte-equivalent under canonical
JSON serialization. The new Coder view compresses 51 old exchanges; its 64 KiB
soft target is still exceeded by protected context and current instructions.
The telemetry explicitly records this overflow rather than deleting mandatory
context. These are serialized UTF-8 byte counts, not provider-billed tokens.
See [capture-results.json](capture-results.json).

## Bounded live continuation

[The plan](live-plan.json) was committed before one invocation using
`gpt-5.6-luna` for both roles. It preserves the previous report probe's exact
seed/task bytes, workspace contents, sampling, output caps and limits of two
task/wire requests, one Observer reserve, zero transport repairs and 210 seconds.

The compressed run **completed with one Coder request in 13.392 seconds**. Its
only new native action was accepted `done`; no edit or verification was run.
The summary included both required literal commands and accurate native test
counts. All five prior checks remained current. All **64 original call messages
and 64 result messages survived exact JSON-value comparison in the saved
session**, including their local metadata. Candidate contents did not change.

The previous probe stopped incomplete after two requests in 19.681 seconds.
Its Coder input estimate was 38,746 tokens; this probe's was 24,511. The new
Coder provider request itself took 11.456 seconds versus 9.133 seconds previously;
the lower total time came with avoiding another request. One sample does not
establish lower provider latency or a causal success-rate improvement.

The new run completed before Observer needed to respond. Observer compression
is covered by native projection parity and the general/diagnostic/handoff unit
tests; this probe supplies no new live Observer response. This is an assisted
report continuation from an already verified candidate, **not a fresh original
#49 repair, a new full qualification or proof of indefinite-run reliability**.
See [comparison.json](comparison.json), [live-results.json](live-results.json)
and [retention-audit.json](retention-audit.json).

## Validation and retained limitations

`bash scripts/check-local.sh` passed: 1,137 main Rust tests (two ignored), six
integration tests, 20/20 TUI replay cases, 97 Python tests, 80 Web tests, five npm
tests, 14 repository-map cases and the Lite smoke. The final production build
has no compiler warnings. Regression coverage exercises three simulated resume
iterations, exact shared-text reconstruction, current reads/failures, stale
checks after edits, signed/ambiguous/incomplete exchanges and approval refusals.

The first failing test assertion and capture-driver setup error are described
in [validation.json](validation.json); neither was a provider trial. Raw captures,
native reports, trace and session remain outside the repository under
`~/.cache/spiral-coder/context-compression-artifacts-20261111/`.

## Fresh long run

Both Luna roles completed the original-tree repair in two iterations and
70 wire requests. All five final native checks and six independent groups
passed. The frozen ledger eligibility gate remains failed.
[R24 live run, context measurements and receipts](long-run/README.md)

Retained native storage and its projection CPU work can grow. Mandatory and
signed context may exceed the soft target. Previously pruned sessions retain
only their available history.
