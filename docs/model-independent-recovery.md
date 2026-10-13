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
| A missing check value remains unresolved after rereading the spec | Diagnostic truncation removes the repair-relevant schema | `tui_replay/diagnostics.rs` puts the failing case/check index and matching enum shape before the general examples. Tests exercise automatic-test rendering and old-result pruning. |
| A refuted `path exists in the repo` claim blocks adding that path as text to another file | Natural-language overlap is mistaken for an actual tool dependency | `assumption_gate.rs` recognizes bounded literal existence claims. The missing path still blocks patching that path, while registry edits remain independent. A fresh matching read/write can confirm the claim; cached evidence cannot. |
| Repeated reflection/impact responses contain textual pseudo-calls but no native function call | An early gate hides the feedback needed to repair the transport format | `provider_turn.rs` supplies native-call guidance at missing/invalid-block and call-count rejection boundaries. It neither fabricates calls nor bypasses completion or reflection requirements. |
| A model obeys `grep -fx` suggested by the gate, which then refuses verification | A diagnostic signature was rendered as an executable command | Suggested configured checks retain exact case and quoted whitespace. Round-trip coverage checks that the gate accepts its own suggested command. |
| After a combined check, successive `done` attempts reveal one missing exact check at a time | The model sees only part of the remaining proof obligations | `benchmark_proof` exposes all pending checks before each turn and at rejected completion. Existing identity, mutation and success requirements remain authoritative. |
| A valid JSON replay fails semantically and recovery falls back to unrelated diagnostics | The report loses input identity and the meaning of its failed assertion | Semantic reports name their input spec and explain Observer/check surfaces; failure focus retains this through compaction and resume. |
| A broad rewrite changes an existing replay case, or an exact edit anchor repeatedly misses | Edit scope and preservation are not explicit enough | Still open. A passing parser does not prove preservation of existing behavior. Keep the artifact audit separate from frozen checks; do not synthesize unrequested assertion changes. |
| Follow-up work continues after an earlier successful check, then the requested handoff is missing | Work completion, current proof, and final reporting are distinct obligations | Existing `benchmark_proof`, `recovery_focus`, `exec_proof`, and `final_handoff` retain these gates. They reject incomplete work, but do not yet guarantee productive progress under small iteration caps. |

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

Native protocol validation is the scope of this change. Web parser/validator
parity remains separate work. Small live-suite totals cannot establish general
reliability, provider rankings, or nonregression.

The [four-cycle record](evals/2026-10-13-recovery-cycles/README.md) retains failures,
deterministic counterexamples and the unchanged final-candidate repeat. A more
actionable rejection is not itself evidence of model task completion.
