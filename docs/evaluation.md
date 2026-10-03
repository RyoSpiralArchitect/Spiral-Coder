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

## Command and automatic-test evidence

Colon-delimited protocol blocks recognize only the active block contract's field
names and aliases at the least-indented field level. A step's literal colon text,
nested fixture keys, or a more deeply indented recognized label remain part of
the current value. This keeps multi-step plans intact when a step describes JSON
or replay fields. Existing XML and bracket-quoted field forms remain supported.

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
