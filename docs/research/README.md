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
| Practice a small complete coding loop | [Boolean settings and JSONL examples](../../benchmarks/examples-v1/README.md) |
| Contribute a change | [Build and checks](../usage.md#build-and-contribute), [contributor rules](../../AGENTS.md) |

## Experiments and open questions

The [plan and report follow-up](../evals/2026-10-29-plan-report/README.md)
adds admitted-plan anchors and native test-count facts shared by both roles.
The adaptive runs retain a checker false positive and its correction, plus an
Observer completion-contract repair that exhausted its output limit.

The [diagnostic-action follow-up](../evals/2026-10-29-diagnostic-actions/README.md)
uses the current native failure location to constrain Coder's next offered tools,
then restores work tools after an actual read. Mistral Large 4 in both roles
qualifies both small tasks with 14 requests each. Frozen qualification, remaining
plan refusals and inaccuracies in incidental report prose are recorded separately.

The [practical examples readout](../evals/2026-10-28-practical-examples/README.md)
starts with short source-and-documentation tasks. After OpenAI quota exhaustion,
Mistral Large 4 fills both roles. A boolean parser repair reaches strict
qualification in one adaptive follow-up; the initial JSONL attempt stopped before
its documentation update. Frozen failures, report-contract alignment, quota
retry repairs and an offline report-assessment correction are recorded separately.

The [budget-rhythm readout](../evals/2026-10-27-budget-rhythm/README.md) implements
a bounded transport-rejection pool, protected Observer capacity, truthful blocked
reports and development milestones under a new explicit prompt. Four fixed
Luna/Large 4 episodes spend 77 requests; none qualifies. The new cadence runs,
but both treatment episodes stop on Mistral body interruptions. One candidate
has only a no-op edit receipt; the other regresses a later-failure control and
then fails to compile. No repair-performance improvement is established.

The [failure-routing readout](../evals/2026-10-26-failure-routing/README.md)
uses a Gemini-free Luna/large-Mistral matrix in both directions, with separate
baseline and candidate attempts: 101 requests, no independently qualified repair.
The runtime retains distinct failed checks through handoff and actual context
pruning; seven malformed Mistral opening prefixes recover live. A later Observer
stream interruption and blocked truthful incomplete reports identify the next
control-flow questions.

The [Observer-return readout](../evals/2026-10-26-observer-return/README.md)
compares a cache-corrected baseline with repeated Coder slices, fresh-failure
handoffs and narrow JSON opener recovery: four attempts, 77 requests. Luna's
new trajectory records three failure handoffs and passes the visible test, but
its patch regresses later-failure controls and does not qualify. The new Argon
episode stops at a provider HTTP 402 before Observer; two retained malformed
Large 4 responses recover in offline replay. Freshness across multiple failed
checks and overbroad candidate state transitions remain open questions.

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
