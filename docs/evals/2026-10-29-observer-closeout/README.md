# Observer completion evidence and bounded final reporting

[Declaration](plan-r8.json) · [executor](executor-r8.json) ·
[offline controls](preflight-r8.json) · [results](results-r8.json) ·
[supplementary review](review-r8.json) · [validation](validation.json) ·
[raw manifest](raw-manifest-r8.json)

This continues the [plan/report work](../2026-10-29-plan-report/README.md).
The earlier r6 settings attempt proposed four completed acceptance criteria
with only two evidence rows. Its one Observer repair exhausted the output limit
without returning content. That incomplete attempt remains unchanged.

## Runtime changes

- Coder and Observer now receive the same exact acceptance references, current
  command choices and authored-report guidance. The TUI and browser endpoint
  derive these from native history. Neither returned evidence mappings nor
  missing values are filled by the runtime. The original r6 proposal is retained
  as an invalid regression fixture.
- If an invalid Observer `done` proposal's single repair reaches the output
  limit, one Coder report-only wire attempt may use remaining task allowance.
  Admission requires current declared artifacts/checks/baselines, no pending
  recovery, current source context and accepted references. Only `done` may
  execute, all existing gates remain, and automatic assistance stops afterward.
  The wire ceiling includes transport/API retries. Receipts record
  `purpose: "report_closeout"`; rejected reports terminate `report_incomplete`.
- A valid PR review finding is fixed: `Before: 14 failures, After: 0 failures.`
  is checked against separate phase clauses. Exact command citations stay local
  to each clause. `Before and after: 0 failures` must agree with both phases;
  reversed or unsupported counts still fail. The regression was red before the
  fix and green afterward.

## Frozen live observations

Both roles use **Mistral Large 4**, with the same task prompts, supplied tests,
independent assessor and budgets as the previous examples. One attempt per task
was declared before either provider call. The wave used **24 wire requests**:

| Task | Coder / Observer | Independent original tests | Native completion and strict qualification |
| --- | ---: | --- | --- |
| `env-bool` | 7 / 3 | 10 run; one documentation failure | interrupted; not qualified |
| `event-summary` | 11 / 3 | all 10 pass | completed; qualified |

Settings repaired its source but left documentation unchanged. Its third
Observer stream emitted partial content, then ended without a terminal frame
after 28,013 ms; provider usage is unknown. The exact post-edit verification
remained pending and no `done` was attempted. The report-only path correctly did
not start. See the [original interruption fragment](interruption-r8.json).
This is a stream interruption, not a token-limit or pure schema failure.

JSONL repaired source and docs, ran the required before/after command, and had
its first authored `done` accepted. Reported counts match native receipts:
10 tests, 8 failure events and 5 errors before; 10 tests with zero failure
events/errors afterward. The all-zero result is explicitly limited to empty
input. Source/docs preserve generators and input strings, skip blank lines,
validate exact lowercase levels, and report physical line numbers for errors.
Two earlier Plan Gate refusals are retained.

**Neither live attempt exercised the output-limit report-only path.** That
path is verified by deterministic TUI and headless transport tests, including
wrong evidence, out-of-schema edits, stale context, missing proof, exhausted
budget, and transport reclassification. A fresh model-authored valid report is
required even in the successful simulated recovery. This is separate evidence
from the one successful live task.

These two adaptive development observations do not establish a general success
rate, a causal efficiency improvement or arbitrary-prose correctness. The
interrupted attempt is retained without a retry in this wave.

## Pins and replay

- Runtime source: `f14c943ce3c1aae225a58cbef5eba31bf946d52a`.
- Declaration commit: `a0c49ac2957e6567d8c9cacc8c867446d82cf10d`.
- Per episode: 24 task requests, 4 transport-repair credits, 6 requests reserved
  for Observer, Coder slice cap 8, 90-second request timeout and 600-second wall
  limit. Output limits remain 32,768 Coder / 8,192 Observer tokens.
- [Executor](executor-r8.json) records binary, suite, matrix, declaration and
  preflight hashes. The frozen raw driver, inputs, sessions, traces and work
  receipts remain under its recorded local raw root. The public manifest names
  and hashes 80 artifacts; it does not embed the entire raw bundle.
- Candidate patches: [settings](r8-env-bool.patch),
  [JSONL](r8-event-summary.patch). Both reapply byte-for-byte to their frozen
  fixtures. Replayed original tests retain the same failure/pass outcomes.

Offline validation passed 996 Rust unit tests plus one integration test
(one ignored), 91 Python tests, 80 Web tests, 5 npm tests, 13 TUI replays,
14 repository-map cases and server/UI smoke. The source, receipts and patch
replay were reviewed by the implementation author; this is not a fork review.

The remaining live issue is bounded recovery from a prematurely closed Observer
stream while work is still pending. Such a response cannot provide a complete
action or completion proof; this change keeps it interrupted.
