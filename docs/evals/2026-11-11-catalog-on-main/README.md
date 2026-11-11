# Replay catalog from the original tree on merged main

R22 started [issue #49](https://github.com/RyoSpiralArchitect/Spiral-Coder/issues/49)
on the original assembled `5535115` tree. The executor was built from
`3c27794` (#79). Both roles used `gpt-5.6-luna`. The [plan](plan.json)
retains the R21 scope, six independent checks and three 32-request iterations.
Its one task change uses the previously validated quiet no-match comparison.
The executable, inputs, original failure and passing reference were frozen
before provider work. [Preflight](preflight.json)

| Iteration | Task / wire | Outcome |
| --- | ---: | --- |
| 1 | 32 / 33 | Reproduced the missing command, implemented and connected the catalog, and left an accepted incomplete report. |
| 2 | 32 / 36 | Added the npm route and guide, passed the Rust suite and no-match comparison, and exhausted the allowance during the report handoff. |
| 3 | 19 / 19 | Corrected the guide example, diagnosed and repaired formatting, ran all five exact checks after the last mutation, and submitted an accepted Coder `done`. |

The process completed without a seed or operator steering. The last mutation
was at step 59, all five current checks passed at steps 60–64 and Coder
reported at step 65. Reapplying the [patch](candidate.patch) reconstructed the
complete candidate. All six independent groups passed, including 11/11
held-out cases, the original Rust, Python, Web, npm, TUI and smoke checks,
and ordinary replay. The literal guide example returned the same JSON through
the assembled npm package. [Frozen assessment](results.json) · [Guide measurement](guide.json)

Two of the frozen gates failed. The authored summary reported the visible
unittest result without spelling out its full command. Also, five Coder
governor blocks were charged to transport by main, whereas the original
self-dogfood ledger validator allows only native argument and Observer envelope
rejections there. The durable ledgers matched, their parent IDs linked, and
the current coupled benchmark's accounting accepted all three rounds within
their caps. That diagnostic does not replace the frozen assessor. R22 remains
unqualified under its declaration. [Budget audit](budget-audit.json)

## Repairs from the captured run

In iteration two, a 240-line launcher read was rejected and the same file was
read successfully at the corrected bounds. The old read error nevertheless
became the most recent unresolved failure after continuation. The explicit
visible check was repeatedly blocked in `Diagnose` even though the current
check belonged in `Verify`. A later correlated native read of the same
normalized path now resolves the read error in phase restoration. The
underlying verification still needs its own successful execution. The
regression exercises a real range rejection, its correction, continuation,
and an unresolved error on a different path. [Captured sequence](read-recovery.json)

The last two Observer responses in iteration two supplied one advisory object
instead of a `suggestions` array. The parser now wraps the original object for
an explicit no-tool clarification with no required acknowledgement. The native
evaluator rejects both captured replies on main and admits both on the revised
runtime. It keeps the report-only capacity and creates no tool result.
[Matched replay](observer-control.json)

## Report-only follow-up

The [next input materializer](next_inputs.py) names the original two report
commands as exact literals using the existing final-report contract. A local
control on the sealed proof rejects the original summary under that clause
and accepts it after adding the missing command, with the same evidence rows.
[Literal check](report-literal-control.json)

A separate, operator-directed [probe](report-probe-default-chunk-plan.json)
reapplied the sealed candidate, restored the session just before `done`, and
added that one clause. It kept the five current checks and all candidate
bytes. One new Luna Coder call included both command names in its summary,
but copied three long proof commands inaccurately in the evidence rows. The
done gate rejected the mapping. One Luna Observer call followed, and the
probe exhausted its allowance without completion. The
[initial CLI setup](report-probe-plan.json) used a chunk size below the
supported explicit minimum and stopped before a provider request.
[Probe results](report-probe-results.json)

The revised runtime passed the full local check script, including 18/18
TUI replays. The original-tree live result and the saved-state report probe
use different runtime revisions. [Validation and usage](runtime-checks.json)
