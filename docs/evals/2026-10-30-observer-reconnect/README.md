# Bounded Observer stream recovery

[Declaration](plan-r9.json) · [executor](executor-r9.json) ·
[offline controls](preflight-r9.json) · [results](results-r9.json) ·
[source/report review](review-r9.json) · [validation](validation.json) ·
[raw manifest](raw-manifest-r9.json)

The [previous wave](../2026-10-29-observer-closeout/README.md) stopped settings
work when its third Observer response ended before a terminal frame. Source
was edited, but documentation and exact verification remained pending. That
original partial response is preserved and now drives a native regression test.

## Runtime behavior

One next-action assist may reconnect **once** after a native stream EOF or
response-body read failure. The fresh request repeats the unchanged native
context and any existing validation feedback, adds current budget accounting,
and excludes the interrupted response. Original partial output and its error
remain visible. Even complete-looking JSON without terminal delivery supplies
no advice or execution evidence.

The native receipt records `stream_interruption`; the reconnect records
`purpose: "observer_reconnect"` and `retry_of`. Reconnection reserves one ordinary
task request, with one wire ceiling including HTTP compatibility retries, and
requires room for Coder continuation. It never refunds the interruption or
raises a budget. Authentication, quota, provider/protocol errors and terminal
blocked diagnosis do not activate it. Context/model/endpoint/credential changes
and cancellation invalidate pending work. Repeated failure stops automatic
assistance for the same context.

TUI ticks and headless execution share this owner. The existing one action
repair can occur before or after the one reconnect; neither grants another
reconnect. All returned advice and Coder `done` still pass the existing gates.
See the [state contract](../../state-schema.md#bounded-observer-stream-reconnection).

## Frozen live observations

Coder and Observer both use **Mistral Large 4**. The task prompts, supplied tests,
independent assessor, role configuration and budgets are unchanged from r8.
One process per task was declared before either provider call; no episode was
retried. All **40 wire requests** were charged to task work.

| Task | Coder / Observer | Independent original tests | Native outcome / strict qualification |
| --- | ---: | --- | --- |
| `env-bool` | 12 / 4 | 10/10 pass | completed / qualified |
| `event-summary` | 18 / 6 | 10/10 pass | blocked / not qualified |

Settings changed its source and documentation within scope. The accepted final
report matches native counts: before 10 tests, 14 failure events and zero errors;
after 10 tests, zero failures/errors. Its first `done` attempt was rejected by
Plan Gate and the next was accepted. [Native receipts](receipts-env-bool.json)
retain both attempts and all four Observer responses.

JSONL also changed only the requested source and documentation. Original tests
pass on the retained candidate, including generators, preserved inputs, strict
lowercase levels and physical line numbers. Empty-input zero counts are stated
conditionally. However, three attempts at the required post-edit exact command
were rejected as plan drift. No native after-check ran. Its authored final
report says `After: NOT VERIFIED`, leaves three acceptance criteria remaining,
and receives `OK: blocked`. The terminal Observer gives a no-tool diagnosis.
The terminal diagnosis incorrectly describes the accepted incomplete report as
a governor rejection; the correlated native result is `OK: blocked`. Its advice
remains non-executable. Independent test success does not complete this episode.
Earlier complete-read
and evidence-block refusals are retained in its [receipts](receipts-event-summary.json).

**No live stream interrupted, so this wave did not exercise reconnection.**
Native TUI/headless mock-transport tests establish the bounded recovery behavior,
including a valid Coder final report after recovered advice. The successful live
settings task is separate evidence. The older report-only closeout path also did
not run. These are adaptive development observations, not a controlled causal
comparison or a general success rate.

## Retained next question

The [four rejected plans](plan-refusals-r9.json) preserve raw authored text,
proposed calls, the last admitted plan and exact refusals. In JSONL, revised goals
and shortened acceptance text differ from the admitted objective/criteria, so
the existing exact-obligation continuity fallback does not apply. The next
investigation is how to retain the admitted task through final verification
without weakening true-drift rejection. No plan-alignment or completion gate
changed in this PR, and the blocked attempt remains blocked. Terminal Observer
explanations must also distinguish an accepted incomplete report from a rejected
`done`; this wave preserves that misattribution for a separate diagnosis follow-up.

## Pins and verification

- Runtime source: `a1988d881cdbb31423ff66bd2aaa0dda0e7180e8`.
- Pre-call declaration commit: `37bb70e79551203715387b73f58b705656e3abdd`.
- Each episode: 24 task requests, four transport-repair credits, six reserved
  Observer requests, Coder slice cap eight, 90-second request timeout and
  600-second wall limit. Output caps: 32,768 Coder / 8,192 Observer tokens.
- Frozen binary, suite, matrix, declaration and preflight hashes are in the
  executor. All 80 raw manifest entries were verified by hash, size and mode;
  selected API credentials were absent from those files. The manifest locates
  the local raw bundle rather than embedding all of it publicly.
- [Settings patch](r9-env-bool.patch) and [JSONL patch](r9-event-summary.patch)
  reapply exactly to their frozen original fixtures. Original checks pass in
  both fresh replay directories: [settings](patch-replay-env-bool.json),
  [JSONL](patch-replay-event-summary.json). No raw workspace was modified by replay.
- `scripts/check-local.sh`: 1,009 Rust unit tests passed, one ignored; one
  integration test, 91 Python, 80 Web, five npm, 13 TUI replay and 14 repo-map
  cases passed, plus server/UI smoke. A later test-only socket-draining change
  passed its targeted regression and does not alter the frozen production runtime.

Source/report review here is by the implementation author, not a fork review.
