# A Coder and an Observer add a native CLI command

In the eighth replay-catalog episode, the GPT pair added `tui-replay-list` to Spiral-Coder's parser and connected it to a handler. Running `tui-replay-list --filter PREFS --max-cases 1` returned one row, `pane-prefs-observer-suggestion`, with its four tags. The native visible check and the independent check both passed. The installed npm entry still routed the command to the TUI. [Sealed R8 results](../evals/2026-11-02-cli-route/results.json)

| Original outcome | Evidence |
| --- | --- |
| Native build, filtered listing and ordinary replay passed | The existing replay control exercised successful and failing cases. |
| 10 of 11 held-out cases passed | The failure was the missing usage documentation. |
| The pair reported blocked | Coder used 24 requests and Observer used eight. The stock suite stopped at formatting, and the final exact Cargo test lacked a fresh in-episode receipt. |
| The candidate remained unqualified | The independent patch replay reproduced all three changed Rust files on the original baseline. |

## Reproduce it without provider calls

Use a Git checkout with the archived baseline commit available, a Rust toolchain,
Node.js and Python 3 on a POSIX host. From the repository root run:

```sh
python3 -BIS benchmarks/self-dogfood-v2/replay-catalog/replay_r8.py
```

The command builds the exact archived patch in a temporary copy, runs the native filtered listing, the held-out cases, the packaged entry and ordinary replay. Its final JSON contains the listing and the check exit codes. A zero process exit with `matches_archive: true` means the observed successes **and** the two expected failures matched. The original result stays `qualified_candidate: false`. Cargo may need to fetch its locked dependencies on the first build. To reuse compiled dependencies, pass `--target-dir` with a dedicated disposable Cargo cache. The replay rebuilds the root package in that cache.

The [public archive index](archives/replay-catalog-r8.json) binds the baseline, executor, first-run receipt, patch, original oracles and filtered output. The original [run declaration](../evals/2026-11-02-cli-route/plan.json), [candidate patch](../evals/2026-11-02-cli-route/candidate.patch) and [step-by-step record](../evals/2026-11-02-cli-route/README.md) are available separately. The full-repository [task definition](../../benchmarks/self-dogfood-v2/replay-catalog/README.md) also describes how to prepare a fresh live episode.

At step 21 the native gate held an edit to the npm launcher until the visible check ran. At step 22 that check passed. At step 23 the pair combined the launcher and documentation edits in one diff, which the file tool rejected. A separate replay on a disposable copy applied just the original step 21 arguments and passed the package test with the same binary. That result belongs to the later control. [Launcher comparison](../evals/2026-11-02-cli-route/launcher-control.json)
