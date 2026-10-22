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

## Round r2 after the correlated-header fix

Source `a0488fe55c3ce875788d51274a66a8dd6e946447` (runtime change in `a1e1be1`).
See the [predeclaration](r2/predeclared.json), [receipts](r2/receipts.json) and
[raw hashes](r2/raw-manifest.json). The task, fixture bytes, selected models,
temperature, output limit, request timeout, shared request cap and wall cap match
r1 exactly. Each provider starts in a fresh workspace with the frozen candidate
binary. This is one adaptive follow-up per provider, not an estimate of success
probability or an isolated causal effect of the fix.

| Same model for both roles | Requests | Range read executed | Independent tests | Completed | Exact reference source |
| --- | ---: | --- | --- | --- | --- |
| mistral-small-2603 | 16 | yes | pass | yes | no: one extra trailing space |
| gpt-4.1-mini | 14 | yes | pass | yes | yes |
| gemini-3.5-flash-lite | 18 | yes | pass | no | yes |

All three recorded the failing native baseline before editing, preserved all 600
prefix comments and supplied tests/configuration, and ran the exact native check
successfully after editing. Independent checks against frozen original tests pass
3/3; runtime completion is 2/3. Mistral adds a trailing space on the repaired
return line, so its source is not byte-identical to the minimal reference.
Requests total 48 in r2 and 102 across both rounds. No episode was retried in place.

Gemini first cites a task-requirement ID as if it were the accepted plan's
criterion. Its next completion attempt corrects that evidence mapping but omits
the required authored before/after outcomes. The final-report gate rejects the
attempt at the shared 18-request cap. Its repaired source and passing native
check do not substitute for the missing requested report. Mistral uses one
Observer assist; GPT repairs its report within the remaining Coder budget.

Strict all-file qualification remains **0/3**. Each workspace changes `greet.py`,
five `.spiral-coder/` runtime files and the existing `__pycache__/greet...pyc`.
These paths remain in the receipts; they are not excluded after observing the
result. Both accepted runtime completions pass the original tests, but that does
not certify the task's strict changed-file scope. Mistral's authored "Only
greet.py was changed" claim is not accurate under that full-filesystem measure.

The remaining failure modes are closeout evidence/report generation under a
shared budget, discovering relevant lines beyond a truncated prefix, and runtime
artifacts inside a restricted workspace. None is resolved by increasing the
model size, relaxing completion requirements or dropping changed paths here.

## Deterministic validation

At the final runtime source: **805 Rust tests passed, 1 ignored; 1 integration
test passed; 80 Web tests passed; TUI replay 4/4**. Formatting and diff checks
passed. The evidence audit verifies all 62 r2 raw-file hashes, identical declared
inputs across rounds and the exact candidate source differences. These checks
establish the bounded contracts and retained observations, not general coding
reliability.
