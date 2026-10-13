# Observer evidence and advisory recovery — 2026-10-13

Observer now distinguishes an attempted action from an observed successful
result, and offers only suggestions that match the actual Coder tool catalog.
Two runtime candidates were checked against a new frozen Observer panel and
the existing, unchanged six-case Coder suite. This continues the
[reflection/replay-input record](../2026-10-13-reflection-repair/README.md).

## Runtime repairs

- Match native results to preceding tool calls once. Rejected or unknown edit
  results cannot establish a mutation or discharge a documentation follow-up.
  Failed automatic tests preserve the edit and its failure; passing anonymous
  automatic tests supply no exact replay/eval command proof.
- Require a successful runtime header and actual shell/CLI grammar for
  verification. Printed command names, help, a different spec, masked failures,
  `git status`, and `git diff` cannot substitute for a successful required check.
  Success headers take precedence over failure words in stdout.
- Mark legacy edit displays without positive receipts as unconfirmed and
  suggest reading the artifact. Command success requires result metadata
  outside the output fence. These diagnoses are advisory and never grant Coder
  execution or completion authority.
- Give TUI Observer the real tool names and argument schemas, `edit` and `verify`
  kinds, the original intent anchor, and bounded diagnostic lines after long
  output prefixes. The next-action example no longer biases a specific source
  path. An exact remaining check is suggested as `exec`, not an invented tool.
- Validate the primary kind/tool pair and argument types before queuing either
  advisory message. Invalid suggestions stay visible without being forwarded.
  Both messages use that one typed action; the separate `quickest_check` display
  cannot replace it with another instruction. No suggested tool runs here.
- Keep Unicode command/path clipping safe and put the newest failure cause
  before long command text in bounded diagnostic evidence.
- After candidate 1 exposed a display-field failure, treat a non-string optional
  `quickest_check` as absent. Its contents are never converted to tool arguments.
  The prompt explicitly requests a string. This is a deliberate parser repair
  for presentation metadata; JSON syntax and actionable suggestion validation
  remain enforced. Candidate 1 failures are not retroactively rescored.

Packet/prompt code and legacy transcript detection were extracted into focused
modules instead of expanding the high-touch TUI orchestration file. The typed
contract changes are documented in `docs/state-schema.md`.

## Deterministic evidence

Final `bash scripts/check-local.sh` passes: **682 Rust tests (1 ignored),
1 integration, TUI replay 3/3, Python 12, Web 53, repo-map 14/14, syntax checks
and Lite smoke**. Fifteen regressions were added across the two candidates.
Five outcome counterexamples failed against the prior implementation before
repair. Intermediate failures and all check-log hashes remain in the receipts;
one extra regression exposed evidence truncation behind a long Unicode command.

Real TUI replay verifies that unknown tools, mismatched kinds, missing/wrong
arguments and arbitrary display instructions do not enter the Coder request.
Valid edit suggestions do not mutate files or create governor proof. The two
captured Mistral display-field failures now parse and queue the intended primary
read without editing their responses. The captured malformed Gemini JSON still
fails parsing and queues no advisory. A fourth captured replay confirms that
GPT's missing tool queues neither message despite valid JSON. Those are offline
replay results, separate
from the fresh live panels.

## Frozen Observer panel

[Cases and conditions](observer-cases.json) and the
[replay input](observer-replay-input.json) were frozen before the first call.
The six curated situations require an exact primary tool and path/command:
read after a failed patch anchor, documentation follow-up, non-Git fixture
recovery, the exact remaining verification, replay input failure behind long
output, and the original task after continuation messages. Five expect a
specific read; one expects the exact `exec` command. These are explicit
next-action probes, not broad coding tasks.

Every request uses the actual prompt exported by `tui-replay` and the actual
Observer system prompt; every received response is passed through the Rust TUI
consumer. Models are unchanged: `mistral-small-2603`, `gpt-4.1-mini`, and
`gemini-3.5-flash-lite`. Conditions are temperature 0.2, maximum output 1200,
request timeout 90 seconds, one request per case, no HTTP retry or fixed seed.
The probe uses non-streaming OpenAI-compatible HTTP, so it does **not** qualify
TUI streaming transport. It executes no tool and never asks Coder to complete
the suggested action.

A case passes when the actual parser accepts the response, the primary tool
and exact required arguments match the frozen case, and no native tool call is
returned. Hint and response-contract queuing are recorded separately.

| Model | Corrected baseline | Candidate 1 | Final candidate |
|---|---:|---:|---:|
| Mistral Small | 5/6 | 4/6 | 6/6 |
| GPT-4.1 mini | 5/6 | 6/6 | 5/6 |
| Gemini Flash-Lite | 5/6 | 6/6 | 6/6 |

The corrected baseline's shared failure was the verification-only case: all
three providers invented `run_command`, `bash`, or another unsupported tool,
and the old consumer still queued the high-confidence suggestion. Candidate 1
selected the required action in every case, but Mistral's two object-valued
`quickest_check` fields rejected otherwise valid envelopes. These failed cases
motivated the final presentation fallback.

The final GPT verification response retained the exact command but omitted
`suggested_tool`. The parser accepted its JSON, while the new action validator
withheld both advisory messages. No missing tool was inferred from prose. This
is a remaining live failure, not a pass from successful parsing alone. Mistral
and Gemini queued both messages in all six final cases; GPT did so in five.
The three fresh final panels had no HTTP or instrumentation errors.

The initial baseline attempt is also retained: Mistral had **two HTTP 503
failures and four instrumentation errors** because the runner called `len()` on
`tool_calls: null`. Those four bodies were not saved, so their quality cannot
be reconstructed. Its raw 0/6 aggregate is not a model-quality estimate. GPT
scored 5/6; Gemini scored 4/6, including one malformed JSON response with a stray
percent character. After changing only the null-safe counter, all three full
baseline panels were rerun on the same binary and identical prompts. Neither
models, conditions nor expected actions changed. In total, all **72 attempted
Observer requests** across the initial panel, corrected baseline and two
candidates are retained, including transport and measurement failures.

## Unchanged Coder suite

The Coder spec, 33 fixture hashes, acceptance checks, required tool-use criteria
and case caps remain unchanged. Each case gets a fresh copied workspace and its
own Cargo target. Conditions remain temperature 0, maximum output 2048, request
timeout 90 seconds, outer timeout 900 seconds, and caps 14/14/10/8/12/18.
Tool choice stays `required` for Mistral/OpenAI and `auto` for Gemini.
All six suites / 36 cases are retained.

| Model | Previous final | Candidate 1 | Final candidate |
|---|---:|---:|---:|
| Mistral Small | 5/6 | 6/6 | 4/6 |
| GPT-4.1 mini | 5/6 | 5/6 | 5/6 |
| Gemini Flash-Lite | 3/6 | 5/6 | 4/6 |

Candidate 1 GPT patched the TUI spec, then hit a semantic assertion comparing a
selected message ID with a case ID. Later repairs missed edit anchors and
reached the cap without current proof or a final handoff. Gemini wandered into
nonexistent source directories; the progress gate eventually blocked the
needed replay-spec read before an edit. These are retained Coder failures,
not failed Observer-primary-action probes. Mistral's passing suite included
four provider retries; transport recovery and task success are separate facts.

Final-candidate counterexamples:

- Mistral completed and verified its TUI artifact with `write_file`; only the
  frozen required `patch_file` criterion failed. Its self-fix case repeatedly
  proposed an empty-anchor documentation patch while generic recovery required
  diagnosis, with no current verification or final handoff. Two provider
  retries occurred in this suite.
- GPT wrote a replay case without a usable assistant target. Subsequent edits
  were blocked by the recovery gate, while a repeated diagnostic read was
  blocked by the trace-derived policy requiring mutation. The cap ended without
  verification or handoff.
- Gemini created a replay case with an unusable target, reread and reran it,
  then could not move a proposed correction past diagnosis. In self-fix, Rust
  passed but the combined required check failed on the missing runtime-eval
  follow-up. Documentation work and the next input read did not complete that
  obligation before the cap; there was no final handoff.

No Coder suite timed out or reported a run error. The six recorded provider
retries are all Mistral (four in candidate 1, two in the final candidate).
The totals are not monotonic and do not establish an end-to-end improvement
from the Observer change.

All six TUI artifacts parse as JSON. Candidate 1 Gemini leaves only the original
case, so preservation does not mean the task was built. The other five have
two cases. Strict original-case preservation passes in five artifacts; final
Gemini drops the original Observer response's second suggestion. This is an
actual content difference, not optional-null serialization. It is recorded
separately without changing the frozen case score.

## Identity and scope

| Runtime | Source commit | Binary SHA-256 |
|---|---|---|
| Baseline | `a1c797964f5110a36ef567a89cfbe856c7b90fe6` | `91393e8ccc6e73c75df295c5499c0e953c78737ba7542add8309e1012c1f9cb7` |
| Candidate 1 | `6b67ba18a705e1fc599f32f997e6657c66b7c955` | `1e8457bccd5e46da14d933084f7bdef43a772d2cecf6ae32445749dff10ac6cc` |
| Final | `c91dc14ed15889410b33c77e8e9fc0b035fd0b49` | `7abab1b2df737a0171518e4c8ea0e628bda008a9a1d1c32f2a0ca00b935fbc73` |

Following commits publish documentation and receipts only. The unchanged Coder
spec SHA-256 is `2edcc57bb2291ed10d0130b9a0997e018e52b0593d0fa576e59bc064a66c5db0`.
The Observer suite SHA-256 is
`4e0c78213db3f8d73718219f05bd09bbeaa27a4dfac99212641f37f780d3fc6a`.
The receipts also bind every prompt, response artifact, runner, fixture and
local check log. Source and binaries differ between candidates; there is no
unchanged final-candidate repeat or isolated causal comparison.

**Draft qualification remains red.** These small panels establish neither
model ranking nor general reliability/nonregression. Correctly parsed advice
is not executed work, verified repair or a final report. Legacy GUI displays
still lack structured write receipts and are conservatively unconfirmed; this
turn contains no rendered GUI qualification. Source-audit follow-ups include
invalidating pending Observer advice on a new human task and reconciling old
failure observations after later success; those are not claimed as reproduced
or repaired here. Previous Coder counterexamples remain in the historical record.

[Receipts](receipts.json) and the [failure ledger](failure-modes.json) separate
runtime counterexamples, model task failures, HTTP errors and instrumentation
errors. Raw transcripts, credentials, opaque signatures and machine-local
paths are not published.
