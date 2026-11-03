# Iterations after R18

R18 stopped with three Rust files changed. The CLI called a handler that
did not exist. The guide, npm entry, two final verification receipts and
Coder report were still due. This series keeps the original R18 run and
works from separate copies of its exact source and native session.

| Condition | Native result | Independent result |
| --- | --- | --- |
| First continuation, gpt-5.6-luna in both roles | Three Coder and two Observer sends. The Coder read the failed dispatch, repaired it in `src/tui_replay.rs`, and passed the visible native check. Both Observer suggestions were rejected and the episode stopped with 27 task requests unused. | Build, visible and ordinary replay passed. Hidden cases were 8/11. The installed entry, guide and formatting were unfinished. [Result](results.json) · [Original Observer responses](observer-refusals.json) |
| Continuation from that repair, same pair | Eight Coder and two Observer sends. The launcher changed at step 28 and the guide at 30. Both native checks passed at steps 31 and 32. The Coder's successful report was accepted at 33. | The build, native listing, installed npm entry and ordinary replay passed. Hidden cases were 9/11. Empty-spec and empty-selection diagnostics differed from ordinary replay, and the stock check stopped at formatting. [Result](range-return/results.json) · [Composite patch](range-return/composite.patch) |
| Public operator acceptance, same pair and workspace | After opening a new human task, the pair corrected the example and shared the ordinary parser and selector. The last guide edit at step 11 was followed by all four exact checks at 12–15 and an accepted Coder report at 16. Sixteen Coder and eight Observer sends used 23 task requests and one transport repair. | Five of six independent groups passed, including the original stock regression suite. The installed example matched its published JSON. Held-out coverage reached 10/11. A no-match filter still produced a different diagnostic. [Acceptance evidence](acceptance/README.md) |

The native closeout on the second condition used two exact commands after all edits. The visible
check passed its one test and Cargo reported 1,030 passed, zero failed and
two ignored. The Coder called the first count independent in its report.
That command was executed inside the Coder episode. An external assessor
later ran its own visible check. The report correctly named the five files
in the composite and left no acceptance criteria outstanding.

On the first continuation the Observer initially proposed a launcher patch
without an anchor. Its repair proposed a 220-line read. Replaying the same
two replies with local endpoints reproduced the early stop. The revised
runtime kept both rejections and put one native, 200-line read window
for the outstanding launcher on the next Coder wire. The later live pair
found the launcher and guide on its own; the local replay is the evidence
for that particular fallback. [Matched wire capture](range-return/replay-control.json).

All candidate source changes in the chain were authored by the pair. The
operator migrated R18's already validated terminal summary and reason from
its older report format, and rebound the workspace prefix in disposable
sessions. The original tree, prompt, native call IDs and returned values
were verified before use. Each composite was independently reapplied.
[Original source and budget](plan.json) · [Remaining-budget condition](range-return/protocol.md).

The three separately declared conditions after R18 used 38 task requests
and 39 wire attempts in total. The original single-episode qualification
stays failed. Each post-R18 chain reached its terminal condition in its first
allocated round. The paid #49 evidence is therefore about saved-session
continuation and new human follow-ups. The [separate stock pilot](automatic-pilot/README.md)
exercises one invocation across two real model iterations.
[Model-free runtime checks](runtime-controls.json) exercise the automatic
second iteration, the last Coder report, and proof separation on a follow-up.

The [prospective fresh runs](../2026-11-04-fresh-iterations/README.md) record the stock pilot and the full catalog task with newly generated Luna responses.
