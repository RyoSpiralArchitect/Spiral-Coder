# GPT-pair repairs before self-dogfooding

With PR #45 merged, `gpt-5.6-luna` in both roles strictly qualified on **both
practical repairs**. In each episode the pair reproduced the failure, changed the requested
source and documentation, ran the exact post-edit check, and supplied an accepted
final report. Reapplying each patch to its frozen fixture reproduced the complete
candidate workspace and passed all ten original tests. [Frozen results](results.json)

| Task | Coder / Observer requests | Independent original tests | Strict qualification |
| --- | ---: | ---: | --- |
| Boolean settings | 12 / 4 | 10/10 | qualified |
| JSONL event summary | 17 / 5 | 10/10 | qualified |

The [preceding Mistral pair](../2026-10-30-observer-reconnect/README.md) qualified
on settings in 16 requests. It repaired both JSONL artifacts but ended blocked
at 24 requests after three Plan Gate refusals of the final verification. Here the
GPT pair reached that check and its first authored `done` was accepted, using 22
requests. This comparison covers one attempt per pair on each of these two
previously used, visible practice tasks.

## What happened during the repairs

**Settings.** The baseline recorded ten test methods and 14 failure events. One
response proposed two reads together, so neither ran. An edit lacked its evidence
block, and a documentation overwrite needed a complete read. The four Observer
continuations have matching executed calls for a diagnostic read, a source patch,
another diagnostic read and final verification. The resulting parser handles
case, whitespace, defaults, explicit tokens and invalid strings. The original
suite then passed, and the final report named both files and the before/after
command outcomes. The requested arrow examples appear inside a Python-labelled
documentation fence as display notation. [Settings patch](env-bool.patch)

**JSONL.** The baseline recorded ten test methods, eight failure events and five
errors. Two `apply_diff` calls failed to match the old file. The first included
source lines absent from the fixture and a documentation hunk in a single-file
request. The second included a trailing `*** End Patch`. An offline check with
the pinned runtime parser found **zero** matching old blocks for the retained
second diff and **one** after removing only that line. The live pair later
succeeded with a smaller hunk. [Retained diagnostic](review.json)

The JSONL trajectory also records one Plan Gate refusal, two Recovery Gate
refusals, an evidence refusal and an overwrite requiring a complete read. The
Coder marked all five Observer suggestions accepted. Three have matching executed
calls, including the final exact `exec`. One matching patch was refused and one
suggested write has no matching call. The finished repair preserves generators
and input strings, rejects invalid records with physical line numbers, and
documents the zero counts specifically for empty input. The native final run
recorded ten tests with zero failures or errors. [JSONL patch](event-summary.patch)

## Reproduction and evidence

- [Pre-call plan](plan.json) at commit `816d78472199b26db8539a1dd09572f7450aef5f`.
  The tested runtime is merged commit `a037e14161f5e8f055856b4b0d8a58ce5e0ea1b4`.
  Its executable hash, suite hash, driver hashes, task texts, initial workspaces
  and all 14 files under the case directories match the Mistral r9 pins.
- [Offline controls](preflight.json) show both broken baselines failing and both
  reference repairs passing. The locked runtime build and all 91 Python tests
  passed before the live calls.
- The reusable [GPT profile](../../../benchmarks/examples-v1/matrix-openai.json)
  uses separate Coder and Observer contexts and the existing Luna adapter with
  `reasoning_effort=none`. The adapter omits the temperature field on the wire.
  Each task allows 24 task requests, six reserved for Observer and up to four
  transport repairs. The Coder slice cap is eight requests. Output caps are
  32,768 and 8,192 tokens respectively.
- Both declared episodes ran once. All **38** wire receipts completed and used
  task allowance. There were zero transport charges, Observer reconnect attempts
  or report-only closeouts. Provider token usage is unknown for all 38 receipts.
- The [supplementary review](review.json) preserves native counts, actual reads,
  authored reports, refusals, handoff matches and patch replay hashes. All 79
  entries of the [raw manifest](raw-manifest.json) were verified against the local
  bundle. The original sessions and traces remain under the root recorded there.
  [Instructions for running a fresh pair](../../../benchmarks/examples-v1/README.md#run-a-bounded-pair)
