# Small-model repair and authored reporting, 2026-10-24

Two adaptive rounds added **six episodes and 132 task requests**. On the retained
`pilot-cli` development task, qualified completion changes from **0/3 in the
retained baseline**, to **0/3 in round 1**, to **2/3 in round 2**. Flash-Lite and
GPT mini finish; Mistral Small still breaks default selection. Strict all-file
qualification remains **0/3 in every round** because runtime state and cache
files also change.

These are single episodes per model and round, with joint interventions selected
after inspecting earlier failures. They are not a causal ablation, held-out
generalization result, stable success rate, or model ranking. The larger models'
[previous successful traces](../2026-10-24-action-closeout/README.md) motivated
minimal source repair and explicit evidence mapping; they were not rerun here.

## Fixed inputs and limits

Coder and Observer remain one unit, using the same selected model in both roles:
`gemini-3.5-flash-lite`, `gpt-4.1-mini`, and `mistral-small-2603`.

Each episode uses 24 shared requests, temperature zero, a 32,768-token output
ceiling, 90-second request timeout and 600-second process wall limit. There is no
fallback model or retry in place. Original prompts, source/test/documentation
fixtures, initial workspace inventories, matrix, and frozen run driver match
the retained six-model experiment exactly. Provider usage not returned by the
API stays unknown; equal request limits do not imply equal compute or cost.

The task repairs repeated `--case` selection while preserving suite order,
duplicates, unknown-ID rejection, default-all behavior and the no-overwrite
guard. It also requires documentation, a native failing baseline before editing,
a separate exact check after all edits and an authored before/after report.
The independent seven-test oracle runs outside the candidate workspace.

The [initial plan](plan.json) predates all new calls and caps this pass at two
rounds / 144 requests. The [second-round declaration](round2-plan.json) predates
its calls and records the observed failures motivating the additional changes.
Both rounds finished within the process wall limit. Round 1's Mistral episode
retains a real streamed-response timeout after 23 shared requests; a process exit
of zero does not erase that model-request failure.

## Runtime and assessment lineage

| Round | Integrated runtime | Runtime changes | Requests |
| --- | --- | --- | ---: |
| Retained baseline | `c1f3add6f6849f21b661a1a6c8c69919ffd53512` | Previous action/closeout support | 72 for these three profiles |
| Round 1 | `deb4e5b93275d3a95bdfc70a81c2dcfb99a1dcc4` | `6dad6a2` | 71 |
| Round 2 | `1995cdd22978c4f4e516965590e8ee0a729fe142` | Also `bde36ce` | 61 |

These integrated commits retain #34's provider compatibility and are pushed on
`codex/capacity-runtime-20261024`. Runtime changes belong to #31; benchmark
receipts and this readout belong to #35. The source binary SHA-256 and exact
inputs are recorded in each round's predeclaration.

Each new round preserves assessment 001 using its original frozen
`shell-whitespace-v2` assessor and assessment 002 using the existing
`selection-paraphrases-v4` assessor. The retained baseline uses its earlier
assessment 003 with that same revision 4. No oracle or grader was changed during
this pass. The two assessor versions agree on qualification for both new rounds.
The documentation check remains a bounded lexical proxy, not a semantic judge.

## What changed

Round 1 binds each native `done` schema to the current plan's stable criterion
references and current citable commands. Google receives flattened complete
evidence-row choices; other providers receive reference/command choices. The
model still chooses its completed/remaining criteria and evidence mapping. No
report is filled in, no returned call is rewritten, and no completion gate is
relaxed. Empty or oversized vocabularies retain generic schema fields.

Both roles also receive a compact repair brief with human-declared source paths,
native read bounds/freshness and the latest successful edit identity. It advises
tracing an assertion to existing source, preserving working defaults and guards,
and avoiding invented dependencies or copied test-local bookkeeping. It does
not select the patch, certify correctness, or authorize changes to supplied tests.

Round 1 did not improve aggregate qualification. Mini repaired the artifacts and
mapped its criteria correctly, but seven reports omitted the exact before/after
rows from `summary`. Lite emitted a JSON plan that the old text parser treated
as missing fields, then drifted into local one-step plans without recording its
baseline. Small continued guessing edit anchors and later timed out.

Round 2 additionally:

- Places current report clauses and native outcome rows directly in the
  `done.summary` field description. Evidence fields do not replace report text.
  Missing or invalidated proof remains `NOT RECORDED` / `NOT VERIFIED`.
- Reads an authored JSON object inside `<plan>` without inventing missing fields
  or discarding criteria. Duplicate fields, wrong types and unknown fields are
  rejected; existing count and task-alignment validation still runs.
- Allows the first exact human-declared baseline through protocol-only recovery,
  before any mutation or previous native attempt and with no pending localized
  failure. Normal execution approval, diagnosis and proof gates still apply.

## Results

Cells show **independent artifacts / behavior + native proof + authored report**,
followed by shared requests. Every failure remains in the denominator.

| Same model in both roles | Retained baseline | Round 1 | Round 2 |
| --- | --- | --- | --- |
| Gemini 3.5 Flash-Lite | pass / fail · 24 | fail / fail · 24 | **pass / pass · 19** |
| GPT-4.1 mini | fail / fail · 24 | pass / fail · 24 | **pass / pass · 18** |
| Mistral Small 2603 | fail / fail · 24 | fail / fail · 23, stream timeout | fail / fail · 24 |

In round 2, Lite and mini use 19 and 18 Coder requests respectively, with no
Observer invocation needed. Small uses 23 Coder requests and one Observer
request. This measures the coupled system with a shared cap, not a Coder-only
ablation. Mini's first final report is rejected, then it authors a correct report
on the next request without rerunning verification. All supplied tests and
configuration files remain unchanged in both new rounds.

Two limitations remain visible in the successful reports and failed artifacts:

- Lite copies the report-context JSON into the start of its summary before the
  required human-readable rows. The bounded completion contract passes, but
  report presentation still needs work.
- Small filters by the requested IDs but omits the empty-request fallback, so
  the default invocation selects no cases. Its source remains narrowly changed,
  but documentation and fresh successful verification are missing. The compact
  error digest retains the shared `assert_selection` helper and the list
  mismatch while dropping the failing unittest case heading. Retaining that
  case identity and comparing baseline versus post-edit failures is a next
  experiment candidate, not an established cause or a measured fix.

## Validation and receipts

Final quality runtime: **825 Rust tests and one integration test passed**;
one Rust test is ignored. The integrated catalog/runtime build passes **834 Rust
tests and one integration**, with one ignored. TUI replay passes **7/7** and
Python tests pass **37/37**. See [validation hashes](validation.json).

Focused tests cover both evidence schema shapes, exact command retention,
current/unverified report rows without generated summaries, native HTTP tool
restriction, JSON-plan parsing/resume and rejection boundaries, and first-baseline
execution following a protocol refusal. The repair brief is tested through native
resume in Coder, TUI Observer and Web projection. TUI replay's new case is an
advice-handoff smoke test; its display-only fixture is not evidence that a native
repair brief was projected. Deterministic tests do not establish live gains.

[Comparison data](comparison.json) records exact runtime and assessment hashes,
request roles, native proof, rejections and errors. The retained baseline and
both final new-round raw manifests were rehashed locally. Local raw sessions,
traces, commands, stderr and candidate workspaces remain under
`~/.cache/spiral-coder/capacity-20261024-small-r{1,2}`. Public artifacts include
predeclarations, offline preflight results, both assessments and their manifests;
provider secrets and opaque provider signatures are excluded.
