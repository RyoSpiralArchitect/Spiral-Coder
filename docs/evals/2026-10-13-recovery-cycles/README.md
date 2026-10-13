# Four recovery cycles — 2026-10-13

Four source changes accumulate failure modes from the
same three weak models. The six-case spec, 33 fixture hashes, acceptance checks,
and iteration caps remain unchanged. The
[previous record](../2026-10-13-repair-feedback/README.md) is retained in full.

## Changes and identities

| Round | Runtime source | Purpose |
|---|---|---|
| 1 | `7729b1283da6d6b5c54c9f0a4989303acd7fe6c7` | Aggregate reflection/impact feedback, exact enum choices and current plan references; explain zero native calls even when a block fails first. |
| 2 | `9e4e7fa7527817923c8ec3d1ebda56d833ba25ed` | Expose all pending exact verification commands, correct the one-call final-turn instruction, localize semantic replay failures and explain Observer/check input surfaces. |
| 3 | Recorded in receipts | Repair described file-existence dependencies and preserve the exact configured command in verification feedback. |
| 4 | Recorded in receipts | Preserve exit statuses and decimal literals inside numbered plan items; valid four-item plans must not be counted as oversized. |

All phases use temperature 0, max output 2,048 tokens, 90-second request timeout,
900-second outer suite timeout, and case caps 14/14/10/8/12/18. Providers may
overlap within a round; each round completes before the next run begins. Each
case has a fresh copied workspace and its own Cargo target directory. There is
no fixed random seed. Credential values stay in the selected child environment.

## Deterministic evidence

Round 1 first reproduces incomplete reflection/impact diagnostics against the old
implementation. Its complete local checks pass with 654 Rust unit tests. Round 2
first reproduces the missing semantic replay diagnostic, then passes 657 Rust
unit tests (1 ignored), 1 integration, TUI replay 3/3, Python 12, Web 53,
repo-map 14/14, syntax checks and Lite smoke. Round 3 adds a configured-command
round-trip regression and broadens existence-claim dependency tests. The seven
new regression tests and extended coverage include:

- reflection enum choices, independent missing fields, and unchanged rejection
  of invalid `continue` or `keep` after repeated failure;
- current impact references, missing fields and unchanged invalid-reference rejection;
- the actual HTTP retry after simultaneous invalid impact and absent native call,
  without executing textual XML commands;
- all pending command identities, combined-command rejection, edit invalidation,
  and bounded feedback that never truncates a command into another command;
- semantic replay failures with actionable Observer input diagnostics, retaining
  the failed assertion and unchanged fixture contents;
- actual replay failure through automatic-test/history compaction into exact
  input-file recovery focus and resumed state, without granting repair proof.
- described affirmative file-existence claims, negative/compound exclusions,
  exact edit dependencies and fresh versus cached confirmation;
- verification suggestions accepted by the same gate, preserving `grep -Fx`,
  mixed-case paths and double spaces inside quoted arguments.
- Round 4 adds an eighth regression for correct numbered-item boundaries, compact
  numbered syntax, and unchanged rejection of a real fifth acceptance criterion.

## Live results

Results and the failure-mode ledger are finalized after all twelve runs.
Round 1: Mistral 6/6, GPT 5/6, Gemini 4/6. All three replay artifacts retain their
original case and contain two JSON-parseable cases. This supplementary audit is
separate from task completion and the frozen pass criteria.

No promotion or broad reliability claim follows from local checks or one live
score. The planned unchanged confirmation runs were replaced by concrete repairs
after new counterexamples appeared. There is one run per source/provider, so
this record does not establish repeated reliability or nonregression.
