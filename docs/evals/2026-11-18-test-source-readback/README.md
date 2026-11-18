# Authored test source during closeout

In [S28](../2026-11-17-s28-merged-main/), the closeout review had three
test function names after formatting. The completed candidate passed 24/24
held-out cases. Its authored tests caught three of the four seeded faults
and missed the symlink fault.

The runtime can now supply the full current source of one declared test
file after a native edit when the history lacks a complete revision.
The workspace readback has an 8 KiB limit and carries the selecting edit's
call ID. Both roles use it for review. Native receipts govern edits and
verification.

## Local replay

Both binaries replayed the conversation through S28's `done` rejection at
native tool step 96 with identical scripted responses. The test file was
restored from the next complete native read. That read followed only
read-only actions. Each runtime sent five Coder and three Observer requests
to the local stub. The workspace inventory and pending exact checks matched.

| First request | Main dedicated source copies | Readback dedicated source copies | Added content bytes |
| --- | ---: | ---: | ---: |
| Coder | 0 | 1 | 2,895 |
| Observer | 0 | 1 | 2,946 |

Each dedicated copy matched all 2,531 bytes of the current test fixture.
Main's Coder request included older test context through ordinary history.
The replay measures prompt delivery with zero provider calls.
[Replay receipt](prompt-replay.json)

## Fresh run

The [plan](plan.json) fixes the original #82 baseline, prompt, spec, budgets,
and all 26 gates for a fresh Luna/Luna run. The original missing-command
checks fail as declared. All seven reference groups pass. The positive
control detects all four seeded faults. The negative control fails one of
four tests against the reference, so its fault runs are skipped.
[Control summary](sensitivity-controls.json)

## First terminal

**Unqualified.** The run reached the six-iteration limit at 192 task and 209
wire requests in 2,139.271 seconds. It passed 18 of 26 gates. The frozen
candidate edits cover all six required paths. Five of the seven independent
groups pass, with 23 of 24 held-out cases.

The final round successfully ran the first five exact checks at steps
146–150. The path check failed at step 151, and the source changed at step
154. The repeated path check failed at step 155. All six exact checks
remained pending at the end, and the runtime accepted an incomplete report
at step 157.

The candidate's two authored tests passed on the reference. They detected
the missing-pending fault in the four-fault audit. The symlink, duplicate
request, and parent linkage faults survived. The independent symlink and
duplicate-ID rejection checks passed against the candidate implementation.

## Remaining path failure

A disposable rebuild of the sealed patch matched the frozen candidate
inventory. A field-by-field comparison found the same difference for all
three missing components of a running iteration. The candidate emits
`pending: null`. The fixture expects the recorded `directory` and null
values for `task_id`, the two request counts, and `in_flight_requests`.
The interrupted controls match for all three missing components.

[First terminal receipt and comparison](results.json)
