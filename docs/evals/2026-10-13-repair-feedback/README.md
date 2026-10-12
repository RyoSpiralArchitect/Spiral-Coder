# Cross-provider repair feedback — 2026-10-13

The [source audit](../../model-independent-recovery.md) separates repaired
harness defects from remaining task-quality failures:

1. Native plan/think validation reports independent defects together with the
   complete field format, rather than charging a model turn for every missing
   field. Validity constraints and original tool calls remain unchanged.
2. Model-free `tui-replay` commands are recognized using the real CLI grammar.
   A preliminary check no longer invents a mutation, clears edit-failure memory,
   or advances an unperformed focused repair to verification.
3. Replay schema errors prioritize the failing case/check index and the matching
   serialized check shape. The required field survives automatic-test digesting
   and old-result pruning. Diagnostics do not edit the artifact or its assertions.
4. Explicit literal existence claims such as `PATH exists in the repo` use the
   same exact dependency handling as `file exists at PATH`. A refuted path can be
   added as text to another file without treating that as patching a missing file.
5. Zero-call rejections at reflection/impact call-count checks receive native function-call
   guidance before they can loop at an earlier gate. No pseudo-call is executed.

## Frozen comparison

The [six-case spec](../2026-10-03-restart/runtime_eval.json), all 33 fixture hashes,
checks, model identities, and iteration caps are unchanged. These are fresh runs
in separate copied workspaces. Each baseline precedes its candidate; providers
may run concurrently. Earlier runs and failed candidates in
[the October 9 record](../2026-10-09-plan-replay/README.md) are retained.

| Phase | Source commit | Binary SHA-256 |
|---|---|---|
| Baseline | `78938559db780d7c83a8b847aee42eea58009f4e` | `6526325be286d305f6f47065e7461193b990ae5ff1dc104ef6c405e088d23fa7` |
| First candidate | `a441182b6db5035b030db1ff49ac9b3d60075705` | `6bafe41ecf74443ace054ca97a66ce53c38b7bbec101a24d99a78294ae9db114` |
| Revised candidate | `9e712ffecace6b201352e33a3d78b28fe7b21bb5` | `e03fa6da115cbd6ded58de659588b1f20269b9d03511c7669278aa43bfebe186` |

Spec SHA-256: `2edcc57bb2291ed10d0130b9a0997e018e52b0593d0fa576e59bc064a66c5db0`.
Conditions: temperature 0, 2,048 output tokens, 90-second request timeout,
900-second outer suite timeout, case iteration caps 14/14/10/8/12/18,
per-copied-workspace Cargo targets. All runs use the same credential-safe
runner hash `72d1ba53a412720a4295f5119dfe4cb1dda314a0b93628a9a9ec399edf3ac3ac`.
Credentials are literal assignments loaded into the selected child environment;
shell startup files are not sourced.

## Deterministic verification

The three initial regressions failed against the previous implementation:
incomplete protocol feedback omitted other missing fields, preliminary replay
was classified as an action, and compaction cut off the failing check's `value`
example. The corrected implementation passes those regressions.

`bash scripts/check-local.sh` passes: 651 Rust unit tests (1 ignored),
1 integration test, TUI replay 3/3, Python 12, Web 53, repo-map 14/14,
JavaScript/shell syntax, and Lite asset/status/git smoke. Ten new regressions
cover the repaired paths, malformed/misleading command negatives, unchanged
protocol rejection rules, and preservation of failed edits, exact required
checks, and focused repair state across resume. Existing tests that expected
replay to be an unknown action now assert its recognized verification behavior;
unknown custom commands retain their conservative classification.
The first candidate passed 649 unit tests; the revised candidate adds exact
dependency/fresh-evidence coverage for model-authored existence claims and a
mock HTTP loop test proving that early zero-call rejection sends native-call
guidance without executing the response's textual XML command.

These checks establish the stated implementation behavior. Live task completion
and repeated reliability require separate evidence.

## First candidate and the concrete reasons for revising it

The [first receipts](receipts-first-candidate.json) preserve every check and raw
artifact hash. Baseline scores were Mistral 6/6, GPT 5/6, Gemini 4/6; the first
candidate scored 5/6, 5/6, 3/6. These results do not show a quality improvement.

Mistral completes the TUI case with both required commands, the automatic test,
and the final report. Its self-fix case instead stalls at the assumption ledger:
`src/tui/agent/session_bridge.rs exists in the repo` is refuted, then its path
literal is mistaken for a dependency while editing the separate
`src/tui/agent/followup_requirements.rs`. The previous exact-path guard recognized
only the canonical `file exists at` wording. The revised parser treats bounded
literal existence forms consistently, preserves the missing-file patch rejection,
and requires fresh evidence to confirm the refuted file.

GPT fails TUI construction. After exact-anchor failures and a malformed JSON
append, it removes the added case to repair syntax. The final file matches the
original fixture but lacks the requested regression. Restoring valid syntax is
not task completion; the frozen checks correctly remain red.

Gemini repairs the damaged session after a single aggregate think-field rejection,
but fails TUI construction and spends the self-fix budget on protocol/inspection.
Benchmark resume obtains fresh required execution proof and writes
`verified-current` to the receipt, yet never produces the required final handoff.
Trace events show one invalid impact followed by repeated zero-call impact/
reflection rejections. Those early exits prevent the ordinary unfinished-turn
native-call explanation from being reached. The revised candidate supplies the
same explanation at those existing rejection boundaries, without bypassing them.

All first-candidate replay artifacts preserve the original case. GPT ends with
only that original case; Mistral and Gemini have two cases. This structural audit
does not certify new-case correctness or change the frozen pass criteria.

## Final results and remaining limits

| Model | Fresh baseline | First candidate | Revised candidate |
|---|---:|---:|---:|
| `mistral-small-2603` | 6/6 | 5/6 | 6/6 |
| `gpt-4.1-mini` | 5/6 | 5/6 | 5/6 |
| `gemini-3.5-flash-lite` | 4/6 | 3/6 | 1/6 |

**The frozen suite remains unqualified for promotion. Gemini's observed score
decreased; no overall quality improvement or nonregression is established.**
There is one run per source/provider, no fixed random seed, and no repeated
qualification of one immutable candidate. Do not interpret these counts as
provider rankings or attribute every trajectory difference to a particular fix.

The [final receipts](receipts.json) include all nine runs, all failed checks,
source/binary/spec/fixture identities, raw artifact hashes, retained gate-message
counts, and pre-tool rejection event counts. They link the preserved first
receipts by hash. No run timed out or retried a provider request; all reported
transport error counts are zero. Raw transcripts, signature values, credentials,
and machine-local paths are not published.

The revised Mistral run passes all six cases. GPT still fails TUI repair after
syntax, target-selection, and exact-anchor failures; its final failing assertion
expects the case ID in the Coder system prompt. All revised replay artifacts are
JSON-parseable, contain two cases, and preserve the original case object. That
preservation check is supplementary and does not certify the generated case.

Gemini's five failed cases are distinct:

- Rust repair produces the correct edit and a passing automatic test, but eight
  invalid-reflection rejections (`strategy_change`) prevent completed reporting.
- Damaged-session repair completes and passes the automatic test using
  `write_file`; only the frozen required `patch_file` usage check fails.
- Benchmark resume appends `&& cat verification_receipt.txt` to the required
  receipt-writing check. That extended command lacks the exact approved check's
  verification authority and invalidates proof as an action. Impact-field
  rejections then prevent complete fresh verification and handoff.
- TUI construction reaches a passing automatic test, but neither exact required
  command nor final completion is recorded within the fixed cap.
- Self-fix performs the primary change and docs work, but leaves the runtime-eval
  follow-up and automatic check incomplete; missing impact blocks also reject
  later text-only responses.

The next common problems are repair scope for structured artifacts, actionable
enum/reference diagnostics beyond plan/think, and coordinating remaining task
obligations with reflection/impact and final-handoff gates. Native-call guidance
at the call-count check does not repair an invalid block rejected earlier in the
pipeline. Existing evidence and completion gates remain enforced; the new
deterministic tests should not be presented as proof of live reliability.
