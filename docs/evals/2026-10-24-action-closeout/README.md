# Six-model action and closeout experiment, 2026-10-24

This matched development experiment keeps Coder and Observer as one unit. The
selected model is identical in both roles. It tests whether retaining rejected
native proposals, exposing the next-action contract and reserving closeout
capacity reduce observed failures. It is a joint intervention with one episode
per model per round, not an ablation, stable success-rate estimate or general
capacity ranking.

## Fixed panel and task

| Provider | Retained weak profile | Selected counterpart |
| --- | --- | --- |
| Gemini | `gemini-3.5-flash-lite` | `gemini-4-argon` |
| OpenAI | `gpt-4.1-mini` | `gpt-5.6-luna` |
| Mistral | `mistral-small-2603` | `mistral-large-4` |

Each episode repairs the same `pilot-cli` repository-derived regression, updates
its documentation, records a native failing baseline, runs the exact check after
all edits and authors the requested before/after report. It has 24 shared wire
requests, a 32,768-token output ceiling, 90-second request timeout and 600-second
wall cap. The full three-case suite remains available for dogfooding and larger
panels; this experiment runs only the CLI case.

Luna uses **reasoning none** with `max_completion_tokens` and omitted temperature
because the current Chat Completions function-tool endpoint rejected default
reasoning. Reasoning-enabled Luna needs Responses transport and is not measured
here. Large 4 retains default reasoning with temperature zero and `top_p: 1`.
Other profiles keep temperature zero and provider defaults. Equal limits do not
mean equal compute, latency or cost. Missing returned usage stays unknown.

Four separate single-request [protocol probes](https://github.com/RyoSpiralArchitect/Spiral-Coder/blob/658e4f5/docs/evals/2026-10-24-model-compatibility/README.md)
retain two initial HTTP 400 responses and two successful tool responses after
parameter correction. No tools executed during those probes. They are separate
from the task results and do not establish coding ability.

## Intervention and validation

The [initial plan](capacity-plan-20261024.json) predates the task calls. R1 uses
runtime `26de51daeaab9c441a9d4817f553b8083dc54810`: PR #31 at `625e4bc` plus
PR #34's provider compatibility at `fbfc4a6`. R2 uses
`c1f3add6f6849f21b661a1a6c8c69919ffd53512`, adding runtime change `fd3400f`.
Both integrated source commits are retained on `codex/capacity-runtime-20261024`.
Task inputs, initial file inventories, prompt hashes, model settings, driver and
request caps match exactly. Each round uses a fresh workspace; no episode was
retried or silently given another model.

The runtime change:

- Retains each rejected multi-tool proposal with the original IDs, argument bytes
  and provider signatures, plus one correlated refusal per call. It executes none
  of the rejected batch. The next request and resume can see what was refused.
- Projects a structured one-native-action contract. For explicitly declared
  task requirements, it reserves the final shared request for native `done`, or
  selects report-only mode within the final three requests once declared native
  proof is current. Unsupported actions and text-to-tool fallbacks are refused.
- Keeps the existing acceptance, baseline, fresh-proof and authored-report gates.
  Restricting tools cannot create successful proof or mark an unfinished task done.

An [offline amendment](capacity-plan-20261024-amendment.json) limits hard report
reservation to explicit task contracts. Eight existing mocked recovery tests
showed that applying it to undeclared one-request interactive repairs blocked the
repair itself. Those tests are unchanged and pass with the scoped policy. The CLI
benchmark already has an explicit task contract, so this scope clarification
changes neither its prompt nor its budget.

Runtime validation: **816 Rust tests, one integration, TUI replay 6/6** passed
(one Rust test ignored). The integrated catalog + runtime build passed **825 Rust
tests and one integration**, with one ignored. Benchmark Python tests passed
**37/37**. Captured HTTP tests cover native rejected-batch history, signature
persistence across save/resume, last-request tool restriction and refusal of
text-only synthetic edits. These deterministic checks do not establish live gains.

## Offline documentation proxy correction

Both frozen runtime rounds use the same original `shell-whitespace-v2` assessor.
R1 initially scores Large 4's documentation false because it says "every case"
instead of "all". The behavior, native proof and authored report passed. Offline
`case-quantifier-v3` accepts the equivalent case quantifiers and rejects incidental
"all" substrings such as "called". The tests retain negative cases. This remains a
bounded lexical proxy, not a general semantic documentation judge.

R2 exposed a second lexical miss: Luna says "at most once, even if an ID is
repeated", without the literal "duplicate". `selection-paraphrases-v4` additionally
accepts that bounded phrase and rejects "twice" or a merely mentioned ID. The
workspaces, exact tests and native proof requirements stay unchanged.

Each round retains original assessment 001, quantifier-only reassessment 002 and
final reassessment 003, with separate assessor snapshots and hashes. R1 changes
from 2/6 to 3/6 qualified at revision 3; R2 changes from 2/6 to 3/6 at revision 4.
Those changes are **assessor corrections**, not model or runtime improvements.
The following results apply revision 4 to both rounds.


## Matched results

Each cell is **independent artifacts / behavior + native proof + authored report**,
followed by requests used. All episodes finished within the declared wall limit.

| Same model in both roles | Before | After |
| --- | --- | --- |
| Gemini 3.5 Flash-Lite | fail / fail · 24 | pass / fail · 24 |
| Gemini 4 Argon | fail / fail · 24 | pass / pass · 18 |
| GPT-4.1 mini | pass / pass · 19 | fail / fail · 24 |
| GPT-5.6 Luna, reasoning none | fail / fail · 24 | pass / pass · 12 |
| Mistral Small 2603 | pass / pass · 23 | fail / fail · 24 |
| Mistral Large 4 | pass / pass · 19 | pass / pass · 12 |

Independent artifacts change from **3/6 to 4/6**; qualified closeout stays **3/6**.
Strict all-file qualification stays **0/6** in both rounds because generated
runtime state and bytecode remain in full-scope accounting. Total task spend is
**247 requests** (133 + 114), plus **4 protocol requests**. There are no hidden
retries, missing episodes or substituted models. All rounds and failures remain
in the denominator. The [comparison data](comparison.json) retains the role
counts, native obligations, gates, usage coverage and assessment hashes.

Argon changes from 23 single-tool rejections and no executed native action to no
single-tool rejections and an accepted authored closeout. It inspects the source,
records the expected baseline failure, edits source and documentation and passes
the exact fresh check. It still needs three retries of an empty done summary
before its accepted report. The new contract exposes a working route, but this
single joint intervention cannot isolate the effect of the prompt from retained
rejection history or establish general coding capacity.

Luna changes from 10 single-tool and 8 reflection rejections with no edit to one
single-tool rejection and qualified completion in 12 calls. Large 4 changes from
four single-tool rejections and 19 calls to one rejection and 12 calls. Both had
real native proof and reports; their documentation-proxy false negatives are
corrected separately above. These are exact-ID, current-transport results.

Flash-Lite reaches passing source and documentation and current native proof. It
also writes the requested before/after report, but cites one evidence row for two
completed criteria, so the acceptance mapping is rejected. Its final request
still does not return an acceptable native done call. Merely offering only done
is insufficient to guarantee a valid report. No report or evidence reassignment
is synthesized to turn this into success.

GPT-4.1 mini and Mistral Small regress in this pair. Mini expands the source repair
and ends with a nonexistent `coupled_pilot` import; the original tests expose four
errors. Small removes the no-filter default and tries to reproduce the test's
`invoked` bookkeeping in production code; the default-case test still fails.
These were provider-authored tool edits, not runtime-synthesized actions. The
failure does not disappear when closeout is reserved. With one trial per model,
these are counterexamples to a blanket improvement claim, not stable rankings or
proof that the intervention caused the regressions.

## What the weak models still need

| Observed failure | Next bounded intervention | Required signal |
| --- | --- | --- |
| Flash-Lite has valid artifacts and proof but an invalid criterion/evidence mapping; final done-only exposure still produces no valid call | Derive report fields and criterion/command choices from the shared ledger in the tool schema; require the model to author its own outcome decisions and summary | Accepted authored closeout with no invented proof or extra requests |
| Mini invents a dependency; Small repairs test bookkeeping instead of the production default | Keep the failing assertion, observed source branch and minimal edit target together for both Coder and Observer; make branch coverage visible before broad repair | The unchanged original tests pass, including no-filter/unknown-ID behavior, while frozen tests remain untouched |
| Strong-model routes improve while weak-model success is unstable | Repeat a predeclared panel and add unseen tasks before promotion; isolate history, action-contract and closeout effects in separate ablations | Stable improvement under the same total budgets, not only one favorable episode |
| Runtime state and test bytecode violate strict workspace scope | Give generated state an explicit location/ownership contract | Full-scope checks improve without dropping inconvenient paths |

The selected three weak profiles remain in the reusable matrix. Coder and
Observer continue to share task requirements, native evidence and request budget.
A report format repair, successful API request or Observer handoff alone is not
coding-task completion. Responses support is still needed to measure Luna with
reasoning enabled; it is separate from the compatible-mode result above.

## Retained evidence

- R1: [declaration](r1/predeclared.json), [preflight](r1/preflight.json),
  [original assessment](r1/assessment-001.json), [quantifier correction](r1/assessment-002.json),
  [matched final assessment](r1/assessment-003.json), [raw manifest](r1/raw-manifest-003.json).
- R2: [declaration](r2/predeclared.json), [preflight](r2/preflight.json),
  [original assessment](r2/assessment-001.json), [quantifier correction](r2/assessment-002.json),
  [matched final assessment](r2/assessment-003.json), [raw manifest](r2/raw-manifest-003.json).

Full sessions, traces, frozen binaries, driver/assessor snapshots and candidate
workspaces remain at the `local_root` paths in the manifests. Public receipts
omit authorization headers and opaque provider continuation data. File hashes
are audit locators; the retained outcomes above remain necessary to interpret them.
The earlier 2026-10-23 rounds and their assessments are unchanged.
