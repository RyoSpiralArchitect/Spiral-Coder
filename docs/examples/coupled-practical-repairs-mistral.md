# Small repairs with Mistral Coder + Observer — PR #42 archive

Repair an environment-style boolean parser or a JSONL event summary, update its
usage notes, and finish with a verified before/after report. These two small
Python tasks exercise the complete Coder + Observer loop without third-party
Python dependencies.

This is the **2026-10-29 archive of [PR #42](https://github.com/RyoSpiralArchitect/Spiral-Coder/pull/42)**,
merged as `728014ff87b4db683073759a8939e1bed1806dcc`. It records that PR's final
frozen wave, not the performance of later runtimes. The [current practice-suite
guide](../../benchmarks/examples-v1/README.md) is the entry point for new work.

## The two tasks

| Task | Required edits | Visible original tests | Archived Coder / Observer requests | Strict result |
| --- | --- | ---: | ---: | --- |
| `env-bool` | `settings.py`, `docs/settings.md` | 10 | 11 / 3 | qualified |
| `event-summary` | `summary.py`, `docs/summary.md` | 10 | 11 / 3 | qualified |

Settings must recognize explicit true/false tokens, ignore case and surrounding
whitespace, use the supplied default for absent or empty values, and reject
unknown strings. JSONL must count three exact event levels, skip blank lines,
and report invalid records using their physical line number. Each task also
requires documentation examples, the exact test command and an authored report.

The final wave used **28 wire requests**, with no transport credits. Qualification
requires native source/docs edits, a failing baseline before editing, a successful
exact check after the final edit, independent original tests, an accepted report,
matched role configuration and strict file scope. Both roles belong to one task.

## Frozen setup

| Setting | Archived value |
| --- | --- |
| Runtime and driver source | `3aa0390ee1ce48cf9c7d63504447407976c14020` |
| Coder / Observer | `mistral-large-4` / `mistral-large-4` |
| Prompt revision | 2 |
| Task budget | 24 requests per task; 6 reserved for Observer |
| Transport repair ceiling | 4 per task; none used in this final wave |
| Coder slice | At most 8 requests |
| Time ceilings | 90 seconds per request; 600 seconds per task |
| Output ceilings | Coder 32,768 tokens; Observer 8,192 tokens |

[The archive index](archives/pr42.json) pins the merged source, task inputs and
original public receipts with SHA-256 digests and immutable GitHub links. The
recorded executable has its own hash in the executor receipt. Rebuilding the
same source does not promise an identical binary across machines or toolchains.

## Reproduce the task setup

From a clone containing the historical commit, use unused paths. Preparation
makes no provider calls; it checks the broken baselines and reference repairs,
then freezes the runtime, driver, tasks, controls, roles and budgets.

```sh
git worktree add --detach /tmp/spiral-pr42-source 3aa0390ee1ce48cf9c7d63504447407976c14020
cd /tmp/spiral-pr42-source
cargo build --locked --target-dir /tmp/spiral-pr42-build
python3 -B -S scripts/run_coupled_bench.py prepare \
  --suite benchmarks/examples-v1/suite.json \
  --matrix benchmarks/examples-v1/matrix-mistral.json \
  --binary /tmp/spiral-pr42-build/debug/spiral-coder \
  --runtime-commit 3aa0390ee1ce48cf9c7d63504447407976c14020 \
  --out /tmp/spiral-pr42-replay
```

With `MISTRAL_API_KEY` already supplied through the environment, these commands
run the pair and assess it against the frozen original tests:

```sh
python3 -B -S /tmp/spiral-pr42-replay/driver/run_coupled_bench.py run \
  --out /tmp/spiral-pr42-replay --profile large-large
python3 -B -S /tmp/spiral-pr42-replay/driver/run_coupled_bench.py summarize \
  --out /tmp/spiral-pr42-replay
```

The two-task maximum is 56 wire requests. Each episode runs once; keep failures
and incomplete results. For a new intervention, prepare another fresh directory.
A new provider run is a new observation and may differ from this archived result.
For current-runtime practice, follow the current suite guide instead of checking
out the historical source.

## Inspect the archived evidence

- [Final readout](../evals/2026-10-29-diagnostic-actions/README.md), [frozen plan](../evals/2026-10-29-diagnostic-actions/plan.json) and [executor pin](../evals/2026-10-29-diagnostic-actions/executor.json).
- [Full results](../evals/2026-10-29-diagnostic-actions/results.json), [baseline/reference controls](../evals/2026-10-29-diagnostic-actions/preflight.json) and [manual review](../evals/2026-10-29-diagnostic-actions/review.json).
- Candidate patches: [settings](../evals/2026-10-29-diagnostic-actions/large-large-env-bool.patch) and [JSONL](../evals/2026-10-29-diagnostic-actions/large-large-event-summary.patch).
- [Earlier interrupted and failed attempts](../evals/2026-10-28-practical-examples/README.md) remain separate. The final two successes do not replace them.

The links above are convenient checkout-relative views; the archive index names
the exact immutable versions. The generated candidates remain evaluation artifacts,
not production source. The archived runtime validation was 981 Rust tests plus
one integration test (one ignored), 91 Python tests, 85 Web/npm tests, 13 TUI
replays, 14 repo-map cases and server/UI smoke. Those counts describe that source
revision, not this documentation change or future versions.

## Limits preserved in the archive

The frozen tasks and tests are visible development fixtures. Two qualifications
are not a general model success rate or a controlled causal result.

The original final reports also had shortcomings that qualification did not
grade: JSONL described the empty-input all-zero dictionary as an always-returned
value, while settings supplied an unsupported failure count of nine. The baseline
control has fourteen failure events, including subtests. Source behavior and
original tests passed, but those incidental claims were inaccurate. The original
reports, plan refusals and earlier failures stay unchanged in their receipts.
