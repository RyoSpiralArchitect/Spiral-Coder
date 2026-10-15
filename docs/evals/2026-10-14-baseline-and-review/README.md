# Initial failures, review boundaries and truthful closeout

This record retains all **36 episodes and 240 wire requests** from rounds 11–14.
Coder and Observer remain two roles of one task, using the same selected model
and one shared request cap. Correct artifacts, current proof, requested tools
and the authored final report are separate requirements of strict success.

## Runtime repairs and deterministic evidence

Round 10 exposed initial test failure followed by unrelated repository
diagnostics. Candidate `7b49d96` localizes an explicit failed check to a unique,
existing workspace file from a Rust diagnostic or innermost Python traceback.
Both roles retain the original check and fresh diagnostic-read obligation across
pruning and continuation. Ordinary path mentions, ambiguous locations, outside
paths and rejected commands do not establish that focus. An actual Cargo fixture
tests failure → diagnosis → repair → the original check, with unrelated success
unable to discharge the obligation.

Candidate `dff6f65` also addresses four PR review findings:

- A new human task discards the previous rollback checkpoint before startup and
  captures its own checkpoint; same-task continuation retains it.
- Browser work handles recheck durable task/revision state before writes and
  tool dispatch, so another tab's newer task invalidates a stale handle.
- Lite retains ordinary meta diagnosis while gating unsupported next-action and
  archive endpoints. Actual browser QA additionally exposed a hash helper scoped
  only to Coder and a missing Lite governor-contract endpoint; both are repaired.
- HF subprocess inference reserves budget before spawning, including Observer
  assistance and repair. Failed attempts remain charged. Tests use an actual
  local subprocess fixture, not a live HF model.

Round 11 Gemini completed a proof task without any final assistant message:
an empty response reached automatic stop checks, which granted completion.
Candidate `7b951b7` keeps the authored report pending and requests native closeout
within the original cap. Actual HTTP TUI regressions cover both a subsequent
valid report and repeated empty responses, without rerunning passed checks for
reporting. This does not synthesize the requested prose.

Round 13 Gemini exposed another handoff defect: an evaluator policy said to
proceed to the patch, but resume interpreted the non-executed refusal as a new
failure requiring diagnosis. Final candidate `7efd46d` restores recovery from
executed results, looking past refusals without erasing older actual failures.
An actual HTTP TUI regression now executes that already diagnosed repair in one
request, while still leaving verification/reporting incomplete. Plan, approval,
evidence and verification gates remain active.

The final `bash scripts/check-local.sh` passes: **739 Rust tests, 1 ignored**,
1 integration test, TUI replay 3/3, Python 16, Web 67, repo-map 14/14, syntax checks
and the Lite smoke. See [verification.json](verification.json). Failed and passing
regression logs are hashed in the raw manifest. These establish the tested
runtime behavior, not model quality.

## Browser exercise

The actual Lite and Rust servers were exercised with bundled Playwright 1.62.1
and installed Chrome because the Browser plugin was not available. Provider
responses were local fixtures. Desktop 1440×1000 and mobile 390×844 were checked;
there was no relevant application console error or mobile horizontal overflow.
The Rust favicon 404 is retained as an unrelated asset observation.

Clicks on diagnosis show the result and release the busy state on Lite without
calling absent archive routes. Rust diagnosis is saved and reopened in Meta.
Two real tabs then replace a task while its old native provider reply is held:
releasing that reply causes **zero stale mutation requests**, preserves the new
durable task, and shows that the old run has stopped. One workspace-setup command
ran before replacement and is separately recorded.

QA scripts, screenshots, prior failed attempts and logs remain outside the repo;
their hashes and the result pointer are in [verification.json](verification.json)
and [raw-manifest.json](raw-manifest.json). Browser checks do not undo work already
dispatched to a server, and localStorage is not an atomic transaction system for
truly simultaneous writers. Other browser engines and the full mobile archive
flow were not exercised.

## Fixed and adaptive live panels

The suite remains `tests/fixtures/coupled-loop-v1.json`, SHA-256
`3c3c24ddec901a769158b6cab126797bdb2427243387b0b9b315e5e3182e4752`.
All 13 fixture hashes, assertions, the required `patch_file` call and caps
**10 / 8 / 12** are unchanged. Temperature is 0, output limit 2048 tokens,
request timeout 90 seconds and episode timeout 600 seconds.

| Model in both roles | R11 | R12 | R13 | R14 |
| --- | ---: | ---: | ---: | ---: |
| `mistral-small-2603` | 1/3 | 1/3 | 1/3 | 1/3 |
| `gpt-4.1-mini` | 2/3 | 3/3 | 2/3 | 3/3 |
| `gemini-3.5-flash-lite` | 1/3 | 2/3 | 1/3 | 1/3 |
| Strict task success | **4/9** | **6/9** | **4/9** | **5/9** |
| Wire requests | 58 | 59 | 63 | 60 |
| Runtime-completed tasks | 6/9 | 7/9 | 5/9 | 6/9 |

Rounds 11–12 were predeclared as two panels on one unchanged candidate:
**10/18 strict successes, 117 requests**. Some final R11 cases overlapped the
start of R12; neither candidate nor evaluation inputs changed. R13 and R14 are
separate adaptive follow-ups after the newly observed bugs, not additional
repetitions of the first candidate. No episode timed out or exceeded its cap.

- R11–R12 binary: `2eb919543ef30f16bedfb3fbea5d2655e6a7c08d8d81d7df92dc1d00449abcb8`
- R13 binary: `3ba98cd65f1d356e8b1949656ccdfd4018021638249a86de583c07542fe559f3`
- R14 binary: `efb34e29937d010c603e99380142587da431cb52cd44f1747fd8dab6efca61bb`

All **12/12** replay outputs preserve complete old JSON, including nested
evidence and extension fields. **7/12** contain a new case passing full old+new
replay; **4/12** pass every frozen task requirement. R11 and R14 Mistral replay
finish with valid artifacts/proof but omit the required `patch_file`; those
remain strict failures. R12 and R13 Mistral source repairs have the same tool
omission. No gratuitous edit or relaxed assertion was used to manufacture a pass.

Final GPT is 3/3, but its R11 and R13 JSON edits repeatedly fail. Mistral still
spends proof-task budget on redundant checks and finishes with acceptance items
left over. Final Gemini reaches a real patch, but it is ineffective; its failed
automatic test correctly requires a fresh diagnosis, and the cap expires before
repair. Its replay run also chases a referenced source path and misses plan/think
requirements without creating the requested case. Earlier Observer advice
sometimes contradicts newer receipt evidence. These remain open in
[failure-modes.json](failure-modes.json).

## Qualification and receipts

These small development panels do not establish improved reliability, general
nonregression, model rankings, or Observer's causal contribution. There is no
Observer-disabled control. The PR is open and marked ready for review; that UI
state is not a successful live qualification or a merge decision.

[receipts.json](receipts.json) preserves every fixed check, cap, role count,
numeric usage/duration when supplied, assist outcome and Observer parser error.
[raw-manifest.json](raw-manifest.json) hashes all retained binaries, declarations,
launchers, native sessions, traces and independent oracles. Opaque provider
continuation metadata stays local. Original failed panels remain unchanged.

Across this PR, including [rounds 8–10](../2026-10-14-repair-continuation/README.md),
[rounds 5–7](../2026-10-14-task-obligations/README.md), and
[rounds 1–4](../2026-10-14-coupled-loop/README.md), the retained total is now
**126 episodes and 871 wire requests**.
