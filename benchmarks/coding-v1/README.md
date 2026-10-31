# Coding capacity and harness development matrix

This suite runs the actual headless TUI Coder + Observer consumers, with the same
model for both roles. It is a three-case **development** suite for finding harness
failure modes. It is not held out, a broad coding leaderboard, or proof that a
model can maintain the full repository. The historical runtime/coupled panels
and their failures remain unchanged.

For shorter tasks with explicit cross-family Coder/Observer pairs, start with
the separate [practical examples suite](../examples-v1/README.md). The original
v1 matrix below keeps its same-model settings and request limits.

| Case | Task and independent check | Shared request cap |
| --- | --- | ---: |
| `large-file` | Repair punctuation after 600 preserved lines; frozen original tests and prefix preservation | 18 |
| `pilot-cli` | Repair `--case` selection in the real pilot script; preserve default/unknown-ID/no-overwrite behavior; update documentation | 24 |
| `replay-json` | Append one bounded-read replay to a large JSON document; preserve all old cases, nested extensions and order; document it | 24 |

The last two cases use source/test/replay snapshots from the `origin_commit` in
`suite.json`. The CLI snapshot has one deliberate selection regression. The JSON
fixture adds unknown nested data and requests a new case; its original four cases
already pass. These are repository-derived dogfood exercises, not full-repository
builds. Reference patches stay outside the model workspace. Candidate tests are
never used as the independent oracle: original tests are copied into a fresh
directory with only the requested candidate artifacts. Documentation checks
cover explicitly requested tokens, not general prose correctness.

## Repository maintenance with the practical-pair budget

After the two small practical repairs, use `suite-maintenance.json` to run the
repository-derived `pilot-cli` and `replay-json` tasks. They reuse the historical
workspaces, references, prompts and exact commands. The CLI case exercises
selection over a real 166-line runner. The replay case requires retaining four
existing cases and unknown metadata in a 1,101-line JSON file, adding exactly one
case, and running the frozen Spiral-Coder binary to verify all five.

```sh
cargo build --locked --bin spiral-coder
python3 -B -S scripts/run_coupled_bench.py prepare \
  --suite benchmarks/coding-v1/suite-maintenance.json \
  --matrix benchmarks/examples-v1/matrix-openai.json \
  --binary target/debug/spiral-coder --runtime-commit "$(git rev-parse HEAD)" \
  --out /tmp/spiral-maintenance-r1
python3 -B -S /tmp/spiral-maintenance-r1/driver/run_coupled_bench.py run \
  --out /tmp/spiral-maintenance-r1 --profile luna-luna
python3 -B -S /tmp/spiral-maintenance-r1/driver/run_coupled_bench.py summarize \
  --out /tmp/spiral-maintenance-r1
```

Start with a fresh output directory and provide `OPENAI_API_KEY` through the
environment for the live run. Coder and Observer each use `gpt-5.6-luna`.
Per episode, 24 task requests include six reserved for Observer. The four
transport repair slots bring the maximum to 28 wire requests per episode.
The Coder slice cap is eight. The two episodes together allow 56 wire requests.
The original-test, post-edit proof, report and file-scope gates all apply.
Replay preservation distinguishes JSON booleans from numbers throughout old
cases and unknown extensions. Native child replay reports are retained under
`run/replays` in the episode output.

The [first GPT maintenance readout](../../docs/evals/2026-10-31-repo-maintenance/README.md)
retains both attempts, a correct replay extension, the blocked CLI repair and a
separate correction to the report assessor. It includes a receipt-based
reproducer for the remaining continuation defect.
The [CLI continuity follow-up](../../docs/evals/2026-10-31-cli-continuity/README.md)
repairs that defect in deterministic replay and records one strictly qualified
CLI repair with the same GPT pair, task and request budget.

## Prepare without provider calls

Build a runtime containing PR #31's bounded reads and task proof first. The
runtime commit is supplied explicitly and its executable hash is retained.
The example uses the working tree's build; do not label an unrelated binary with
the current commit. Explicit model IDs bypass the picker, but still need matching request compatibility.

```sh
cargo build --locked
python3 -S scripts/run_coupled_bench.py prepare \
  --binary target/debug/spiral-coder --runtime-commit "$(git rev-parse HEAD)" \
  --out /tmp/spiral-capacity-r1 \
  --profile gemini-lite --profile gemini-argon
```

`prepare` checks each broken fixture and known repair offline, then freezes the
binary, tasks, fixture/reference files, matrix, driver and full workspace
inventories. A failing preflight is retained and prevents a run. Existing output
directories are never overwritten. Repeated `--case` / `--profile` options select
entries once in declared order; omit them to select the entire matrix.

## Run a declared model pair or provider panel

Set provider credentials through the environment before running. No secrets are
accepted in arguments, matrix files or receipts. The driver sends only the
selected provider's credential to the process. Use the frozen driver so later
edits cannot silently change the experiment.

```sh
python3 -S /tmp/spiral-capacity-r1/driver/run_coupled_bench.py run \
  --out /tmp/spiral-capacity-r1 --profile gemini-lite
python3 -S /tmp/spiral-capacity-r1/driver/run_coupled_bench.py run \
  --out /tmp/spiral-capacity-r1 --profile gemini-argon
python3 -S /tmp/spiral-capacity-r1/driver/run_coupled_bench.py summarize \
  --out /tmp/spiral-capacity-r1
```

The provided matrix retains weak baselines for all three providers and the
selected counterparts:

| Provider | Weak baseline | Selected counterpart |
| --- | --- | --- |
| Gemini | `gemini-3.5-flash-lite` | `gemini-4-argon` |
| OpenAI | `gpt-4.1-mini` | `gpt-5.6-luna` |
| Mistral | `mistral-small-2603` | `mistral-large-4` (public preview) |

Use a runtime containing PR #34's request compatibility as well as PR #31's
harness. Luna uses **reasoning none**, `max_completion_tokens` and no explicit
temperature on the current Chat Completions tool route. This is a compatible-mode
comparison, not a test of Luna's reasoning-enabled capacity; that needs Responses
transport. Large 4 uses `top_p: 1` with temperature zero. Both roles use the same
settings. Copy `matrix.json` before preparing a fresh output to change the panel.
The earlier four-profile results retain their original frozen matrix.

Every profile uses a 32,768-token output ceiling, a 90-second request timeout
and a 600-second episode wall cap, with the sampling exceptions above. The entire
six-profile matrix has at most **396 wire requests**; selecting only `pilot-cli`
uses at most **144**. Coder, Observer, repairs and native transport
attempts share each case's cap. The driver does not retry an episode or substitute
a different model. A new trial needs a fresh directory and predeclaration.

The output limit is higher than the earlier 2,048-token pilot because Argon uses
one limit for thinking plus output; Google recommends at least 32,768 for existing
apps with a smaller limit. Thinking remains at each provider's default except Luna's explicit none mode. Equal
request/output ceilings are **not equal compute, latency or price**; compare
returned usage as well, and keep missing usage unknown. See the
[Argon output-limit guide](https://ai.google.dev/gemini-api/docs/latest-model#setting-output-token-limits)
and [GPT-5.6 Luna](https://developers.openai.com/api/docs/models/gpt-5.6-luna).

## Read the separate outcomes

Each immutable `assessment-NNN.json` records:

- Runtime completion, independent artifact behavior, native pre-edit failure and
  fresh exact checks after the latest mutation.
- Authored report fields and acceptance, request roles/usage, Observer assists,
  pre-tool rejections, execution gate blocks and ordered tool actions that locate
  the divergence. A pre-tool rejection can consume a request without creating a
  native action; an empty action list does not imply no attempts were made.
- All changed paths, including ignored/untracked runtime and bytecode files.
  `behavior_and_handoff_qualified` covers behavior/proof/reporting;
  `strict_all_files_qualified` also requires no changes outside requested paths.
  Runtime files are not dropped to make a passing score. Exact reference source
  equality is a separate diagnostic, not required for a semantically valid fix.
- Missing, crashed and unfinished episodes remain rows; they are not silently
  removed from the denominator. Partial re-scoring creates another assessment.

Raw sessions/traces, process logs, preflight logs, snapshots and SHA-256 manifests
remain under the output directory. They are local evidence; review them for
credentials and private data before publishing. A hash is not a substitute for
reading the retained result.

If an older runtime exits before writing `report.json`, the assessor recovers
spent requests from its durable work ledger. It never reconstructs execution
proof or a successful completion from that ledger; unavailable counts remain
unknown. An explicit `summarize --reassess` can use a newer offline assessor while
retaining original inputs and prior assessments. Each assessment records and
snapshots its exact assessor code. Running with changed driver code remains
forbidden. The documentation proxy accepts shell line continuations and an
equivalent known-ID phrase; it still does not certify arbitrary prose quality.

## Interactive dogfooding

Use a separate prepared directory for a manual session and its frozen runtime,
so the interactive exercise uses the same build as the headless suite:

```sh
python3 -S scripts/run_coupled_bench.py prepare \
  --binary target/debug/spiral-coder --runtime-commit "$(git rev-parse HEAD)" \
  --out /tmp/spiral-dogfood-r1 --profile gemini-lite --case pilot-cli
cd /tmp/spiral-dogfood-r1/episodes/gemini-lite/pilot-cli/workspace
/tmp/spiral-dogfood-r1/runtime/spiral-coder tui \
  --provider gemini --model gemini-3.5-flash-lite \
  --observer-model gemini-3.5-flash-lite \
  --temperature 0 --max-tokens 32768 --timeout-seconds 90 --max-iters 24
```

Paste the neighboring `../task.txt` in the Coder pane. The installed `spiral`
launcher can be used when it points to this same build; an older npm release
does not contain the runtime under test. All edits stay in the copied exercise.
Run `summarize` afterward for independent
artifact checks. A manual session without the headless native report is labeled
`not_run` for that driver and cannot qualify native evidence or completion;
passing artifact tests alone does not create those receipts. Do not mix manual
edits with a declared headless episode: `run` rejects a changed initial workspace.

Compare the first divergence on the same task before changing a harness. Record
an intervention hypothesis and a new frozen pair, keep gates/oracles unchanged,
and retain failed rounds. A one-run model difference motivates an experiment;
it does not establish that a harness intervention caused an improvement.

The [initial 20-episode readout](../../docs/evals/2026-10-23-capacity/README.md)
retains three adaptive rounds, the transport failure and the remaining closeout
and tool-format failures. Its results are not a model leaderboard.

The [six-model matched follow-up](../../docs/evals/2026-10-24-action-closeout/README.md)
retains weak profiles from all three providers, adds the selected Luna/Large 4
counterparts, and reports action/closeout successes alongside weak-model failures.
Offline assessment corrections are versioned separately from runtime effects.

The [small-model repair follow-up](../../docs/evals/2026-10-24-small-model-repair/README.md)
retains two more rounds / 132 requests with unchanged tasks and assessors.
Flash-Lite and mini reach qualified completion in the second round; Mistral
Small's default-selection regression and first-round timeout remain recorded.
