# A whole-task route for the Observer

The Observer already received native obligations, a plan and recent attempts.
Those surfaces were distributed across the prompt. Its `why_this_first` and
`fallback` fields were displayed but omitted from the Coder's soft hint.

The runtime at `5389629` derives a compact route across declared artifacts,
their linked requirements, dedicated test cases and shared behavior rules.
It shows which checks another edit would invalidate and the requests available
before a full check sweep and one report. Missing reproduction work remains a
separate prerequisite. These counts are optimistic lower bounds; reads,
diagnosis, retries and semantic coverage gaps can require more work.

All Observer intervention modes receive the route. The prompt asks for an
achievable milestone, a feasible alternative and the observation that would
change the next action. Existing diagnosis and report-only rules keep their
precedence. The two optional strategy fields now accompany a validated,
confident primary suggestion, each bounded to 700 characters and marked when
truncated. They remain advice. They create no tool call, permission or proof.

The current system hint is retained in native session saves and through wire
compression. The next continuation refreshes system context; historical
Observer replies remain in the run's display record.

## Matched native replay

The replay starts at the same saved S28 closeout, with the same current authored
test bytes. Both binaries use the native Coder/Observer loop against a local
scripted provider. The control is the integrated parent `aa7c2e7`; the treatment
is `5389629`.

| Observation | Control | Whole-task route |
| --- | ---: | ---: |
| Current authored test source visible to both roles | Yes | Yes |
| Whole-task route and edit/verification cost visible to Observer | No | Yes |
| Scripted fallback reaches the Coder | No | Yes |
| Workspace changed | No | No |
| Pending checks changed | No | No |
| First Observer message bytes in the replay projection | 65,658 | 74,177 |

The larger prompt is a measured cost, not a capability result. The replay
establishes delivery and unchanged native proof. It makes no external provider
request and does not qualify #82. [Replay receipt](prompt-replay.json)

## Frozen live condition

The [plan](plan.json) was committed and pushed at `636786d` before provider
requests. It retains the original #82 baseline, prompt, inputs, driver and
qualification gates. Luna is both Coder and Observer, with six iterations of
32 task requests, eight reserved Observer requests and four transport repairs
per iteration. Global caps remain 192 task requests, 216 wire requests and
3,600 seconds.

The reference passes and detects all four advertised faults. The retained S12
tests fail on the reference, so their mutant test runs are skipped.
[Sensitivity controls](sensitivity-controls.json)

This run also incorporates main's coverage planning and report-continuation
changes and #104's earlier diagnostic handoffs. Comparing it with an older
live run cannot isolate the causal effect of the new route.

## First terminal and independent result

The fresh run completed in iteration 5 with 152 task / 166 wire requests in
1,580.528 seconds. Native `done` was accepted with all six exact checks current
after the final edit, and the before/after rows on their own lines.

**25/26 gates pass; #82 is unqualified and the patch is not adopted.** The
independent groups pass 6/7, with 23/24 external behavioral cases. The authored
tests pass on the reference and detect all four frozen faults. Their three
test functions cover missing pending data, symlinks, duplicate request IDs
and parent linkage. [Sealed assessment](results.json)

The external `test_two_rounds` case fails with `duplicate request id`. Two
valid iterations reuse request IDs under different ledger owners; the
candidate carries one uniqueness set across them and rejects valid input.
This exposes a limit of negative fault sensitivity: rejection tests alone do
not establish that independent valid units still compose.

The first native test edit was step 66 in iteration 3. The preceding
readback-fresh run first edited that file at step 102 in iteration 4. The
new tests change an existing ID without changing the request count and replace
the recorded directory with a link to its saved valid target. These are
observed choices, not an isolated causal estimate. The native record also
contains 16 governor rejections across the retained rounds.
[Native timing and retained advice](route-observations.json) ·
[Unadopted candidate patch](candidate.patch)

## Ownership and composition follow-up

The prompt at `5b859ea` adds an advisory question about the unit that owns and resets
an identity or counter, grounded in the declared contract and observed producer
or schema. It asks for a relevant positive composition control as well as a
negative rejection case. It changes no acceptance gate and names no benchmark
fixture or required implementation.

The limited follow-up starts from the sealed candidate just before its final
report, with the original task and native checks. The operator makes no
candidate source or test edit and supplies no hidden-check feedback in the
seed. This is a saved-point diagnostic continuation, not a fresh qualification
run. Its [separate plan](boundary-plan.json) pins the request and time caps
before new provider calls.

The first terminal was `report_incomplete`: 32 task / 36 wire requests in
337.134 seconds, including eight Observer requests and four transport repairs.
The Coder chose missing-component and dangling-symlink test expansion before
the first funded Observer assist; the Observer continued that route. No
positive multi-owner composition case was added. Only the authored test file
changed. [Continuation receipt](boundary-results.json) ·
[Unadopted test delta](boundary-candidate.patch)

The added tests initially failed to compile. After a repair, both new tests
panicked during setup: they tried to remove a pending ledger that the `unknown`
fixture had never created. Creating the parent directories did not supply that
file. The independent rerun passed 5/7 groups and 23/24 behavioral cases; the
original `test_two_rounds` failure remained. The tests also failed on the valid
reference (three passed, two failed), so the four mutant test runs were skipped.
This is an invalid sensitivity result, not four demonstrated missed faults.

No exact final check was current after the last edit. The final blocked report
cited a guide check without a current success receipt and was rejected. The
process exited normally, but the task did not complete. Neither candidate is
adopted, and the fresh result remains 25/26 rather than being replaced by this
continuation.

This follow-up does **not** establish an improvement from the ownership lens.
The lens was developed after examining the external failure, and the saved
point already contained native closeout obligations. There is no matched live
control that isolates why the continuation took this route. The remaining
engineering questions are how to separate product uncertainty from broken test
setup, ground a diagnosis in the actual expression and diagnostic, and choose
a small informative control before spending the final budget on test expansion.

## Runtime checks

The full local check script passed: 1,241 Rust unit tests with two ignored,
six integration tests, all 20 TUI replay cases, 97 Python tests, 80 web tests,
five npm tests, the repository-map checks and the local server smoke check.
An initial check caught two tests whose decision-brief parser expects that
brief first in the prompt; its existing position was restored. The final
full check passed. The complete local suite also passed for the later ownership
prompt at `5b859ea`. A test/doc correction describes the actual system
hint lifecycle and its focused compression/proof test also passed.
