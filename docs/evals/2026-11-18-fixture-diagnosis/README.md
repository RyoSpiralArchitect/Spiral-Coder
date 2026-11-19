# Diagnose a failing test's fixture

The [earlier focused run](../2026-11-18-interruption-challenge/README.md#a-wrong-fixture-changed-the-validator) wrote an empty ledger under the first directory but kept the second directory in the manifest. It edited the validator while that mismatch was unresolved. The new runtime quotes the specific case's setup, the called fixture helper, and the matching branch of the producer in both prompts.

A matched local replay executes the archived test edit and observes its real automatic test failure. Both runtimes require the same read of the failed test. The amended Observer request cites the test's first-directory write and the fixture's second-directory branch. The captured request grows from 15,919 to 19,071 bytes. Each runtime used five scripted Coder requests and three scripted Observer requests over loopback. [Prompt evidence](fixture-context-replay.json)

A second matched replay starts from the same restored diagnostic tree used in the live follow-up. Its Coder selects the original exact integration check and performs the mandatory read. The amended runtime returns the slice to Observer immediately after that read. The trace binds the handoff to the exact native read call. [Read handoff evidence](post-read-replay.json)

## Live results

All runs used gpt-5.6-luna for both roles. Each preserves the first terminal and an independently replayable delta. The two diagnostic runs use the same starting files, human task and budget. Their starting files contain two archived native edits, verified against the recorded after-edit fingerprints. The test-producing code, working directory and budget stay fixed across the two diagnostic trials. Model responses can vary between trials.

- [Full repair plan](focused-plan.json) starts with exactly the earlier #105 focused prompt and its 24/24 candidate, with an empty conversation.
- [Diagnostic plan](diagnostic-plan.json) and [matched rerun plan](post-read-plan.json) start with the known failing test at the first bad fixture edit. [Human task](diagnostic-task.txt) · [Protected directory-pair check](visible-directory.py)

The broad repair finished in one iteration with 25 task and 25 wire requests. All seven exact checks passed after the last edit. Independent checks passed all seven original groups and 24/24 original cases. Its six authored tests passed against the reference and detected all four original seeded faults. The public tests accepted both normal and pretty-printed two-row JSON and detected all three altered outputs. The skipped-directory pair passed. The same binary read the completed live run and two genuine first-interruption controls without changing files.

Nine additional writer-state cases still fail. They cover contradictory known and unknown totals, known limits masked by pending usage, the pending task cap and unsupported charge values. The exact Rust integration tests fail compilation for `x86_64-pc-windows-gnu` because they call Unix symlink APIs unconditionally. These failures keep #105 open. [Independent score and native receipts](focused-results.json)

| Diagnostic run | First changed file | Terminal | Task / wire requests | Current exact checks |
| --- | --- | --- | --- | --- |
| Source excerpts in both prompts | `src/coupled_status.rs` | Blocked after reverting the source change and repairing the test | 32 / 35 | 2/4 |
| Source excerpts and the first-read return | `src/coupled_status.rs` | Blocked with remaining source changes | 32 / 34 | 0/4 |

The first diagnostic run identified a fixture discrepancy before its first
edit. A later test-only proposal was rejected because its evidence block
omitted the target fields. The next proposal included the structured fields and retried the fixture edit. Recovery required another diagnosis. The first
terminal still contained a test-only patch. [Diagnostic receipts](diagnostic-results.json)

The rerun emitted the additional Observer handoff exactly once after the
mandatory read. The following Observer proposed reading the validator and
the Coder accepted. This left the setup untouched and the repair stopped
with all four exact checks pending. Both candidates still rejected a skipped
directory in the independent pair. Scores on the 27 interruption cases
were 18/27 and 16/27 respectively. [Rerun receipts](post-read-results.json)

## Recreate the observed input

Apply the [two archived edits](diagnostic-start.patch) to the [original focused
starting tree](../2026-11-18-interruption-challenge/README.md) and copy
`visible-directory.py` to `scripts/status_interruption_directory.py`. The
diagnostic plans pin the resulting file inventory and the exact task text.
The three terminal patches replay from their respective start trees and
reproduce every file entry in the terminal inventories. [Full repair
patch](focused-candidate.patch) · [First diagnostic patch](diagnostic-candidate.patch)
· [Rerun patch](post-read-candidate.patch)

The operator ran the exact `interrupted` generator after both trials and
read its written manifest and ledger paths. It writes a pending pointer to
`iteration-02` and starts with one finalized row. The authored test clears
the finalized rows, records aggregate counts of 1/1, and writes the new
ledger in `iteration-01` while retaining the generated pointer. The new
ledger literal in that exact saved revision also fails JSON parsing. These
observations identify the smallest evidence needed before recommending an
edit. Show what the generator wrote, compare it field by field with the
test's mutations, then exercise a coherent first-iteration input and the
single skipped-directory change. [Observed input fields](input-observation.json)
