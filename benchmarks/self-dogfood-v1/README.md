# Self-dogfood v1: recovery handoff

This first real-source task starts from Spiral-Coder commit
`9982a68a6978058c75a9a700e7407df5a49f010c`. A newer, frozen execution
runtime uses different Coder and Observer providers to propose a repair to that
baseline. The runtime does not reload candidate code during the episode.

The defect is a recovery-state regression after a failed exact check, diagnosis
of the test, a failed patch to a different source file, and a successful read of
that source. A live Fix stage becomes Diagnose again after session restoration.
The task requires repairing that transition without promoting stale diagnosis
past a newer failed check, failed edit, or failed read.

`prompt.txt` is the complete candidate assignment. Only its four listed files may
change; the documentation file must actually change, and the native task
contract also requires it. `probe.rs` is appended to a separate verifier copy, and has six tests:
one intended baseline failure and five negative-control passes. The successful
case covers native session JSON roundtrip and runtime Continue. It does not
contain an explicit new-human-task-boundary test. Existing Rust tests are also
run from the frozen baseline; tests added to the candidate can supplement them.

`reference/` is an operator-written feasibility repair. It is used only to prove
that the frozen checks are jointly satisfiable before provider calls. It is not
copied into the candidate repository or prompt, and is not a model outcome.
The candidate baseline predates this catalog. This is a published reproducible
case, not a hidden or contamination-resistant benchmark.

The two fixed configurations are:

| Spec | Coder | Observer | Shared wire cap | First Coder slice |
| --- | --- | --- | ---: | ---: |
| `luna-argon.json` | OpenAI `gpt-5.6-luna` | Gemini `gemini-4-argon` | 32 | 8 |
| `argon-large.json` | Gemini `gemini-4-argon` | Mistral `mistral-large-4` | 32 | 8 |

Each episode has a 1,200-second wall limit, 90-second provider timeout, and
32,768-token Coder response ceiling. These specs explicitly allocate 8,192 tokens to the Observer at temperature
0.2; the runtime default remains at most 1,200 tokens when no override is set. Luna retains the existing provider adapter's
`reasoning_effort=none` behavior; this is not a reasoning-enabled comparison.
Both roles must actually make a native provider request. Declaring two models
alone cannot qualify a result. These are two feasibility episodes, not a model
ranking or a controlled estimate of the Observer's causal contribution.

Checks independently build the candidate, run existing Rust tests, run the
six-case probe, and execute the candidate-built binary's seven-case TUI replay.
The required native task check is `cargo test --locked --bin spiral-coder -q`.
A candidate must also satisfy native completion, fresh proof, exact role and
budget receipts, strict file scope, and frozen-input integrity. A passing patch
is a proposal for review; the runner never adopts or merges it.

See [the runner workflow](../../docs/self-dogfood.md) for preparation,
credential handling, immutable declaration hashes, and limitations. External
receipts and verification copies are an evaluation boundary, not an OS security
sandbox. Run only trusted repository tasks on the chosen host.
