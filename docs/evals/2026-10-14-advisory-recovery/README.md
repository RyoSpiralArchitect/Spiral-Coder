# Bounded Observer repair and recovery precedence — 2026-10-14

Two runtime candidates continue the [Observer recovery record](../2026-10-13-observer-recovery/README.md).
Observer can now request one explicit correction of an unusable action contract.
Coder diagnosis no longer competes with trace-derived repeated-read mutation
pressure. **Live qualification remains incomplete; PR #16 stays Draft.**

## Runtime changes

- `events/observer_request.rs` owns the next-action request, original prompt,
  Coder source-context stamp and one repair budget. Invalid JSON, a missing tool,
  an incompatible kind/tool pair or wrong arguments request one model correction.
  The original response remains visible. The runtime never guesses a missing
  tool or turns `quickest_check` prose into executable arguments.
- Repair uses the original packet and real tool schemas, bounded quoted
  validation feedback and the quoted failed response. A second invalid answer
  stops. Valid low-confidence and valid no-tool suggestions do not trigger a
  confidence-raising retry. The normal confidence/action checks still govern
  advisory delivery; Observer does not execute a tool or grant proof.
- Coder context changes, cancellation, errors and replacement Observer work
  expire pending requests. Completed advice is retained only for a continuation
  of the same intent. An empty new response cannot reuse an older successful
  answer. All new request state is transient.
- During diagnosis, repeated-observation MetaHarness/Evaluator pressure and
  learned progress prompt overlays are suspended. Saved policies and current
  verification obligations remain intact. The separate new-repo scaffold
  policy keeps the existing missing-artifact creation allowance.
- Progress and MetaHarness use the same correlated observation window since
  the last successful mutation. Rejected edits and embedded success text in
  blocked exec output do not reset it. A genuine edit permits a fresh read;
  unchanged read loops remain bounded.
- The second candidate makes failed replay checks carry the actual Observer
  parser/action-validation reason. Missing `suggested_tool` is visible beside
  `hint_queued=false`; unknown kinds include the parser's current vocabulary,
  including `edit` and `verify`. This replaces a stale hand-maintained list.
  Assertions, acceptance conditions and artifacts are not rewritten.

## Deterministic checks

`bash scripts/check-local.sh` passes with **693 Rust tests (1 ignored),
1 integration, TUI replay 3/3, Python 12, Web 53, repo-map 14/14, syntax checks
and Lite smoke**. Eleven regressions were added. Four specific counterexamples
were reproduced as failing tests before repair: conflicting diagnosis/mutation
pressure, pre-mutation reads blocking fresh inspection, missing action feedback
in the failed check, and stale Observer kind vocabulary.

A local HTTP mock exercises the actual TUI streaming request/consumer through
initial failure, one correction and advisory queuing. A second invalid response
stops after the second request. Additional tests cover uncertainty, valid
no-tool advice, empty replies, source changes, cancellation and stale completed
advice. Real replay exports `observer_repair_prompt.txt` without making an API
request, so the live repair panel uses the runtime's exact prompt.

The final measured runtime is `493196fd148ea1b66b39415977935f244d7fb7e0`, binary
SHA-256 `87bbccbb40fd4b49c4219db0aecbb6716740a11990bc3c76ca127fe9f30b62f6`.
Candidate 1 is `6a386e31c2b2451ffe1157c6d39e697a18898421`. Subsequent commits
publish documentation and receipts only. Intermediate failed check logs are
hashed in [receipts.json](receipts.json); the new-repo scaffold regressions caught
an overly broad initial suppression before live evaluation.

## Live Observer: initial answer and repair are separate

The [six-case panel](observer-cases.json), models, temperature 0.2, output cap
1200, request timeout 90 seconds and zero HTTP retries remain unchanged.
Initial-answer scoring remains: actual parse, exact expected primary tool and
arguments, and no native tool call. Hint/contract delivery is recorded separately.

The [repair panel](repair-cases.json) freezes three previously captured synthetic
responses: GPT's missing tool, Gemini's malformed JSON and Gemini's unsupported
tool. Each model receives one correction request for each captured response.
Repair passes only when the actual consumer parses the new answer, its primary
action matches the expected tool/arguments, a hint is queued, and no native tool
call is returned. The original invalid response is not rescored as successful.

| Configured model | Initial answer, candidate 1 → final | Captured-response repair, candidate 1 → final |
|---|---:|---:|
| `mistral-small-2603` | 6/6 → 6/6 | 3/3 → 3/3 |
| `gpt-4.1-mini` | 5/6 → 5/6 | 3/3 → 3/3 |
| `gemini-3.5-flash-lite` | 6/6 → 6/6 | 3/3 → 3/3 |

GPT again omits `suggested_tool` in the current-verification initial answer;
the consumer withholds it. All 18 captured-response correction calls return the
expected action and queue both advisory messages. These are separately sampled
repairs of historical responses, not paired retries of the fresh initial-answer
panel. Across both candidates there are **36 initial Observer calls and 18
repair calls**, with no observed HTTP or instrumentation errors.

These live probes use non-streaming OpenAI-compatible HTTP with the exported
TUI prompt and actual Rust consumer. They execute no suggested tool and do not
establish provider-specific TUI streaming reliability or completed Coder work.
The local streaming mock covers the bounded lifecycle separately.

## Unchanged Coder suite

The Coder spec hash is
`2edcc57bb2291ed10d0130b9a0997e018e52b0593d0fa576e59bc064a66c5db0`.
All **33 fixture hashes**, assertions and tool-use requirements are unchanged.
The six caps remain **14 / 14 / 10 / 8 / 12 / 18**, temperature 0, output cap
2048, request timeout 90 seconds and suite wall limit 900 seconds. Each case
uses a fresh copied workspace. All **36 case results** are retained.

| Configured model | Candidate 1 | Final |
|---|---:|---:|
| `mistral-small-2603` | 5/6 | 5/6 |
| `gpt-4.1-mini` | 4/6 | 5/6 |
| `gemini-3.5-flash-lite` | 4/6 | 5/6 |

All final models pass creation, Rust repair, damaged-exchange resume, exact
benchmark proof after resume, and source/docs/eval follow-up with final reporting.
The follow-up case also passed for all three in candidate 1. These are bounded
suite observations, not a causal claim about a particular patch.

The remaining final failure is the TUI replay case for all three, with different
failure modes:

- **Mistral:** creates a valid case, passes its automatic test, and executes both
  exact required checks successfully. The 12-turn cap ends after verification,
  before `done` and the requested final report. Completion remains false.
- **GPT:** repairs missing target data but leaves an assertion comparing the
  selected message ID with the case ID. Verification and completion remain
  missing. The old case also loses the nonempty
  `observer_response.suggestions[1].based_on` provenance array; strict JSON
  preservation is false, and this is not an equivalent optional default.
- **Gemini:** creates a valid case through `write_file` and passes its automatic
  test on the last iteration. It has no successful native `exec` for the exact
  required checks, no required `patch_file` call, and no completed outcome.
  Automatic-test success alone is not promoted to those proof obligations.

Candidate 1 additionally contains GPT's otherwise completed resume repair
failing only the frozen `patch_file` criterion, and Gemini's protocol-gate
exhaustion before repairing the Rust fixture. Those failures remain in the
receipts. Both rounds have zero suite timeouts, run errors and provider retries.
The original replay case is preserved in all three candidate-1 outputs and in
final Mistral/Gemini outputs; final GPT's provenance loss is audited separately
from the frozen acceptance checks.

## Remaining boundaries

The next failures to address are budget-aware final reporting, constructing
replay targets/assertions without discarding prior case evidence, and selecting
required exact verification before the tool budget is exhausted. Repairing
feedback does not make these obligations complete. No iteration cap, fixture,
required tool, assertion or model was changed to improve a score.

The small panels, different candidates and lack of an unchanged final repeat
cannot establish general reliability, model ranking or nonregression. GUI
receipt parity remains separate work. [Failure modes](failure-modes.json)
and [receipts](receipts.json) retain unsuccessful cases and source/binary/spec
identities. The synthetic repair inputs are replayable; raw Coder transcripts,
credentials, opaque continuation signatures and machine-local paths are not
published.
