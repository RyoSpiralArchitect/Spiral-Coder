# Fresh catalog repair with context compression

Both `gpt-5.6-luna` roles repaired [issue #49](https://github.com/RyoSpiralArchitect/Spiral-Coder/issues/49)
from the original tree and reached accepted completion in two iterations.
The [plan](plan.json) pins the R23 prompt, 1,027-file assembled baseline,
214 protected paths, spec, driver, role settings and budgets. The episode starts
with a fresh session. The [preflight](preflight.json) retains the expected
baseline failures and the passing reference.

| Run | Iterations | Task requests / wires | Process wall | Native outcome | Independent checks |
| --- | ---: | ---: | ---: | --- | ---: |
| [R23](../../2026-11-11-catalog-efficiency/README.md) | 3 | 96 / 106 | 1,394.238 s | Iteration limit | 6/6 |
| R24 with compression | 2 | 62 / 70 | 636.373 s | Accepted `done` | 6/6 |

R24 used 34 fewer task requests, 36 fewer wires and 757.865 fewer seconds.
Recorded provider intervals totaled 432.040 seconds, compared with 766.366 in
R23. The comparison describes two sampled trajectories, including differences
in edits, build work and provider latency. [Timing profile](profile.json)

## Repair and qualification

- Iteration 1 reproduced the visible failure, edited the two Rust entry
  points and submitted a blocked report. It used 32 task requests and 36 wires.
- Iteration 2 added the catalog, npm route and guide. The guide check first
  raised an unlabelled `AssertionError` on the missing example. A guide patch
  missed its search anchor. After two recovery refusals, a bounded read at the
  end of the guide supplied the anchor for the successful patch.
- Formatting was the final mutation at step 46. Formatting, visible unittest,
  Rust binary tests, no-match diagnostics and the guide check passed at steps
  47–51. The report at step 52 hit a plan-format gate. Step 53
  supplied a valid plan and an accepted report with all four criteria complete,
  both required literal commands, 1 visible test and 1,030 passing Rust tests
  with 2 ignored. All five exact checks were current. [Native results](results.json)

The sealed patch reapplies to the baseline and changes all five required
paths. All 214 protected paths and the original guide prefix are preserved.
Independent verification passed all six groups, including all 11 held-out
cases and the stock regressions. The assembled npm package produced the same
two catalog cases as the guide's literal example.
[Candidate patch](candidate.patch) · [Packaged guide receipt](guide.json)

The frozen assessor records **one failed gate**, `distinct_parent_linked_ledgers`.
Its transport eligibility rule rejects the eight Coder `governor_block` charges.
Both rounds' durable ledgers, parent links, role settings and caps match the
current coupled benchmark policy. Each transport charge correlates with its
native trace event. The frozen qualification verdict remains false.
[Frozen assessment](results.json) · [Current ledger audit](context.json)

## Context across iterations

| Iteration | Coder assemblies reduced | Coder message bytes before | After | Reduction |
| --- | ---: | ---: | ---: | ---: |
| 1 | 18/28 | 3,032,333 | 2,702,565 | 10.9% |
| 2 | 27/27 | 5,514,603 | 3,304,072 | 40.1% |
| Total | 45/55 | 8,546,936 | 6,006,637 | 29.7% |

These byte counts compare each Coder assembly before and after history projection,
excluding tool schemas. After projection, 54 of 55 assemblies exceeded the 64 KiB
soft target. The largest projected view was 138,135 bytes. All 26 tool-call
messages and 26 result messages from the first saved session survived in the
second with exact JSON values, including local metadata. Native session size
grew from 169,477 to 300,452 bytes. [Context measurements](context.json)

| Role | R23 median estimated input per wire | R24 median | R24 completed replies |
| --- | ---: | ---: | ---: |
| Coder | 36,319.5 | 31,539 | 55 |
| Observer | 20,058.5 | 14,923 | 15 |

The native estimate uses serialized payload characters divided by four and
rounded up. Provider usage receipt count was zero. The live role medians include
the different actions and iteration counts. The [matched Observer captures](../README.md#matched-model-free-controls)
hold the source packet fixed for the byte comparison.

Run `python3 -BS measure.py <prepared-output>` to recompute context, request and
retention measurements from the finalized native rounds. Raw traces, ledgers,
the sealed receipt and check logs are retained under
`~/.cache/spiral-coder/context-compression-artifacts-20261111/r24/`.
