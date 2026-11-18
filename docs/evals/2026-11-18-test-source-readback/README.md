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
