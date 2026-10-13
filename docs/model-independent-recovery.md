# Repair feedback across providers

The weak-model trajectories in [the October 9 evaluation](evals/2026-10-09-plan-replay/README.md)
expose shared harness problems. A failure observed with one provider is a
counterexample to a runtime assumption, not evidence that only that provider
needs the fix. Keep transport failures, task failures, and evaluator failures
separate when deciding what to change.

| Observed failure | General problem | Source boundary and response |
|---|---|---|
| A preliminary replay succeeds, then the intended edit is blocked as already fixed | A check's effect is confused with a mutation | `exec_classification/replay.rs` recognizes actual model-free replay execution using the CLI grammar. Recovery, mutation timestamps, edit-failure memory, and resumed proof share the classification. |
| Successive turns repair fields individually or repeat an invalid enum/reference | Validation exposes one defect per expensive model turn without the accepted vocabulary | `protocol_validation.rs` reports independent plan/think/reflection/impact defects together, exact enum choices and bounded current plan references. Accepted inputs and proof requirements stay the same. |
| A missing check value remains unresolved, or multiple bad check kinds consume successive repair turns | Diagnostic truncation removes schema context and validation reveals independent defects serially | `tui_replay/diagnostics.rs` lists up to four invalid check positions together, one matching enum shape and compact allowed kinds inside the existing automatic-test diagnostic budget. Extra positions are explicitly counted as omitted. Input identity precedes the long schema error. No invalid check is accepted or rewritten. |
| A refuted `path exists in the repo` claim blocks adding that path as text to another file | Natural-language overlap is mistaken for an actual tool dependency | `assumption_gate.rs` recognizes bounded literal existence claims. The missing path still blocks patching that path, while registry edits remain independent. A fresh matching read/write can confirm the claim; cached evidence cannot. |
| Repeated reflection/impact responses contain textual pseudo-calls but no native function call | An early gate hides the feedback needed to repair the transport format | `provider_turn.rs` supplies native-call guidance at missing/invalid-block and call-count rejection boundaries. It neither fabricates calls nor bypasses completion or reflection requirements. |
| A model obeys `grep -fx` suggested by the gate, which then refuses verification | A diagnostic signature was rendered as an executable command | Suggested configured checks retain exact case and quoted whitespace. Round-trip coverage checks that the gate accepts its own suggested command. |
| After a combined check, successive `done` attempts reveal one missing exact check at a time | The model sees only part of the remaining proof obligations | `benchmark_proof` exposes all pending checks before each turn and at rejected completion. Existing identity, mutation and success requirements remain authoritative. |
| A valid JSON replay fails semantically or before selecting a target, then recovery falls back to unrelated diagnostics | The report loses input identity and the meaning of its failed assertion | Semantic reports and early input errors name their spec and explain Observer/check surfaces. Early errors put a workspace-relative input path before the cause so long workspace prefixes cannot consume the diagnostic limit. Failure focus retains this through compaction and resume. Empty selections also report the requested filter and bounded existing case IDs without executing checks. |
| A broad rewrite changes an existing replay case, or an exact edit anchor repeatedly misses | Edit scope and preservation are not explicit enough | Still open. A passing parser does not prove preservation of existing behavior. Keep the artifact audit separate from frozen checks; do not synthesize unrequested assertion changes. |
| A revised patch to the same file is rejected as an unchanged action | Reflection action identity discards the edit payload | `reflection_action.rs` compares complete execution-relevant arguments, preserving path/command case, digits and payload bytes. Identical retries remain blocked despite changed call IDs or JSON formatting. Revised arguments grant no diagnosis, repair or verification credit. |
| Follow-up work continues after an earlier successful check, then the requested handoff is missing | Work completion, current proof, and final reporting are distinct obligations | Existing `benchmark_proof`, `recovery_focus`, `exec_proof`, and `final_handoff` retain these gates. They reject incomplete work, but do not yet guarantee productive progress under small iteration caps. |
| Observer treats a rejected edit or failed check as completed follow-up work | An attempted action is mistaken for positive execution evidence | `observer/detector.rs` correlates preceding native calls once and uses shared positive result headers. `exec_classification/observed.rs` checks actual command grammar and expected spec. Unconfirmed display edits request a read; they do not create a mutation anchor. |
| Observer invents a tool for the exact remaining check, or forwards contradictory display text | Next-action vocabulary and delivery are detached from real tool contracts | `events/observer_assist.rs` supplies the actual catalog and `edit`/`verify` kinds. `suggestion/action.rs` checks kind/tool/argument shape; both forwarded messages use the same primary action. Missing or invalid tools are withheld, not guessed. |
| A valid primary read is lost because optional display text is an object | Presentation metadata gates the actionable contract | A non-string `quickest_check` is ignored without converting it to arguments. Captured response replay verifies recovery; invalid JSON and invalid actionable fields remain rejected. |
| A failed check requires diagnosis while learned read-loop rules demand an edit | Fresh failure obligations conflict with older progress pressure | `recovery.rs` owns diagnostic priority; repeated-observation policies and learned prompt overlays yield during diagnosis. Observation counts restart only after correlated successful mutation, preserving rejection and proof boundaries. |
| An otherwise useful Observer answer omits its actionable tool | Invalid advisory output needs an explicit bounded correction path | `events/observer_request.rs` asks the model once using the original packet, schemas and quoted failure. Missing tools are never inferred, low confidence is not retried for promotion, and stale request context expires. |
| Replay reports hint failure without its actual action-contract cause | Schema feedback is detached from the consumer | Failed checks now carry the shared parser/action validator's reason. Actual enum errors include `edit`/`verify`; no duplicate kind list, relaxed check or artifact rewrite is used. |

The policy is to make the next correction actionable without claiming it already
happened. Validation diagnostics do not fill in reasoning fields, change tool
arguments, turn text into a tool call, or create execution evidence. Provider
continuation metadata remains opaque and the original call stays intact.

## Boundaries to preserve

- Running another replay case grants only the ordinary behavioral verification
  result of that execution. It cannot satisfy a different exact required command,
  clear a failed edit, or mark a pending focused repair as written. A subsequent
  edit still invalidates earlier proof. Restore must produce the same state.
- Recognition uses the actual subcommand and valid arguments. `eval`, help,
  unknown flags, command names inside option values, shell writes, and status
  masking do not become replay verification.
- Protocol defects are aggregated without accepting a missing field, an invalid
  step, a mismatched tool, an unrelated exec prefix, or an oversized plan.
- Existence-claim recognition covers explicit English literal forms, not arbitrary
  prose or compound claims. Bounded affirmative file descriptions share the same
  exact dependencies. Nonexistence assertions and broader semantic
  assumptions keep their prior handling. Provider selection does not change
  dependency matching or early missing-call guidance.
- Replay schema examples are examples, not replacements for intended assertions.
  Invalid specs remain invalid; diagnostics do not write the artifact.
- The frozen suite includes a required `patch_file` call even where the task
  prompt does not name that tool. An otherwise successful `write_file` repair
  can fail that criterion. Preserve the criterion and report this distinction;
  do not infer a new user instruction or add fixture-specific runtime coercion
  just to improve the score.

Native runtime recovery and TUI Observer handoffs are covered here. Web
parser/validator parity remains separate work. Small live-suite totals cannot establish general
reliability, provider rankings, or nonregression.

The [four-cycle record](evals/2026-10-13-recovery-cycles/README.md) and the
[three further reflection/replay-input cycles](evals/2026-10-13-reflection-repair/README.md)
retain failures and deterministic counterexamples from every candidate. The
latter also records a new unsupported existence-claim wording, conflicts
between follow-up diagnosis and progress pressure, and exact-command extensions
that fail current-proof requirements. Strict JSON preservation and equivalent
optional defaults are distinguished in the artifact audit. A more actionable
rejection is not itself evidence of model task completion.

The [Observer recovery record](evals/2026-10-13-observer-recovery/README.md)
separates a new six-case primary-action panel from the unchanged Coder suite.
All initial measurement/HTTP failures, both runtime candidates and captured
response replays are retained. The final missing-tool response is withheld,
while Coder still exhibits conflicting diagnosis/mutation guidance, unfinished
follow-ups and missing handoffs. Neither parsed advice nor a passing Observer
panel establishes completed Coder work.


The [bounded-advisory and recovery-precedence record](evals/2026-10-14-advisory-recovery/README.md)
retains two further runtime candidates, 36 fixed Coder cases, 36 initial Observer
calls and 18 separately scored repairs of captured invalid responses. Final
Coder results are 5/6 for all three weak models; the TUI case still exposes
budget-end reporting, target/assertion construction and exact-proof scheduling.
Observer correction delivery is not Coder completion. All fixtures, caps and
acceptance checks remain frozen. The PR remained Draft at evaluation time.
The subsequent [fresh review](reviews/2026-10-14-pr16-fork-review.md) records
the code-review outcome and merge decision; it does not promote these live
results to a general reliability claim.
