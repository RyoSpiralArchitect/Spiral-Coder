# Carry one authored launcher edit across its verification check

R8 had a working native catalog and an unmodified npm launcher. Step 21 proposed a one-file launcher change while a specific visible check was required. Step 22 passed that check. At step 23 a second proposal mixed in another file and was rejected. The earlier isolated file-tool comparison showed that the first diff would fix the packaged entry. The [public benchmark example](../../examples/replay-catalog-r8.md) reproduces the original passing and failing checks from the repository.

The native shared task context now returns the original arguments for review when a complete read, a verification refusal and its successful check form an unambiguous sequence. The same proposal remains available after one matching complete reread of unchanged contents. The normal file-tool preview, scope, evidence requirements and approval policy govern the new call.

| Sealed prefix | Prior executor | New executor |
| --- | --- | --- |
| Through step 21, before the visible check | No offer | No offer |
| Through the successful check at step 22 | No offer | Exact original launcher diff and the two native call IDs |
| Through the failed mixed-file attempt at step 23 | No offer | No offer |

A second model-free replay restored the candidate and seeded the native coupled loop from the original session through step 22. Two scripted HTTP responders read the actual Coder and Observer request bodies. On the new executor the Observer used the offer to recommend the original arguments. The Coder accepted, submitted those arguments in a normal `apply_diff` call and changed only the launcher. The actual packaged-entry test passed. On the prior executor the same script had no offer to follow, made no edit and the packaged test failed. Neither comparison spent any external provider requests. The local continuation uses its own budget and does not change the archived episode.

[Prefix comparison before](before.json) · [After](after.json) · [Prior local pair](paired-before.json) · [New local pair and matching call](paired-replay.json) · [Frozen R9 protocol](protocol.md)

## What happened on the fresh ninth run

The same `gpt-5.6-luna` Coder and Observer started from the original repository. The R8 task, probes, reference controls and budgets were held fixed. Only the executor changed. The pair used 24 Coder and eight Observer requests in 365 seconds, with zero transport repairs. The native loop accepted a blocked report. [Pre-run declaration](https://github.com/RyoSpiralArchitect/Spiral-Coder/commit/d5c4e70313fab685322d01c0f092cd0ade98ba2a) · [Plan and executor](plan.json)

| Measurement | R9 result |
| --- | --- |
| Baseline failure and final specified Cargo test | Both observed in the native trace |
| Native visible check and independent visible check | Both failed. The command was absent from CLI help. |
| Held-out cases and packaged entry | 0/11 and 0/1 passed |
| Independent build and ordinary replay | Both passed |
| Stock regression suite | Stopped at the formatting check in `src/tui_replay/catalog.rs` |
| Candidate and qualification | Three files changed, one outside the declared scope. The candidate remained blocked and unqualified. |

The pair wrote a catalog module and declared it in `src/tui_replay.rs`. It did not wire the native CLI, npm launcher or documentation. There was no native `apply_diff` held for verification on this run. Re-evaluating its retained successful check on the pinned executor produced no deferred proposal. The fresh run therefore did not exercise the handoff reproduced from R8. Reapplying the unchanged candidate patch to the pinned baseline reproduced all candidate files. [First run and independent checks](results.json) · [Trace observations](deferred-observations.json) · [Plain replay](plain-replay.json) · [Candidate patch](candidate.patch) · [35 raw evidence hashes](receipts.json)

## The runtime steered the pair to an excluded module

After repeated reads of `src/main.rs`, the native `mutate_existing_now` policy named `src/task_scope.rs` three times. The task explicitly listed six editable paths and this file was outside that list. At step 14 the Coder submitted a `patch_file` that added an unrelated module comment. The native tool executed it. The trace records zero tool-argument rewrites. [Policy events](deferred-observations.json) · [Changed calls and exact scope verdict](results.json)

The follow-up runtime uses the current human task's explicit edit list when selecting targets and checking named file edits. A scripted native Coder replay submitted the original step 14 tool arguments after the recorded preceding read. The fixture supplies its own reasoning prelude. The frozen R9 executor recommended the excluded module and changed it. The corrected executor returned an edit-scope refusal, with no forced target and identical file hashes before and after. [Runtime source](https://github.com/RyoSpiralArchitect/Spiral-Coder/commit/2f2b743156db7b0b156eeadb5a6483aed4ec95af) · [Before](scope-before.json) · [After](scope-after.json) · [Replay driver](replay_scope.py)

The same corrected binary also requires a unique whole-line anchor for a deferred diff. On the original R8 prefixes it offered the launcher only after step 22. In the local two-role continuation, the Coder accepted the Observer's recommendation, submitted the original arguments and changed only the launcher. The packaged-entry check passed. These two post-run comparisons used scripted local responses with zero external requests. The published R9 outcome remains the first live result under its predeclared executor. [Three prefixes](reviewed-prefixes.json) · [Two-role replay](paired-reviewed.json) · [Replay driver](replay_pair.py)
