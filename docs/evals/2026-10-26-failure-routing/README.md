# Separate failure ownership through Coder–Observer handoffs

The runtime now retains older unresolved checks while routing diagnosis to the
newest failed check. The controlled live attempts still produced **no qualified
repair**. This establishes the covered recovery behavior, not a repair-success
uplift or a model ranking.

## Frozen comparison

[plan.json](plan.json) was committed as `41d2fd0` before any calls. Gemini was
excluded while its quota was unavailable. The new matrix uses Luna 5.6 with
Mistral Large 4 in both role directions. Each pair has one baseline attempt and
one candidate attempt, with the same prompt, task inputs, protected/editable
scope, independent oracle, driver and budgets. This is separate from the prior
Gemini comparison.

- Task source: `9982a68a6978058c75a9a700e7407df5a49f010c`, with the unchanged
  `recovery-handoff-visible` overlays.
- Baseline executor: `5be9d51f54520d5e8ff57a018247d734a2703859`, binary SHA-256
  `b74db0f8649898fffb76f80686f56ace4d5e62c74c027bd5e51d12991e811b9c`.
- Candidate executor: `3a9e523f3ac147acf7c70f7fe4914f0623bca3ec`, binary SHA-256
  `a56753512abaa04f8dd0ba309d4b2cf8fed837952546da371a69c21baea2bbe2`.
- Per attempt: 32 shared requests, 1,200-second wall cap, 90-second provider
  timeout, Coder slice 8, output ceilings 32,768 Coder / 8,192 Observer tokens.
  All native retries consume the shared cap. No episode was retried or steered.
- The package-cache-reset driver is unchanged. Every baseline/reference
  preflight passed, including the expected failing task checks and passing
  feasibility reference, before provider calls.

The executor remains separate from the candidate workspace. Model patches are
retained proposals; none was adopted, merged or published.

## Outcomes

| Wave / Coder → Observer | Requests C / O | Native handoffs | Opener recoveries | Visible | Independent | Qualified |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| Baseline Luna → Large 4 | 25 / 5 | 2 | 2 | 0/1 | 5/6 | No |
| Baseline Large 4 → Luna | 26 / 6 | 5 | 0 | 0/1 | 5/6 | No |
| Candidate Luna → Large 4 | 25 / 7 | 5 | 5 | 0/1 | 5/6 | No |
| Candidate Large 4 → Luna | 6 / 1 | 1 | 0 | unchanged 0/1 | unchanged 5/6 | No; Observer stream interrupted |

All four remained natively incomplete. All four assessment copies built, passed
the original **864 Rust tests** (one existing ignored), and passed their
seven-case TUI replay. All preserved the protected files and edit scope.
The baseline Large 4 candidate changed the required documentation; neither Luna
candidate did. The interrupted candidate made no edits.

The four attempts consumed **101 of the declared maximum 128 requests**.
The collector verified **3,745 raw file entries**, all declaration/run/assessment
pins, and **sixteen successful package-cache-reset receipts**. Episode wall caps
and output limits did not fire; no secret redaction was required.
[results.json](results.json) retains the detailed gates, request order/status,
assists, native errors, recovery snapshots, checks and artifact hashes.

## What the live traces show

In candidate Luna → Large 4, the original explicit visible check remains pending
when the automatic `cargo test 2>&1` fails. The later exact
`cargo test --locked --bin spiral-coder -q` has its own failure state as well.
The retained snapshots show up to three separate unresolved commands. A newer
ambiguous failure gets an unlocalized diagnosis instead of borrowing the
original visible-test path. This exercises the new ownership branch, but does
not establish that the model understood or repaired the underlying source bug.

Mistral Observer's missing opening brace/quote recovered **seven times across
the two waves** under the already-existing narrow parser. Authored values and
raw responses were retained. These are live framing-recovery observations;
accepted advice or extra Observer calls did not establish task completion.

Candidate Large 4 → Luna returned the first failed explicit verification to
Observer. Its only Observer request received HTTP 200 headers, then ended after
90,002 ms with `Observer next-action failed: error decoding response body` and
an incomplete JSON prefix. No assist or continuation followed. This is retained
as a stream/transport interruption, not evidence that Luna cannot produce valid
JSON or a measured token-limit failure. The episode-level wall cap did not fire.

The top-level report nevertheless has `error: null`, the request remains marked
`http_200`, and `agent_end.ok` is true. The pane and trace preserve the actual
error; the high-level report currently loses that stop reason.

## Deterministic repair and review

The implementation keeps one failure state per exact command, makes the newest
failure active, and clears only the command that actually passes. Shared Coder,
TUI Observer and browser projections use that active diagnosis. Native Rust/
Python locations can identify a failing supplied test different from the edited
source; ambiguous or unsafe locations cannot be replaced by a guessed file.

Fork review found and corrected two additional P2s: a later automatic success
could prevent restoration of the older pending verification stage, and actual
context pruning could discard the success needed for that restoration. The
final corrective recheck found no actionable issue. The pruning regression
exceeds 48 messages and verifies that a disposable exchange is removed while
the failure/resolution evidence remains.

[validation.json](validation.json) records **939 passing Rust tests**, one
existing ignored test, the server integration test and ten TUI replays on the
final source. The earlier full local gate also passed 73 Python, 80 Web, five
npm, fourteen repo-map cases and server smoke; the review corrections only
changed Rust recovery code and documentation. [source-ci.json](source-ci.json)
records successful Linux/macOS quality checks, five native platform builds and
package verification for `3a9e523`. Publication was skipped.

## Remaining work

1. Preserve Observer stream/decode failures in the top-level report and request
   terminal status. A bounded retry, if introduced, must retain the partial raw
   response, spend the same cap and never dispatch incomplete advice.
2. Distinguish a truthful incomplete handoff from a successful completion claim.
   Candidate Luna authored three `done` calls with nonempty remaining acceptance;
   the pending-failure gate rejected all three. On the final request the wire
   contract offers only `done`, while Observer still requests a diagnostic read.
   Completion must stay false without making an honest remaining-work report
   impossible.
3. Investigate repeated diagnosis of an unchanged failure. Any reuse needs to be
   tied to the exact check, relevant mutation and diagnostic identity; a newer
   unrelated failed read/check must not inherit the older diagnosis. The live
   models still failed to repair the source transition within the fixed budget.

These are follow-up questions, not additional changes or extra live attempts in
this comparison. There is one sample per pair per wave, so no causal improvement
estimate is claimed.
