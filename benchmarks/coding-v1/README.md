# Coding capacity and harness development matrix

This suite runs the actual headless TUI Coder + Observer consumers, with the same
model for both roles. It is a three-case **development** suite for finding harness
failure modes. It is not held out, a broad coding leaderboard, or proof that a
model can maintain the full repository. The historical runtime/coupled panels
and their failures remain unchanged.

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

## Prepare without provider calls

Build a runtime containing PR #31's bounded reads and task proof first. The
runtime commit is supplied explicitly and its executable hash is retained.
The example uses the working tree's build; do not label an unrelated binary with
the current commit. Model catalog changes are not required for explicit IDs.

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

The provided matrix includes Gemini 3.5 Flash-Lite / Gemini 4 Argon,
GPT-4.1 and Mistral Medium 3.5. GPT-4.1 is the larger counterpart of the established
GPT-4.1 mini baseline, not OpenAI's latest flagship. Copy `matrix.json` to select
other supported model IDs, including Anthropic, before preparing a new output.
Use a runtime with compatible provider parameters for newer GPT/Claude models
(the separate PR #34 supplies current Chat Completions compatibility). Responses
API-only models require a different transport and are not validated by this suite.

Every profile uses temperature 0, 32,768 output tokens per request, a 90-second
request timeout and 600-second episode wall cap. The entire four-profile matrix
has at most **264 wire requests**. Coder, Observer, repairs and native transport
attempts share each case's cap. The driver does not retry an episode or substitute
a different model. A new trial needs a fresh directory and predeclaration.

The output limit is higher than the earlier 2,048-token pilot because Argon uses
one limit for thinking plus output; Google recommends at least 32,768 for existing
apps with a smaller limit. Thinking remains at each provider's default. Equal
request/output ceilings are **not equal compute, latency or price**; compare
returned usage as well, and keep missing usage unknown. See the
[Argon output-limit guide](https://ai.google.dev/gemini-api/docs/latest-model#setting-output-token-limits)
and [Mistral Medium 3.5 tool support](https://docs.mistral.ai/models/mistral-medium-3-5-26-04).

## Read the separate outcomes

Each immutable `assessment-NNN.json` records:

- Runtime completion, independent artifact behavior, native pre-edit failure and
  fresh exact checks after the latest mutation.
- Authored report fields and acceptance, request roles/usage, Observer assists,
  gate blocks and the ordered tool actions that locate the divergence.
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

Use a separate prepared directory for a manual session. Enter an episode's
`workspace`, start `spiral --provider gemini --model gemini-3.5-flash-lite`, select
the same model for Coder and Observer, and paste the neighboring `task.txt`.
All edits stay in the copied exercise. Run `summarize` afterward for independent
artifact checks. A manual session without the headless native report is labeled
`not_run` for that driver and cannot qualify native evidence or completion;
passing artifact tests alone does not create those receipts. Do not mix manual
edits with a declared headless episode: `run` rejects a changed initial workspace.

Compare the first divergence on the same task before changing a harness. Record
an intervention hypothesis and a new frozen pair, keep gates/oracles unchanged,
and retain failed rounds. A one-run model difference motivates an experiment;
it does not establish that a harness intervention caused an improvement.
