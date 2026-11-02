# All-or-nothing recovery of a bare diff trailer

Run once using the exact r4 prompt, candidate commit, allowed paths, probes, reference, driver, model pair and budget. Change only the executor to one with the narrow file-tool repair. Use the same [reuse observations and qualification gates](../reuse-r4/protocol.md).

Record the number of executed diffs with a bare trailer, the result of each and any partial write, separately from attempts that never executed. Identify reported format repairs in native output. If neither role emits such a diff, that episode provides no direct test of the format change. Track any duplicate helper and any evidence of reuse separately from syntax handling.

The offline comparison replays the original r3 inputs. Apply the first multi-hunk request to the original source on both executors. On the replacement executor, apply each later request independently to the pre-edit bytes reconstructed with the original executor. Compare its complete output to the explicitly trimmed result retained in r3. Count helper definitions in each result.
