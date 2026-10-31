# Small practical Coder + Observer examples

Start with a task small enough to finish a useful loop: read a short source file,
reproduce a failure, repair it, update its usage notes and verify the result.
These two authored practice fixtures need only Python's standard library.

| Example | Work | Original tests |
| --- | --- | ---: |
| `env-bool` | Parse explicit boolean environment settings, defaults and invalid values | 10 |
| `event-summary` | Count JSONL event levels, skip blank lines and report invalid physical line numbers | 10 |

Each task changes one source file and one documentation file. All requirements
and tests are visible. Independent assessment restores the original tests in a
fresh directory and copies only those two candidate files. Exact reference source
equality is diagnostic, not required. Documentation tests check the examples and
command explicitly requested by the task; they do not certify general writing
quality. The reference implementations and preflight receipts stay outside the
model workspace.

This is an entry-level **development suite**, not a held-out benchmark, a claim
about full repository maintenance or a replacement for failed harder episodes.
The [coding matrix](../coding-v1/README.md) and
[self-dogfood tasks](../self-dogfood-v1/README.md) retain their original contracts.

## Run a bounded pair

Build a current source runtime, then prepare a fresh directory:

```sh
cargo build --locked
python3 -B -S scripts/run_coupled_bench.py prepare \
  --suite benchmarks/examples-v1/suite.json \
  --matrix benchmarks/examples-v1/matrix-mistral.json \
  --binary target/debug/spiral-coder --runtime-commit "$(git rev-parse HEAD)" \
  --out /tmp/spiral-examples-r1
```

Preparation makes no provider calls. It verifies broken baselines and known
repairs, then freezes the driver, runtime, fixtures, checks, roles and budgets.
Set `MISTRAL_API_KEY` through your environment, then use the frozen driver:

```sh
python3 -B -S /tmp/spiral-examples-r1/driver/run_coupled_bench.py run \
  --out /tmp/spiral-examples-r1 --profile large-large
python3 -B -S /tmp/spiral-examples-r1/driver/run_coupled_bench.py summarize \
  --out /tmp/spiral-examples-r1
```

The quick-start matrix uses `mistral-large-4` for both Coder and Observer, in
separate role contexts. To use GPT in both roles, prepare a fresh output directory
with `--matrix benchmarks/examples-v1/matrix-openai.json`, set `OPENAI_API_KEY`
in the environment and run the frozen driver with `--profile luna-luna`. Both
roles use `gpt-5.6-luna` with `reasoning_effort=none`.
For cross-family comparison, prepare a separate output
with `matrix.json`, which pairs `gpt-5.6-luna` Coder with `mistral-large-4` Observer
(`luna-large`), then reverses them (`large-luna`). That matrix needs both
`OPENAI_API_KEY` and `MISTRAL_API_KEY`. Both roles form one research unit. Each episode allows
24 task requests (6 reserved for Observer), at most 4 transport repairs, an
8-request Coder slice ceiling, 90 seconds per request and 600 seconds per episode.
Each same-model pair has a two-episode maximum of **56 wire requests**. The four
cross-family episodes allow up to **112**. Coder output cap is 32,768,
Observer 8,192. Existing compatible-mode provider settings apply; missing usage
remains unknown and equal request ceilings do not mean equal compute or cost.

Select one task or direction with `--case` / `--profile` during preparation.
No automatic episode retry, replacement model or reuse of a started workspace
is allowed. Keep interrupted attempts in the denominator. Inspect completed
profiles before starting another; a new intervention needs a new frozen directory.

## What counts as success

Qualification requires all of: actual native file edits, the failing baseline
before editing, fresh exact successful checks after final edits, independent
original tests, an accepted native `done` with the requested before/after report,
both roles called, exact role configuration and bounded accounting. Strict
qualification also requires every changed file to be in the requested scope.
Python checks use `-B` so verification does not itself create bytecode files.

Matrix v2 adds explicit Observer settings and frozen per-case budgets. Native
receipts must match them; output-limited/empty responses cannot qualify for the
transport pool. Matrix v1 retains same-model defaults and its historical limits.
Results separate artifact behavior, native proof, authored report and file scope.

Prompt revision 2 explicitly requests **before/after test outcomes**, activating
the runtime's existing native outcome-row checks in `done.summary`. The first
revision asked for outcomes without naming tests in the final-answer clause;
the runtime accepted a loose report that the frozen benchmark rejected. The
follow-up used that same frozen report predicate; the current assessor also
rejects negated or example-only outcome mentions. A regression exercises both real prompt
files through the native report validator, including rejection of qualified
labels such as `Before (baseline): FAILED`.

## Try one manually

Prepare a separate directory with one case, enter its `episodes/<profile>/<case>/workspace`,
launch the frozen `runtime/spiral-coder tui` and paste the neighboring `task.txt`.
Use explicit Coder and Observer model options for your chosen pair. Manual
sessions are useful practice but do not manufacture the headless runner's native
completion receipt. Never mix manual edits into a declared automated episode.

## Recorded runs

The [GPT-pair readout](../../docs/evals/2026-10-31-gpt-pair/README.md) records
strict qualification on both tasks using `gpt-5.6-luna` in both roles. Settings
used 16 requests and JSONL used 22. Both candidate patches passed the original tests
in fresh workspaces. Each episode reached native verification and an accepted
report. The JSONL receipt also preserves two failed diff attempts.

The [Observer reconnect follow-up](../../docs/evals/2026-10-30-observer-reconnect/README.md)
adds one bounded native stream retry. Its same-pair wave spends 40 requests:
both artifacts pass original tests, settings reaches native completion, and
JSONL remains truthfully blocked after three Plan Gate verification refusals.
No live stream interrupted, so reconnect behavior is deterministic-test evidence.

The [plan and report follow-up](../../docs/evals/2026-10-29-plan-report/README.md)
retains admitted objectives during replanning and preserves native test counts
for both roles. Its live loops include corrected report-checker false refusals
and an incomplete Observer handoff, alongside the qualified examples.

The [diagnostic-action follow-up](../../docs/evals/2026-10-29-diagnostic-actions/README.md)
qualifies both tasks with Mistral Large 4 in both roles: 14 requests per task,
including source, docs, independent original tests and native completion. It also
records remaining plan refusals and inaccuracies in optional report prose.

The [first readout](../../docs/evals/2026-10-28-practical-examples/README.md) retains
the provider-quota interruption and both Mistral × Mistral tasks. An adaptive
`env-bool` follow-up strictly qualified with 21 requests after the report
obligation was made explicit. JSONL remained blocked before its documentation
update in that earlier wave. These are reproducible development examples, not a
general model success rate.
