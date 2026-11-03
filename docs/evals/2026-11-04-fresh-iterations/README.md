# Fresh iterations on the stock pilot and issue #49

The [earlier pilot](../2026-11-03-coupled-iterations/automatic-pilot/README.md)
ended without a report on a fresh run. Replaying its first 16 responses and
buying one new Coder response demonstrated the repair. The runs below use
new Luna responses in both roles. Each condition has its own input hashes,
budget and first terminal outcome.

## A second iteration completed the pilot without operator input

| Condition | First iteration | Second iteration | External assessment |
| --- | --- | --- | --- |
| [Fresh replication](pilot/results.json) | Reproduced one failing test out of seven. The Coder submitted one patch spanning two files. It was rejected in full. The Coder then issued an honest blocked report. | The scheduler stopped because there was no changed file or passing check. | The unchanged fixture still failed. 11 task requests and 12 wires. |
| [Fresh run on the revised runtime](pilot-after-baseline/results.json) | Reproduced the test failure, changed the Python implementation and handed the unfinished guide to the next budget. | The Coder edited the guide, ran the seven original tests successfully and submitted its own accepted report. | The original independent oracle passed the source, guide and protected files. 14 task requests and 15 wires across both roles. |

In the second run both the source edit and the failing baseline qualified
as progress. To isolate the new rule, a [local control](pilot/compare_boundary.py)
replayed the **first** failed trial, where no file had changed, on both
executors. Each executed the original twelve responses and reproduced the
unchanged workspace. Only the revised runtime opened iteration two. The
control intentionally stopped at its first Coder wire. [Wire records](pilot-after-baseline/boundary.json).

A baseline can unlock another budget once when the task explicitly requires
it, the native command ran in the task root before any edit, and its recorded
unittest or libtest counts show a test failure. Repeated failures, untyped
nonzero exits and a successful post-edit check have separate meanings.

## The original catalog task reached a fresh two-round report

[R19](catalog/plan.json) started from the original assembled source tree with
the same prompt, pair and six independent checks as R18. The first iteration
spent 32 task requests, changed two Rust files and issued a blocked report.
The second spent 27 requests. The Coder finished the native command, installed
launcher and guide, ran formatting at step 40, passed the visible check and
the Rust binary suite at steps 41 and 42, and submitted an accepted report
at step 43. All 59 responses were freshly generated.

Five of six independent groups passed, including stock regressions. Ten of
eleven held-out cases passed. The ordinary replay and catalog differed when
a filter matched nothing. The guide's literal `--filter rescue` example
also exited with status 1 from the assembled package despite showing a
successful JSON response. The source contained a separate selection loop
for the catalog. [All original gates](catalog/results.json) ·
[Packaged example](catalog/guide.json) · [Public diagnostic](catalog/diagnostic.json) ·
[Exact candidate](catalog/candidate.patch).

A separate [operator-directed task](diagnostic/plan.json) started from the
previous day's saved catalog. The pair added a CLI regression but used the
wrong flag for ordinary replay and replaced one generic error with another.
A formatting check failed. Both terminal Observer envelopes put an object
in the field for a list of evidence references. The run retained both
rejections, three unused shared requests and one unused Coder request.
[Native evidence](diagnostic/audit.json) · [Independent assessment](diagnostic/results.json).

When one Coder request remains and the task assigns it to reporting,
the runtime allows the Coder to issue a truthful blocked handoff after an
explicit Observer repair ends in a recorded envelope transport rejection.
The normal completion and approval checks are applied when it consumes that
turn. A native integration test exercised two malformed Observer envelopes,
an accepted blocked report, zero remaining Coder requests and all three
still-available Observer task allocations.

## Public acceptance on new model work

[R20's declaration](catalog-public/plan.json) adds two visible checks and an
explicit formatting check. The extra commands compare the no-match
diagnostic to ordinary replay and compare a documented positive example
to the built catalog. Both failed on R19 and passed on a disposable
reference with its measured output. The original six independent groups
and the source inventory stay frozen. Up to three 32-request iterations
are available within this single invocation. [Public controls](catalog-public/preflight-public.json).

A [separate assisted repair](diagnostic-finish/plan.json) gives the pair its
own previously authored regression, the spelling of the output switch in
public CLI help and the location of the ordinary diagnostic in public
source. Its frozen test checksum permits the switch correction and rustfmt.
The operator supplies no candidate edits.

R20 spent 96 task requests and 97 wires over three iterations. The catalog
and installed route changed, but failed formatting diagnostics in the later
rounds stalled the Coder. Its final blocked report left the guide and
checks outstanding. The independent build, visible case, npm entry and
ordinary replay passed. The held-out suite reached 9/11 and the stock
check stopped at formatting. [R20 results](catalog-public/results.json).

### The assisted candidate passed all six independent groups

The first assisted repair changed the two permitted files and reached
11/11 held-out tests. The read-only listing now returned the ordinary
replay's full error. Its local `cargo fmt --all -- --check` failed on two
files, and the runtime kept blocking the requested formatter after the
Coder read those files. The stock check stopped at formatting. The
[stopped two-iteration run](diagnostic-finish/audit.json) spent 40 task
requests and 42 wires.

The revised runtime identifies the first changed rustfmt hunk in the
workspace and recovers the diagnosis from older empty session snapshots.
In a matched local replay, the old runtime rejected the prior formatting
request. The revised runtime ran it in a disposable copy and changed only
the catalog and regression test, producing the frozen test bytes.
[Matched native action](format-resume/wire-control.json).

A freshly generated [same-task continuation](format-resume/plan.json) then
used 12 Coder and seven Observer requests. The model itself fixed each
remaining formatting difference through the ordinary edit tools.
At steps 31–34 it passed the exact formatting, regression, visible and
Rust checks. It authored the accepted report at step 35. The independent
assessor reapplied its composite onto the original baseline and passed all
six groups, including 11/11 held-out tests and the full stock regression
suite. The guide matched the installed package, both error streams
matched, and the listing left files and directories untouched. The
regression matched its frozen flag-corrected, formatted bytes.

The guide was delivered by the earlier task and independently exercised
again. The older assessor's `native_guide_checks_report` specifically
requires a fresh guide edit in the task being assessed. The current
production acceptance instead required code and test edits and four new
exact checks. [All acceptance gates](format-resume/audit.json) ·
[Original six groups](format-resume/results.json) ·
[Composite patch and code review](format-resume/candidate-review.json) ·
[Literal package example](format-resume/guide.json) ·
[Exact public diagnostic](format-resume/diagnostic.json).

The original R18 single-episode verdict stays failed. This passing
candidate includes several independently sealed model episodes and explicit
operator feedback. The operator made zero feature edits in this candidate.
The raw catalog retains two unused imports. The existing feature PR uses
its already-reviewed implementation and receives the model-authored
regression byte-for-byte as a separate integration test.
