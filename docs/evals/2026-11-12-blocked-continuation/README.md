# S14: resume work strategy after an accepted blocked closeout

PR #83 merged at `5525a8f`. S13 retained an immediate third-round blocked
report despite fresh allowance. Its final reflection said the governor required
reporting. Inspection found that loop setup restored the prior closeout's
`abandon` reflection as an imperative to execute only its report action.

The runtime now excludes only native accepted blocked-closeout assistant turns
from work-plan, last-reflection and working-memory strategy projections. Native
call/result correlation is required. The history, remaining obligations, failed
checks and approval barriers are retained. Compaction protects the preceding
work anchors instead of the report-only strategy. Budget and no-progress gates
are unchanged.

The failing-before/passing-after test covers reload, compression, preserved work
strategy and unaccepted/unmatched reports. A native HTTP-mock test executes a
diagnostic read after the closeout while the exact failed check remains pending.
An initial attempt to extend the existing Observer handoff test failed because
a terminal done intentionally changes that test's latest-action handoff trigger;
that test was restored and the distinct blocked-resume scenario has its own test.

S14 used the same Luna/Luna pair, original feature-free source baseline,
prompt and native fixture, six exact checks, 24 independent cases, 26 path
subcases and four sensitivity faults. Six rounds, 192 task / 216 wire requests
and 3,600 seconds are the unchanged caps. The PR #83 reference correction is
retained. The prospective [plan](plan.json), committed at `74a8571` before provider calls, identifies runtime `84a56ed` and the exact inputs.
No seed, operator repair, automatic adoption, or historical rerating is allowed.

## First live terminal

**Unqualified, not adopted, #82 remains open.** Five rounds used 160 task / 180
wire requests in 1,155.343 seconds (19.3 minutes). The first round reproduced
the visible baseline. Rounds 2–4 resumed with native reads and made edits.
Round 5 exhausted its allowance on rejected work proposals without a new edit
or verification, so the existing `no_new_edit_or_verification` rule stopped the
chain before the sixth-round ceiling. No budget was extended.

The independent oracle is **15/24**, below S13's 22/24. Build succeeds, but
visible behavior, actual npm behavior, guide example and boundary checks fail.
Stock checks stop at formatting; later stock suites are not credited. Required
authored tests and the state-schema update are absent, six native checks remain
pending, and the authored-test sensitivity audit is unrun. A functioning restart
is not evidence of task completion, efficiency improvement or repeatability.

The retained transcript has 18 Plan Gate rejections in round 4 and 27 in round
5. Its proposed goal adds `required` to the previously admitted goal. Although
the visible text describes the same delivery, the verbatim-goal fallback does
not accept that change. The runtime supplies the original fields, but the model
keeps paraphrasing. That is follow-up work; no alignment threshold was lowered.

[Full sealed assessment](results.json) · [Diagnostics](diagnostics.json) ·
[Exact candidate patch](candidate.patch). The patch is an experimental failure
record and is not applied to the production source.

## Post-run shared strategy correction

The shared `strategy_memory` summary independently selected the report plan.
The post-run regression uses a valid three-step reporting plan: before the fix,
both roles see report/list/end steps; afterwards they see the prior work steps
across reload and compression. The earlier one-step report fixture could not
expose this particular defect because the strategy-summary validator rejects
one-step plans. This fixture correction and first unsuccessful reproduction
are retained in the separate post-run validation receipt.

The shared summary now uses the same correlated-closeout exclusion as runtime
strategy restoration. This correction is **not in S14's frozen executor** and
has no new provider run. It does not establish that the later plan-paraphrase
loop is fixed. S14 is not rerated.
