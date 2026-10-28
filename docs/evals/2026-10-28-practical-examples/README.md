# Small practical examples: provider availability and the next bounded wave

The [examples suite](../../../benchmarks/examples-v1/README.md) adds two authored
practice tasks: boolean settings and JSONL summaries. Each changes one short
source file and one documentation file, with ten visible original tests and no
external dependencies. The difficult coding/recovery suites and their gates are
unchanged. This is a new task tier, not evidence of a runtime performance gain.

## First wave: Luna / Large 4

[Plan](plan.json), [executor pin](executor.json), [preflight](preflight.json),
[results](results.json). The baseline and reference controls passed before calls.

| Coder → Observer | Task | Wire requests | Outcome |
| --- | --- | ---: | --- |
| Luna → Large 4 | env-bool | 3 | Coder HTTP 429 retries exhausted its slice before any action |
| Luna → Large 4 | event-summary | 3 | Same, before any action |
| Large 4 → Luna | env-bool | 8 | Three Coder reads, then five Observer HTTP 429 attempts |
| Large 4 → Luna | event-summary | none started | Withheld after an explicit account-exhaustion response |

The third episode retained `error.type=insufficient_quota` and
`error.code=credit_balance_exhausted`. Earlier Coder errors lost the response body
when the request allocator refused a further retry; their native ledgers retain
HTTP 429, but the exact body cannot be reconstructed. No episode edited a file,
executed a baseline check or completed. There are **0 qualified of 3 attempted**,
with **1 declared but not run**. This does not measure whether the pair can solve
the tasks. The native unrun request count remains unknown in the assessor; no
request was launched for that episode by this driver.

All 92 entries in the raw manifest were verified against hashes, sizes and modes;
the selected credentials were absent from retained text artifacts. Candidate
patches are empty. The assessor is the frozen driver, not the later runtime.

## Repairs prompted by that wave

Three actual coupled-loop regressions fail before the fix and pass afterward:
Coder quota, Observer quota, and temporary throttling that runs out of Coder
allowance. Explicit structured account exhaustion now stops after one request;
temporary throttling keeps bounded retries, and budget denial retains the
previous HTTP cause. Neither case receives transport credit or task completion.
The same quota policy covers the completions fallback; native Anthropic is
unchanged. These fixes were not used in the first wave.

## Follow-up: Mistral Large 4 in both roles

After the quota result, Ryō selected Mistral × Mistral. The follow-up uses a
separate matrix and fresh output directory, two tasks once each, at most 56 wire
requests. It does not retry or replace the first wave's rows. The role pair and
runtime both change, so this cannot isolate a causal harness effect.
