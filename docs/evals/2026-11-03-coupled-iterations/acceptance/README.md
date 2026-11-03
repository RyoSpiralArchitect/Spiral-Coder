# Production acceptance on the saved catalog

Luna worked as both Coder and Observer. The operator supplied a new task
after independently finding an incorrect published example, a formatting
failure and duplicated spec loading and selection. Both roles received the
original conversation plus fresh acceptance criteria. The run used an
untouched copy of the preceding candidate and no operator-authored source
edits. The previous assessment was known to the operator. The [task](task.txt)
and [limits](plan.json) were frozen before the first request.

The previous launcher and native command remained usable. During this task
the Coder changed three files. It made the catalog use the ordinary replay
loader and case selector, formatted the Rust and corrected the guide's JSON.
The [receipts](public-verification.json) record the final steps.

| Coder step | Native action on the final tree | Outcome |
| --- | --- | --- |
| 11 | Patch the guide with output measured from the candidate | Changed |
| 12 | Check Rust formatting | Passed |
| 13 | Run the published catalog subcommand and arguments through Cargo | The returned JSON matched the corrected guide |
| 14 | Run the exact visible check | Passed |
| 15 | Run the exact Rust binary suite | 1,030 passed, zero failed, two ignored |
| 16 | Submit the final report | Accepted, no declared checks left |

The 24 wire sends comprised 16 Coder task requests, seven Observer task
requests and one Observer transport repair. The run completed in its first
24-task-request round, in 323.483 seconds.

An external assessor applied the [composite](composite.patch) onto the
original tree and matched the resulting inventory. Build, visible, installed
npm command, ordinary replay and stock regressions all passed. The stock
check included 13/13 TUI replays. The [literal package example](guide-after.json)
also matched its shown case ID and four tags. It had [disagreed](guide-before.json)
before the acceptance run. The held-out group passed 10 of 11 tests.
An unmatched filter exits with an error but prints `no replay cases selected`.
Ordinary replay uses its detailed empty-selection diagnostic, including
`0 cases`. The Coder's final report describes the shared helpers and its
four requested checks accurately. It does not establish parity on this
remaining error branch. [Independent results](results.json).

The [runner](../live.py) used the one CLI-token substitution pinned in the
plan, selecting `--follow-up-session` in place of `--seed-session`.
The native runtime was `93b4201`. The later last-request reservation and
ambiguous-edit rules were checked separately on `f1717c6`. The original
single-episode R18 verdict and the two earlier continuation scores are kept
in the [series](../README.md).
