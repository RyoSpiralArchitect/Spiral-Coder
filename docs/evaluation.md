# Runtime evaluation evidence

## Copied-workspace build isolation

Each copied fixture must use its own Cargo build output. Sharing an absolute
`CARGO_TARGET_DIR` across copies of the same package can reuse a passing test
executable from another copy even when the current source still fails. A
controlled local audit reproduced this difference with unchanged fixture source.

Before any selected copied case runs, `eval` rejects an inherited absolute or
parent-traversing `CARGO_TARGET_DIR`. Leave it unset (Cargo then uses each copied
workspace's own `target`) or use a relative child directory. Non-copied cases
retain their existing build configuration. This guard covers the inherited
environment variable; fixture-local Cargo configuration and commands that
explicitly override a target directory still need separate review.

For example, build the executable first, then run the evaluation with
`env -u CARGO_TARGET_DIR target/debug/spiral-coder ... eval ...` on Unix.
Avoid using a shared target to save build time in correctness evaluations.

Runtime eval specs remain version 1. New case reports identify the evaluator as
`metrics.evaluator_revision: "outcome-proof-v2"`. Historical reports and raw run
artifacts retain their original results; they are not retroactively relabeled.

## Completion and errors

Every case requires `completed` and `error_free`, including cases with custom
checks. Explicit copies of those checks are not duplicated. Content matches,
tool-call counts, and files on disk cannot bypass these outcome requirements.

`agent_outcome` is runtime telemetry emitted before the response stream ends:

- `completed` is a Boolean describing the task outcome.
- `state` records the final runtime state for diagnosis.

A transport `done` event means the stream ended. An `agent_end` event with
`ok: true` means the run returned without an error. Neither alone establishes
that the task completed. The evaluator requires a successful run end, a true
`agent_outcome` from the latest round, and no error events. Missing outcome
telemetry in older traces fails the current completion check closed.

An iteration cap with intermediate text remains incomplete. An accepted `done`
call with remaining acceptance criteria also remains incomplete. A validated
runtime finalizer may complete the task after the last iteration. Its remaining
acceptance rows still prevent a completed outcome. This outcome records what the
runtime accepted; independent artifact and verification checks remain necessary
for correctness.

Verified-action automatic closeouts defer to the model when the root instruction
contains `Final answer must include` (case-insensitive). This covers the loop-start,
redundant post-verification tool, and session-end action finalizers. The runtime
returns the original final-answer instruction in its `done` hint and rejects an
empty action `done.summary` instead of replacing it with a generic summary. A
nonempty model summary still needs the usual acceptance and verification evidence;
the guard does not invent requested status labels or certify arbitrary prose.
Required literals belong in the displayed `done.summary` text; extra JSON keys
are not a final answer. Repair hints preserve exact capitalization and spaces,
and show the observed and required spelling for an ASCII case-only mismatch.
They leave successful execution proof intact and never insert claims themselves.
For edited files whose automatic tests fail, both immediate result shortening
and later history pruning retain up to four bounded diagnostic lines after the
first runtime test status. Pruning is idempotent; source diff text before the
test and later stdout status strings do not replace that status.
The existing independent final-answer checks remain authoritative. Prompts without
this explicit instruction retain automatic action closeout. This boundary does
not claim coverage of unrelated read-only finalizers.

Recovery stages take precedence over benchmark-plan tool coercion. In Diagnose,
requested diagnostic commands and file reads stay intact after a failed edit.
In Fix, the model's repair remains intact instead of being replaced by a generic
benchmark edit. In Verify, its verification call remains intact until recovery
completes. A no-tool response in any of these stages does not synthesize a
benchmark edit. Benchmark assistance resumes when the recovery governor clears
the stage; preserving a requested call does not bypass the stage's normal gate.

On resume, an unreviewed mutation is identified by its runtime step relative to
the last impact review, independently of the wording of the recovery prompt. An
approved benchmark task may reconstruct its missing plan using the existing
action-plan builder. The reconstructed plan must pass the task and instruction
contracts, then is retained before validating the impact block. If that block is
rejected, the next model request includes the retained plan's step and acceptance
labels. Invalid progress references still fail. Retaining a plan does not count
as verification or completion.

`pre_tool_gate_rejected` records otherwise invisible reflection, impact, and
single-tool gate failures: gate name, bounded reason, tool count, up to four
bounded tool names, response character count, and parsed-block presence flags.
It omits raw response text, tool arguments, and call IDs.
`impact_resume_plan_restored` records the mutation step and task lane when the
validated resume plan is adopted. These events diagnose blocked progress; they
do not change a case's outcome.

## Resume contracts and repeated edit failures

Native agent-session resume selects the latest human request, excluding explicitly marked
runtime continuations and feedback. The same root supplies exact file-content,
requested final-answer, and benchmark verification requirements. New human
steering replaces the previous root. Legacy untagged messages remain human;
previously saved runtime messages without provenance cannot be classified safely
from their wording alone.

A failed edit followed by a successful read remains unresolved. After two failed
edit attempts, reflection and a strategy-change hint remain active across reads
and resume. A successful edit or action-classified shell command clears this
retry counter; diagnostic/verification commands do not. A failing automatic test
still requires separate verification recovery. Pruning keeps the latest successful
reset exchange. An action classification is not evidence of a correct repair. Regressions exercise real missing-anchor
patch failures, read/repair cycles, session round trips, and pruning.

These checks establish contract retention and recovery mechanics. Live model
completion still requires the unchanged artifact, execution, and outcome checks;
a helpful hint or an emitted reflection is not a passing evaluation. Read-only
completion-path coverage, CLI auto-review/TUI automatic handoff task boundaries,
and Web/native final-handoff parity remain separate work. Auto-review continues
to start a new task: retaining the original root without resetting its completion
evidence could close the review round before its fixes are addressed.

## Localized automatic-test recovery

A localized failed automatic test is a pending check, even after a successful
repair operation or an unrelated passing command. Recovery retains the workspace
file and bounded cause, routes diagnosis toward the relevant evidence, and keeps
completion closed until the failed check itself passes when launched from the workspace root.
The same command in another directory is a different check; success-looking cwd
text inside command output cannot replace the runtime working-directory header. This complements edit
retry tracking: an edit can succeed while its verification remains unresolved.

Regression coverage must include failure → unrelated diagnostic → target read →
repair → verification, with a session round trip and pruning between those steps.
A failed target read must not create a permanent inspection lock. Text that looks
like a success status inside test stdout cannot override the runtime's first
status, and a path mentioned in unrelated output cannot grant verification.

Repeated failures of the original check must refresh its diagnostic. When the
new cause has no confirmed location, the old path is only historical context;
recovery allows broader diagnosis while retaining the verification obligation.
Regression coverage includes schema repair followed by a pathless semantic
failure, failed explicit reruns, and preservation of this distinction on resume.

Automatic-test display keeps a bounded diagnostic before a bounded output tail.
Workspace prefixes are shortened for display so a long absolute path does not
hide the relative source path, line, and cause. Rust source locations and Python
traceback frames remain adjacent to their diagnostic context. Output shortening
is not a new source of task instructions or successful execution evidence.

Malformed TUI replay JSON reports a bounded, JSON-escaped excerpt from the exact
source that failed parsing. A `target_message_contains` failure reports the
selected message ID and expected fragment, and clarifies that it checks the
selected message ID. The assertion semantics remain unchanged. Recovery snapshots
retain a bounded cause plus supporting diagnostic lines, rather than only a
failure count. Tests require an explicit selector repair to pass the same
assertion and verify that diagnostics do not modify the replay fixture.

## Provider continuation checks

Gemini's OpenAI-compatible tool responses can contain
`extra_content.google.thought_signature`. Transport coverage exercises late
metadata, repeated complete signatures, conflicting signatures, native and Web
SSE paths, blocked calls, compaction, save/load, resume repair, and provider
switches. A healthy tool-result round trip is required before model-quality
comparisons. Gemini uses `tool_choice: "auto"` to permit the explicit protocol
prelude; `required`/ANY suppressed it in the bounded live probe while AUTO
returned both text and the same function call. This changes request compatibility,
not plan/evidence/completion acceptance. An unavailable model or a missing-signature HTTP rejection is a
transport/access result, not a quality score. Legacy unsigned sessions may still
need a fresh Gemini run; no signature is fabricated to bypass validation. A
text-only Google response remains eligible for ordinary verified completion.
Synthetic-tool rescue is disabled for Google; unfinished text turns receive
runtime feedback requesting a native call after the completion gates. A pseudo-call written
as XML cannot execute a command or create a tool result. Regression coverage
keeps the human task unchanged and does not fabricate a tool result.

## Command and automatic-test evidence

Native colon-delimited protocol blocks recognize only the active block contract's field
names and aliases at the least-indented field level. A step's literal colon text,
nested fixture keys, or a more deeply indented recognized label remain part of
the current value. This keeps multi-step plans intact when a step describes JSON
or replay fields. `Acceptance Criteria:` is an explicit shared alias for the plan
acceptance field, with the same indentation and validation rules; unrelated
headings do not satisfy it. Existing XML and bracket-quoted field forms remain
supported. The Web parser still uses its earlier colon-field grammar; native/Web
plan-parser parity is not established by these native runtime runs.

`verified_command_seen` requires an assistant tool call and matching tool result
ID, an exact successful runtime exit-status header, and current proof. Command
matching ignores surrounding whitespace only: case, quoting, and internal
whitespace retain their shell meaning. Output printed by a command and final
assistant claims do not establish success. The recognized runtime pruning suffix
`[pruned NL]` preserves an otherwise valid success header.

A failed rerun revokes that command's previous success. Successful file edits
invalidate earlier command proof, including edits whose appended automatic test
fails. Other shell actions invalidate proof even when they fail or time out,
since they may already have changed files. Explicit runtime rejections and
nonmutating diagnostic commands preserve proof. The evaluator uses the runtime's
shared action classification; exact commands declared by `verified_command_seen`
are treated as verification commands under the spec's contract.

`successful_exec_commands` and `auto_test_pass_count` remain historical telemetry.
Checks use `verified_exec_commands` and `fresh_auto_test_pass_count`; those fields
may be empty even when historical counts are nonzero.

Automatic-test success must accompany a correlated successful file-edit result.
The first automatic-test status determines success, so a failed test printing a
success marker cannot manufacture a pass. Later mutations invalidate that pass.
A later failed explicit rerun of the configured test command also revokes it.
History digests and old-tool-result pruning retain the first automatic-test
status verbatim. Later stdout that prints a success marker cannot replace a
failed status, and older successful edit evidence remains available to resume
and evaluation after its verbose output is pruned.

A command-specific `auto_test_passed` check additionally requires exact agreement
with `verification_config.test_command`. The CLI records this telemetry after
project configuration is loaded, using the same command passed to the runtime.
A mention in the final answer or a different successful command is insufficient.
Missing command metadata in historical traces fails a command-specific check
closed. Checks without a command still require fresh automatic-test evidence.

These transcript checks cannot detect external filesystem changes, prove a test
suite is adequate, or replace artifact assertions. The command classifier and
explicit verification contract bound what shell mutations can be recognized.

## Artifacts and promotion

`tool_root_file_equals` compares file bytes with the UTF-8 encoding of `value`.
A missing file, extra text, trailing newline, or directory fails. The
`create-single-file` case uses this check for exactly `ship it` in
`notes/todo.txt`; a tool attempt and a claimed path alone cannot pass.

Before promotion, all requested artifact checks still apply. Only existence of
`.spiral-coder/governor_contract.overlay.json` is deferred because promotion itself
creates that reserved file. Content checks on that path, similarly named paths,
and all other existence checks remain prerequisites. The final case report
checks the complete original check set.

## TUI replay assertions

Every TUI replay case must contain at least one entry in its top-level `checks`
array. Loading or executing a case with missing or empty checks fails before
replay artifacts are created. An unknown nested `replay.checks` object does not
satisfy that requirement. A replay command's successful exit therefore requires
evaluated assertions; it is still only evidence for the assertions specified.

Replay failures print bounded structured records with the case, failed check,
and observed detail, plus the report path. These use `Error:` lines so the
Coder's error digest preserves them when verbose stderr is shortened. A missing
queued hint points to the recorded Observer suggestions; descriptive
`quickest_check` text alone does not queue a Coder action. Parse errors include
examples serialized from the supported check enum, including required `value`
fields. `target_message_contains` checks the selected message ID, not source
contents. Diagnostics do not populate suggestions, repair cases, or waive checks.

Target inference requires the latest nonempty assistant message in top-level
`coder_messages` to describe a failure. An explicit `msg:coder-<index>` selector
can select an earlier completed, failure-like assistant message. User and tool
messages are never valid targets. Missing-target diagnostics report role counts
and this distinction; changing a check's expected value cannot supply a target.
When an edit succeeds but its automatic test fails, history compaction preserves
the first status and prioritizes up to four error lines from the test output
before diff context. Nonstandard failures retain a representative failure line.

The [2026-10-08 harness comparison](evals/2026-10-08-harness/README.md)
records the unchanged six-case baseline/candidate runs and their unresolved live
failures. It is evidence for review, not runtime promotion.

The [2026-10-09 recovery comparison](evals/2026-10-09-recovery-focus/README.md)
adds localized failure retention and fresh runs against the preceding candidate.
The live traces exercise target rereads, but neither provider completes TUI
replay; the opposite changes in suite totals do not establish nonregression.
