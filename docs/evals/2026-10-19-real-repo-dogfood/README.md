# Real repository dogfood — 2026-10-19

**No live episode completed this task: 0/9 passed the independent CLI oracle.**
This is a first real-repository maintenance task, separate from the historical
three-case coupled-loop fixture. Coder and Observer remain one task-level unit.
The final integrated feature was completed by the maintaining agent after the
live candidates were frozen; it is not a live-model success.

The task was to add repeatable `--case CASE_ID` filtering to the pilot runner,
offline tests, and a documentation example. [task.txt](task.txt) is unchanged
across all three rounds. Each provider started from an isolated archive of the
candidate repository, with a declared Python-only test-command overlay. Both
roles used the same selected model, a shared cap of 18 provider requests,
temperature 0, 2048 output tokens, 90-second request timeout and 600-second wall
cap. Historical fixtures, assertions, request caps and failed receipts were not
modified. No Observer-disabled ablation was run.

| Both roles use | R1: runtime / CLI | R2: runtime / CLI | R3: runtime / CLI | Requests R1/R2/R3 |
|---|---|---|---|---|
| `mistral-small-2603` | incomplete / fail | complete / fail | complete / fail | 18 / 14 / 16 |
| `gpt-4.1-mini` | incomplete / fail | complete / fail | incomplete / fail | 18 / 8 / 18 |
| `gemini-3.5-flash-lite` | complete / fail | incomplete / fail | incomplete / fail | 2 / 18 / 18 |

There were **130 provider requests**, six Observer assists, no timeout and no
shared-cap overrun. Four runtime completion claims were false according to the
independent oracle; one remains in the final live round. These adaptive rounds
establish neither a task-success improvement nor model rankings. The independent
oracle was frozen before R1 and checks actual CLI behavior using a fake binary,
without network calls. It checks default selection, single selection, repeated
out-of-order IDs, invalid IDs, subprocess order and both selection receipts.
It is necessary but not sufficient for the full task: tests, docs and final
reporting also require review. All candidates already fail this necessary check.

## What the runs exposed

1. **Malformed native arguments were treated as tool failures.** Truncated large
   patches became empty arguments, and raw exec text had a shell fallback. R2
   rejects the entire malformed turn before dispatch, preserves every original
   native call and correlated refusal, and requests a complete smaller edit.
   The request remains spent. This improves diagnosis, not model compliance:
   GPT still repeatedly sends truncated rewrites in R3 and changes no file.
2. **Report wording bypassed authored closeout.** R1 Gemini emitted a text-only
   pseudo-call and changed nothing, yet old tests let it auto-complete. R2
   recognizes bounded positive final answer/report/response/summary requirements.
   A real HTTP regression now leaves that sequence incomplete.
3. **An intermediate passing test forced premature closeout.** R2 Mistral's
   failed next patch needed a reread, but the Completion Gate demanded `done`.
   R2 GPT's partial helper addition was similarly treated as finished. R3 keeps
   authored-report tasks open for normal diagnosis and repair. A failing-before,
   passing-after HTTP regression covers the repair read after an automatic test.
4. **Plain prose exact checks were omitted from obligations.** R3 recognizes
   positive inline `Run exactly:` directives for known verification commands.
   The existing native ledger requires separate successful calls after the last
   mutation, including after restart. Automatic tests are insufficient. This
   bounded extension does not interpret arbitrary prose or bless shell mutations.
5. **Semantic completion remains unresolved.** R3 Mistral executes both requested
   commands but leaves the selection disconnected from the execution loop,
   filters unknown IDs away before validating them, and adds neither requested
   tests nor docs. Its old tests pass and its final report overclaims completion.
   R3 Gemini repeatedly observes without editing; the scaffold progress policy
   also blocks some reads needed by this multi-file maintenance task. These
   remain next-round work, not repaired or qualified behavior.

The rendered Web check also exposed a separate persistence race: reloading
within the 350 ms display-save debounce kept native receipts but lost the visible
conversation. The integrated browser fix flushes the latest thread snapshot on
`pagehide`. A no-timeout unit regression and an immediate-reload Chrome check
verify that both the conversation and its incomplete native task survive.
This browser-only repair follows the frozen R3 candidate.

The next dogfood task should retain a failing-before acceptance check for each
new behavior, distinguish artifact/test/doc obligations, and allow bounded reads
of remaining requested files. Passing an old configured test cannot prove that a
new behavior exists. No heuristic added here claims to solve that distinction.

## Integration and audit

The maintaining agent implemented the final runner change in 11 added lines:
validate requested IDs before output creation, select in suite order, execute
that selected list, and record the ordered IDs in metadata and summary. Four new
offline tests cover default selection, repeated selection, unknown IDs without
credentials, and preservation of existing output. The unchanged external oracle
passes all four scenarios on this integrated version. No model-generated patch
was adopted as complete.

[R1](r1-predeclared.json), [R2](r2-predeclared.json), and
[R3](r3-predeclared.json) bind the source archives, binaries, prompt, oracle and
configuration. R1 used main `7862e3d`, R2 `5a5293c`, and R3 `a9b03f9`; the manual
feature integration follows R3. [receipts.json](receipts.json) contains the CLI
oracle verdicts, request telemetry, bounded action categories and final pending
checks. Missing provider usage stays null. [raw-manifest.json](raw-manifest.json)
hashes the retained local sessions, traces, patches and logs. `LOCAL_RAW` resolves
to `~/.cache/spiral-coder/dogfood-20261019`. Raw native provider continuation
metadata stays local; credentials are not published.

To re-evaluate a retained workspace without a provider:

```sh
python3 -S docs/evals/2026-10-19-real-repo-dogfood/oracle.py /path/to/workspace
```

Main's hosted checks recovered on both Ubuntu and macOS in
[run 38733164793, attempt 2](https://github.com/RyoSpiralArchitect/Spiral-Coder/actions/runs/38733164793).
The candidate's local check and browser evidence is recorded in
[verification.json](verification.json). Hosted/local checks and mocked browser
interaction verify engineering behavior; they do not qualify the live task.
