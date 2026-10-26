# Recovery handoff with protected Observer capacity

This is a new prompt variant of the [visible regression](../recovery-handoff-visible/README.md).
The task baseline, visible fixtures, six independent probes, reference repair,
editable paths and 864-original-test floor are unchanged. `prompt.txt` adds the
exact single-test reproducer, later-failure invariants, required documentation
edit, and full native check after the final mutation. These instructions do not
replace any existing completion, scope, evidence or independent check.

The four specs use Luna 5.6 and Mistral Large 4 in both role directions:

| Spec prefix | Task requests | Transport rejection pool | Observer-only task capacity |
| --- | ---: | ---: | ---: |
| `baseline-` | 32 | 0 | 0 |
| No prefix | 32 | 4 | 8 |

Each pair has identical inputs except the two explicitly declared allocation
fields. The maximum is 32 wire attempts for baseline or 36 for the treatment.
All requests remain in the ledger. Only completed, wholly rejected native
argument or Observer envelope framing/type errors can use the transport pool.
HTTP failures, interrupted streams, semantic rejections and failed checks remain
task charges. Local deterministic JSON recovery adds no request.

Coder cannot borrow the eight Observer slots. The new runtime normally paces
three Coder requests per Observer intervention and yields earlier on a fresh
verification failure. Success, a blocked report or an interruption may end the
run before all reserved capacity is consumed. A blocked report retains unmet
obligations, receives one Observer diagnosis, and cannot qualify.

Use the [prepare/run/assess workflow](../../../docs/self-dogfood.md). Freeze the
runtime, spec and binary before calls; never substitute another attempt for a
failed cell. The [first four-episode comparison](../../../docs/evals/2026-10-27-budget-rhythm/README.md)
records the runtime/allocation treatment under this prompt. It is neither a
runtime-only comparison with older prompts nor an isolated causal test of
Observer cadence.

Development-only milestones report exact reproduction, measured behavior
change, visible pass, control non-regression, fresh full native check and
docs/receipts. Report every milestone alongside its observed/not-observed status;
a stage count cannot override binary qualification. Candidate patches remain
review proposals and are never adopted by this benchmark.
