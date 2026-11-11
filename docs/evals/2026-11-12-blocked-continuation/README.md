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

S14 will use the same Luna/Luna pair, original feature-free source baseline,
prompt and native fixture, six exact checks, 24 independent cases, 26 path
subcases and four sensitivity faults. Six rounds, 192 task / 216 wire requests
and 3,600 seconds are the unchanged caps. The PR #83 reference correction is
retained. The prospective plan will identify the exact runtime and inputs.
No seed, operator repair, automatic adoption, or historical rerating is allowed.
