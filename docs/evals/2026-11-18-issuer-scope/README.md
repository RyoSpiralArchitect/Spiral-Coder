# Following request IDs back to the writer

The native writer creates each WorkState with an empty request vector. It assigns
the next request ID from that vector's length, then pushes the receipt into
the same vector. Two independently initialized iteration ledgers can therefore
both contain IDs 0, 1 and 2. Within either ledger a repeated ID is invalid.
The two live runs used gpt-5.6-luna for both Coder and Observer.

| Start | Native outcome | Independent CLI cases | Independent groups | Authored tests |
| --- | --- | --- | --- | --- |
| Original #82 baseline, original prompt | Stopped after 4 iterations, 128 task / 143 wire requests, 983.63 s | 16/24 | 3/7 | No test file |
| Previous 23/24 candidate, focused public task, empty conversation | Completed in 2 iterations, 51 task / 59 wire requests, 522.52 s | 24/24 | 7/7 | 6/6 on the reference, all 4 frozen faults detected |

The fresh trial passed 17/26 qualification gates. The [independent review and issue closeout](review-closeout.md) records the subsequent disposition.
The focused result establishes a repair and a regression test on the saved candidate.

## What reached the Coder

A matched native replay starts with the same archived messages and workspace
for both runtimes. An earlier read had stopped at line 200 of the 641-line writer.
With the new runtime the Observer request included the empty vector at line 141,
the assignment at 356 and the push at 357. The replay used scripted responses
and zero external provider calls. [Replay](prompt-replay.json)

In the fresh live trial the Observer cited the allocation expression in a
primary reason delivered in iteration 3. It asked for two valid finalized
owners and rejection of a duplicate within one owner. The Coder was still
working on the visible status check and never authored the test. Its parser
accepted both the valid two-ledger input and the invalid duplicate. The last
iteration made no new edit or verification. [Native advice](fresh-advice.json) ·
[Fresh result](results.json) · [Public CLI measurements](fresh-public-scope.json) ·
[Predeclared plan](plan.json)

In the focused live trial the Observer identified the writer, requested
evidence of its owning state's lifetime, then gave the Coder a public test
recipe. The Coder copied the native ledger, retained its numeric IDs and
linked a second distinct task. Its first fixture marked both iterations
completed. The CLI rejected that input. In the second iteration the Coder
marked the first row blocked and the last row completed and passed all six
exact checks after its final edit. The implementation change moved the
request-ID set inside the finalized-ledger loop. [Native advice](focused-advice.json) ·
[Task](focused-task.txt) · [Predeclared plan](focused-plan.json) ·
[Terminal and independent results](focused-results.json)

## Public regression control

The control starts with a native single-iteration fixture, extends it to two
finalized iterations whose ledgers both contain IDs 0, 1 and 2, then changes
only the second ID in the second ledger to 0. The valid output contains two
rows, four task requests and six wire requests. The invalid input requires
nonzero exit, empty stdout and a diagnostic on stderr. The reference passed
all three checks. The previous candidate rejected the valid pair with
`duplicate request id`. [Public control](check_public_scope.py) ·
[Calibration](public-controls.json)

The focused Coder edited the validator before running its new test. A separate
native test run placed the final authored tests on the exact preceding
implementation. The five earlier tests passed. Only
`owner_local_request_ids_are_valid_across_two_iterations` failed, with
`duplicate request id`. All six passed on the repaired implementation and the
independent reference. [Test results and binary digests](focused-results.json)

## Replaying the candidates

The full focused patch applies to the frozen #82 baseline. A separate delta
applies to the preceding sealed 23/24 workspace. Both were applied to disposable
copies and exactly reproduced all 1,669 files of the focused candidate inventory.
The fresh trial's patch also reproduces its own terminal inventory. The
harness PR archives these candidates for review. [Focused patch from baseline](focused-from-baseline.patch) ·
[Focused delta](focused-delta.patch) · [Fresh trial patch](fresh-candidate.patch) ·
[Inventory checks](patch-replay.json)
