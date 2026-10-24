# Format recovery and failing-case identity — 2026-10-25

This pass adds deterministic format recovery and retains the identity of failing
tests. It does **not** establish improved weak-model reliability: qualified
completion is **2/3 in the retained baseline, 2/3 in round 1, and 1/3 in round 2**
under the corrected, consistently applied assessor. Independent artifact checks
are **2/3 → 3/3 → 2/3**; strict all-file scope remains **0/3** in every round.

Six new episodes spent **132 requests** (61 + 71), below the predeclared 144 cap.
There were no additional compatibility probes, in-place retries, fallback models,
or third live round. All processes finished within their wall caps without a
recorded transport error; process success is distinct from task completion.

## Matched task and receipts

The [plan](plan.json) and [second-round amendment](round2-plan.json) precede their
calls. Each profile uses the **same model for Coder and Observer**, sharing a
24-request / 600-second episode cap:

- Gemini `gemini-3.5-flash-lite`
- OpenAI `gpt-4.1-mini`
- Mistral `mistral-small-2603`

All three rounds use the exact retained `pilot-cli` prompt, suite, matrix, initial
workspace inventory, frozen driver and original seven tests. Preflight confirms
that the broken fixture fails and the reference passes. Requested work repairs
case filtering while preserving defaults, order, guards and independent oracles,
updates documentation, records native failing-before/fresh-after checks, and
authors the requested closeout. Tests are evaluated again outside the candidate
workspace. The unchanged documentation proxy is `selection-paraphrases-v4`.

| Model | Retained baseline | Round 1 | Round 2 |
|---|---|---|---|
| Flash-Lite | Qualified, 19 requests | Qualified, 15 | Artifacts pass; missing final native check, 24 |
| GPT-4.1 mini | Qualified, 18 | Artifacts pass; inline-report refusal, 24 | Qualified after assessor compatibility correction, 23 |
| Mistral Small | Default-all broken, 24 | Qualified after two further repairs, 22 | Whole-file rewrite loses existing functions, 24 |

Round 1 roles: Lite 15 Coder / 0 Observer; mini 23 / 1; Small 21 / 1.
Round 2: Lite 23 / 1; mini 21 / 2; Small 23 / 1. The two mini Observer requests
include a bounded repair of a `done` suggestion with empty arguments. Missing
authored completion fields were not fabricated to forward that suggestion.

## Runtime changes

Round 1 runtime [`5af1cb3`](https://github.com/RyoSpiralArchitect/Spiral-Coder/commit/5af1cb3e65deb72d128265a3fcc4f9f594aea390)
integrates quality change `e7ad44f` and the existing provider/catalog compatibility:

1. Protocol blocks and Observer parse one complete, bounded JSON object with
   optional prose/fences. Known field aliases/types normalize deterministically;
   unknown metadata is ignored. A double-encoded Observer argument object can be
   decoded once. Duplicate keys/aliases, competing objects, truncation, invalid
   types and missing required decisions still fail. Native calls and signatures
   are not rewritten, and all semantic/action/proof gates remain in force.
2. Python diagnostics keep the failing unittest case, innermost frame and
   exception through compact output, pruning and resume. The shared repair brief
   exposes consecutive failures of the same declared check with native call IDs.
   A changed failing case does not prove that its predecessor was fixed.
3. Final-report instructions use plain text. Only an exact leading echo of the
   current report-context object can be removed before validation/display; missing
   authored prose is still missing. Original native arguments remain unchanged.

Round 2 runtime [`4918b4a`](https://github.com/RyoSpiralArchitect/Spiral-Coder/commit/4918b4a529f0fdf798dccae2d6eef2c9686031a1)
adds `a93f199`: exact authored outcome rows may follow bounded report labels, or
the explicit `Verified with <exact command> before and after edits:` sentence.
The retained round-1 mini report reproduces the old refusal. Arbitrary colon
prefixes, negative qualifiers, stale commands and missing rows still fail. No
status, command or completed criterion is supplied by this adjustment.

## What the runs show

**Small, round 1:** the first patch broke no-filter selection. The compact native
result retained `test_default_runs_every_case_and_records_selection`. Its next
patch restored that behavior but used requested order, retaining
`test_repeated_reverse_selection_runs_once_in_suite_order` on the subsequent
failure. A third patch passed, followed by documentation and fresh native proof.
The same model failed in the next round, so this trajectory is a useful retained
example, not evidence of a stable improvement or an isolated diagnostic effect.

**Mini, round 1:** seven closeout refusals followed correct artifacts and native
proof. Five reports contained both exact required rows, but the first row followed
a verification sentence ending in a colon. That formatting defect is reproduced
and covered by the second change. Round 2 accepts its report; the new colon form
was not emitted there, so the live round is not an ablation of that parser fix.

**Lite:** its round-1 JSON plan contained seven criteria. The parser preserved
them and the existing four-criterion maximum correctly rejected it; the model
subsequently authored a valid plan. The accepted final report has no report-context
JSON echo. Round 2 produces correct artifacts but reaches the cap without the
required final explicit check. A recovery gate blocks its verification attempt
after earlier protocol/edit refusals; the Observer identifies the pending check,
but the remaining request is already reserved for closeout.

**Remaining semantic failures:** Small's second round replaces the source with a
shorter program, losing `run_bounded` and `preserved`; original tests then report
seven errors and requested documentation is absent. In the first round its Observer
also proposes an early source read after native checks already pass; Coder defers
that stale advice. Structural validity alone does not choose a useful next step.

New wrapper/encoded-argument recovery is covered by deterministic tests, including
the real TUI and Web consumers. It was not observed in retained Coder output here.
Successful Observer output in these frozen runtimes was saved only after display
formatting, so its raw format cannot be reconstructed. The post-run audit fix
[`1081160`](https://github.com/RyoSpiralArchitect/Spiral-Coder/commit/1081160)
retains original `Message.raw_content` beside formatted coupled-run output, without
adding it to provider history or proof. That recording-only follow-up is tested
offline and has **no additional live-model qualification** in this pass.

## Assessor correction is separate from runtime gains

The frozen original and v4 assessments report **2/3 → 2/3 → 0/3**. Round-2 mini
omits `remaining_acceptance` while explicitly completing every plan criterion with
accepted native evidence. The runtime already treats omission as empty and checks
exhaustive completed/remaining coverage; the assessor required a literal empty
array. This was an evaluator mismatch, not an unfinished model task.

The [offline amendment](assessor-amendment.json) introduces
`accepted-omitted-remaining-v1`. Only a correlated accepted native `OK: done` can
use that default. Nonempty remaining criteria, rejected/absent done, invalid
explicit field types, missing proof and failing independent artifacts still fail.
Field presence is recorded; original arguments are unchanged. The same assessor
is snapshotted and applied to all three retained rounds, with **zero API calls**.
It changes only mini's round-2 qualification to true. Old assessments/manifests
remain byte-for-byte intact; this one qualification is **not a model gain**.

## Validation and provenance

- Final quality tree: 834 Rust tests passed, one ignored, plus one integration;
  TUI replay 7/7. The frozen round-2 integrated build passed 842 Rust tests plus
  one integration, with one ignored. The subsequent audit build is recorded
  separately in [validation.json](validation.json).
- Python benchmark suite: 38 passed, including accepted omitted-field compatibility
  and rejection of missing proof/nonempty remaining criteria.
- [comparison.json](comparison.json) records exact commits, binary/assessment
  hashes, both assessor counts, request roles and native qualification axes.
- `retained/`, `r1/`, and `r2/` contain immutable predeclarations, offline preflight,
  all three assessments and all three raw manifests. All manifest versions were
  reverified against local bytes, permissions and hashes; latest manifests cover
  360 entries across the three roots.
- [r1-observations.json](r1-observations.json) records failing-case identities and
  the exact-field/colon pattern without publishing private raw provider sessions.
- Local raw roots are `~/.cache/spiral-coder/capacity-20261024-small-r2`,
  `capacity-20261025-format-r1`, and `capacity-20261025-format-r2`.

This is an adaptive development fixture, one episode per model per round, with a
retained earlier baseline and joint interventions. It is not held out, randomized,
an estimate of stable success rates, full repository dogfooding, or release
qualification. Runtime state/cache files still fail the strict all-file scope
metric. No npm release or PR merge is part of this pass.
