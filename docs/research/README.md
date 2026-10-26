# Spiral-Coder research & development

[Documentation](../README.md) · [Use Spiral-Coder](../getting-started.md)

The research unit is **Coder + Observer together**. We study how the execution
harness helps that pair diagnose failures, preserve the original task, make a
useful edit and verify it. Separate contexts, tool execution and an authored
"done" message are not themselves evidence of a correct repair.

## Choose a starting point

| Goal | Read |
| --- | --- |
| Understand the runtime | [Architecture](../runtime-architecture.md), [state ownership](../state-schema.md) |
| Follow a task across Coder and Observer | [Coupled work loop](../coupled-work-loop.md), [shared work progress](../observer-work-progress.md) |
| Work on recovery and tool behavior | [Model-independent recovery](../model-independent-recovery.md), [follow-up edits](../followup-edit-safety.md), [file tools](../file-tools.md) |
| Understand what counts as completion | [Task requirements](../task-requirements.md), [evaluation contract](../evaluation.md) |
| Run bounded cross-family self-dogfood | [Runner and frozen contracts](../self-dogfood.md), [task catalog](../../benchmarks/self-dogfood-v1/README.md) |
| Reproduce ordinary runtime examples | [Benchmark examples](../examples/README.md) |
| Contribute a change | [Build and checks](../usage.md#build-and-contribute), [contributor rules](../../AGENTS.md) |

## Experiments and open questions

The [visible-regression readout](../evals/2026-10-26-visible-regression/README.md)
records two episodes and 42 requests with a protected, executable failing test
and the original six independent controls. Neither pair repaired the defect;
both stayed natively incomplete. It changes the task presentation and prepared
baseline, so it is reported separately from a runtime-only comparison. Post-run
review also found shared Cargo cache ambiguity; the readout preserves that
limitation and the runner now invalidates package artifacts between copies.

The preceding [cross-family readout](../evals/2026-10-25-self-dogfood/followup-r5.md)
records six episodes and 131 provider requests across its full series. No repair
candidate qualified. The latest Luna→Argon run reached native completion but
left the behavioral defect; Argon→Large 4 stopped after malformed Observer
output. Those failures motivate the visible reproducer, no-op edit handling,
and retaining useful diagnosis when an action cannot be executed.

Other entry points:

- [Model capacity comparison](../evals/2026-10-23-capacity/README.md)
- [Small-model repair iterations](../evals/2026-10-24-small-model-repair/README.md)
- [Format recovery](../evals/2026-10-25-format-recovery/README.md)
- [Historical benchmark milestones](historical-milestones.md)

## Keep claims tied to evidence

Record native completion, actual mutations, executed checks, independent
qualification, file scope and request accounting separately. Freeze the task,
driver, limits and baseline before calls. Retain unsuccessful and interrupted
attempts. A passing local suite establishes its covered regressions, not a
general repair success rate; a model-generated candidate still needs review
before adoption. See the [evaluation contract](../evaluation.md) and
[self-dogfood boundary](../self-dogfood.md#assessment-and-evidence).

The project began as a two-day Mistral hackathon experiment. The
[earlier full overview](https://github.com/RyoSpiralArchitect/Spiral-Coder/blob/394a339fa3ab37e4ffa40f9257489199f8cef731/README.md)
retains the former long-form README for provenance; current usage starts in the
[getting-started guide](../getting-started.md).
