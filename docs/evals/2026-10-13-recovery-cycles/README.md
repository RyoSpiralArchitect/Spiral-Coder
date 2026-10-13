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
| 3 | `e678081f1ad3ba8092641fe65463eb5c7c4e8ede` | Repair described file-existence dependencies and preserve the exact configured command in verification feedback. |
| 4 | `ac9e83c9db005a454b32ab8128ea4ca81b62dc68` | Preserve exit statuses and decimal literals inside numbered plan items; valid four-item plans must not be counted as oversized. |

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

| Model | Round 1 | Round 2 | Round 3 | Round 4 |
|---|---:|---:|---:|---:|
| `mistral-small-2603` | 6/6 | 4/6 | 5/6 | 6/6 |
| `gpt-4.1-mini` | 5/6 | 5/6 | 4/6 | 5/6 |
| `gemini-3.5-flash-lite` | 4/6 | 3/6 | 4/6 | 4/6 |

All 12 suites / 72 cases finished within the outer bound. No provider retries or
reported transport errors occurred. [Receipts](receipts.json) retain every check,
source/binary/spec/fixture identity, raw artifact hash, gate counts and artifact
preservation audit. The previous receipts are linked by hash.

The final local checks pass with 659 Rust tests (1 ignored), 1 integration,
replay 3/3, Python 12, Web 53, repo-map 14/14, syntax checks and Lite smoke. The
final evaluated runtime source is `ac9e83c`; subsequent edits only record evidence.

**Draft qualification remains red. No overall reliability or nonregression claim
is established.** The planned unchanged confirmation runs were replaced by
concrete repairs after new counterexamples appeared. Each source/provider has one
run, with no fixed random seed. Failed intermediate candidates are retained.

## Failure modes and what changed

The [failure-mode ledger](failure-modes.json) records 13 distinct observations,
repair commits where applicable, and boundaries that each repair must preserve.

- Round 1: Gemini's earlier repeated invalid-enum loop does not recur in Rust
  repair, but its TUI case spends successive turns discovering the two missing
  exact command calls, then reaches the cap without a final report. Self-fix also
  verifies without reporting. GPT exhausts its TUI budget on schema, target and
  Observer-response construction.
- Round 2: Gemini finishes TUI construction and reporting but fails only the
  frozen `patch_file` criterion; Mistral has the same tool-choice-only TUI failure.
  Mistral self-fix hits a false dependency on a described missing file. Gemini's
  create-file case follows the runtime's lowercased `grep -fx` suggestion and is
  repeatedly blocked because the configured command actually uses `grep -Fx`.
  Damaged resume adds a Git diagnostic in a non-Git fixture after a test failure.
- Round 3: Mistral's valid four-criterion plan is rejected as oversized because
  `exit_code == 0).` is parsed as an extra item. GPT finishes damaged resume with
  `write_file` and fails only required `patch_file` usage; its TUI artifact is
  malformed JSON. Gemini TUI omits required reflection after an overwrite refusal;
  self-fix verifies but reaches its cap without completed reporting.
- Round 4: Mistral passes all six. GPT's TUI artifact is parseable but its generated
  `coder_system_contains` assertion is false; required checks and handoff remain
  incomplete. Gemini TUI changes its failed patch's search/replacement, but the
  reflection/action guard still treats it as the same action by tool and path.
  Gemini self-fix updates source and docs, then fails to finish the eval follow-up
  and verification before the cap.

The last reflection/action collision is an **open harness defect**, not evidence
that the revised patch would have been correct. Its argument fingerprints and
source boundary are retained for the next repair. Structured artifact repair,
protocol-block omission and coordination of diagnosis with follow-up work also
remain open. Passing an automatic test does not establish task completion.

The frozen required `patch_file` checks are never waived. An otherwise completed
`write_file` repair is reported separately from a broken artifact; no
fixture-specific tool coercion is introduced to improve scores.

All parseable replay artifacts retain their original case. Round 3 GPT is
unparseable, so preservation is unknown. Round 3 and 4 Gemini retain only the
original case; preserving it does not mean the requested regression exists.
This supplementary audit does not alter the frozen evaluation criteria.
