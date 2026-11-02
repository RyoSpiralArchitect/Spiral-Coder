# From a public CLI name to its Rust dispatch

On replay-catalog R7 the Coder searched the source file for a sibling command's public name, then proposed changing a line in the inventory. A disposable rebuild confirmed that the extra inventory entry left a real filtered invocation unable to parse its flag. [R7 compiled control](../2026-11-02-observer-summary/README.md)

The scoped search now includes a small navigation note when the same Rust source contains the corresponding UpperCamelCase identifier. We replayed R7 action 21 with its original file bytes and arguments through a native coupled loop. Both the Coder's next tool message and the Observer's action packet retained the three original literal search rows. Both also received the two locations of `TuiReplay`. In the pinned source, line 93 declares the argument-bearing variant and line 618 dispatches it to its handler. [Variant](https://github.com/RyoSpiralArchitect/Spiral-Coder/blob/55351155d73675605e478e0518cc09279ae02b80/src/main.rs#L93) · [Dispatch](https://github.com/RyoSpiralArchitect/Spiral-Coder/blob/55351155d73675605e478e0518cc09279ae02b80/src/main.rs#L618)

The local test used four scripted Coder requests and one scripted Observer request per executor, all on loopback. It captured the actual outgoing bodies. The extra line identifies a spelling and sample locations. A public invocation still needs its own verification. Two positive regressions failed with the old runtime. The final local suite passed 1,050 Rust unit tests with two existing ignores, four integration tests and 15 TUI replays, plus the Python, Web, npm, repo-map and server checks.

[Before](before.json) · [After](after.json) · [Reproduction script](replay_search.py) · [R8 protocol](protocol.md)

Run `cargo test --locked file_tools::rust_navigation_tests` for the public regressions. To repeat the two-role comparison, run `python3 -BIS replay_search.py EXECUTOR SEALED_R7 NEW_OUTPUT_DIRECTORY` from this directory once per binary, with a fresh output directory for each replay.

In a second local run, a scripted Observer used the two locations from the search result to request one line at a time. The Coder accepted each advisory and called native `read_file`. The two assist receipts identify the matching calls. The returned lines contain the declaration and the dispatch shown above. This fixture used 12 local Coder and three local Observer requests. Add `--follow` to the replay command to reproduce it. [Native follow-through](followthrough.json)

## The first passing native catalog listing

R8 was the first of the eight frozen episodes to produce a passing independent visible CLI check. The Coder and Observer used 24 and eight task requests, respectively. The native command accepted the requested filter and cap and returned the expected JSON. Its post-edit visible test passed in the episode and again on the independent candidate build. The existing ordinary replay still passed its pass/fail control.

| Gate | Sealed R8 result |
| --- | --- |
| Native public command | `src/main.rs` gained an argument-bearing command variant and the matching dispatch arm. A real invocation of the compiled program produced the selected catalog row. |
| Held-out checks | 10 of 11 passed, including validation, ordering, filtering and empty selection. The only failure was the missing usage documentation. |
| Installed entry | The npm launcher still sent `spiral tui-replay-list` to the TUI. |
| Stock checks | The suite stopped at formatting in the new Rust file. The pair did not finish the required exact Cargo command after its final edit. |
| Closeout and eligibility | The runtime accepted the model's blocked report. Three in-scope Rust files changed. The exact patch reproduced all candidate bytes on the plain baseline. The candidate did not qualify for adoption. |

An early catalog edit referred to a missing CLI argument type. After the pair introduced the variant and fixed its argument type, the compiler reported a non-exhaustive match. The pair read the dispatch region at native step 18 and added the arm at step 19. The visible test passed at step 22. The only live search attempted `TuiReplayArgs` under the `src` directory and was blocked before execution. There were zero retained identifier-location notes. The deterministic paired replay demonstrates the new navigation behavior. The independent live result demonstrates the working native command. The first passing listing cannot be attributed to the extra locations from this episode.

[R8 declaration](plan.json) · [Preflight](preflight.json) · [First run and independent gates](results.json) · [R7 route observations](r7-route.json) · [R8 route observations](route-observations.json) · [Candidate patch](candidate.patch) · [Plain-source replay](plain-replay.json) · [35 sealed file digests](receipts.json)

## What separated the native command from the npm entry?

At step 21 the Coder had already written a one-file edit adding the new name to the launcher. The pending verification gate blocked it. Step 22 satisfied that visible check. The next diff combined the launcher edit with an update to the documentation inside the same file-tool call. The tool rejected the mixed edit.

After assessment, we rebuilt the sealed candidate in a disposable copy and ran its frozen package oracle. The original failed. Replaying the exact mixed step 23 arguments through the native file tool changed no files. Replaying the original step 21 arguments changed only the launcher, and the same packaged-entry test passed with the same candidate binary. This comparison tests the edit and its effect on routing. The official episode ended before the launcher edit succeeded.

The next available case is a paired handoff after a required check. The Coder and Observer could surface the earlier one-file proposal, recheck its target and submit it through the ordinary edit policy. The combined proposal and the other outstanding work remain independently observable. [Packaged comparison and filtered JSON](launcher-control.json) · [Reproduction script](replay_launcher.py)
