# Bounded reads of a large source file

This new fixture places the existing greeting bug after 600 preserved comment
lines (37,846 bytes total). The task, supplied original tests, exact before/after
command, selected weak models and shared 18-request cap match the preceding small
regression. The source layout differs, so this is not a controlled performance
comparison with its 3/3 completions or the original repository/JSON panel.
Coder and Observer use the same selected model per episode. Temperature 0,
2,048 output tokens, 90-second request timeout and 600-second wall cap are fixed.
No episode is retried in place. Independent tests run against the candidate source
and frozen original tests in a fresh directory; the collector also compares the
entire source with the one-character reference fix and retains every changed path.

## Round r1

Source `e125bac0a813ed27ef55c21d9bea8c810d07ed08`. See the [predeclaration](r1/predeclared.json),
[receipts](r1/receipts.json) and [raw hashes](r1/raw-manifest.json).

| Same model for both roles | Requests | Range read executed | Independent tests | Completed |
| --- | ---: | --- | --- | --- |
| mistral-small-2603 | 18 | yes | fail | no |
| gpt-4.1-mini | 18 | yes | fail | no |
| gemini-3.5-flash-lite | 18 | yes | pass | no |

All three recorded the expected native failing baseline and preserved supplied
tests/configuration. Gemini's source exactly matches the reference fix, including
all 600 comments, but native final verification/reporting stayed incomplete.
All episodes exhausted their shared cap; total 54 requests, no false completion.
Strict all-file qualification is 0/3, including runtime state and bytecode paths.
These are retained as changed files, without retroactive exclusion.

The run exposed a newly introduced header incompatibility: `read_file` returned a
valid range of the failure file, but the diagnostic gate only accepted the old
full-read header. Mistral and GPT consequently remained in diagnosis after that
read. GPT also repeatedly read ranges ending at line 600, before the actual
function at lines 601–602. Plan/think repairs consumed requests across providers.
These are distinct failure modes; the header fix alone does not establish broad
reliability or solve target discovery and closeout budgeting.

The follow-up recognizes a range header only when its path and bounds agree with
the correlated native call, including the existing pruning suffix. It advances
the existing path-level diagnosis obligation without creating verification or
whole-file overwrite authority. Deterministic coverage includes mismatched
headers, continuation, Web forwarding, and rejection of whole-file overwrite.
