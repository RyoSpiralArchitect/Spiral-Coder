# Task requirements and native proof

Coder and Observer consume one task and one request budget. A model's plan is a
working plan, not a replacement for the original human requirements.

For repository maintenance, put a versioned block on standalone lines in the
original request:

```text
Implement selection, add tests and update the usage guide.
<task_requirements>
{
  "version": 1,
  "requirements": [{
    "id": "selection",
    "description": "Selection is wired, tested and documented",
    "artifacts": [
      {"path": "src/runner.py", "kind": "source"},
      {"path": "tests/test_runner.py", "kind": "test"},
      {"path": "docs/usage.md", "kind": "documentation"}
    ],
    "checks": [
      {"command": "python3 acceptance.py", "baseline_failure": true}
    ]
  }]
}
</task_requirements>
```

The caller supplies the acceptance check before work starts. Prefer a frozen
check outside the model's editable workspace when measuring task success. A
check can cover multiple requirements if it actually tests each one. No command
in this block is automatically executed.

For each declared requirement the completion gate requires:

1. A successful native `write_file`, `patch_file` or `apply_diff` result for every
   declared relative artifact path. Source, test, documentation and spec outputs
   remain separate obligations. Shell-only edits do not satisfy this artifact
   ledger; use native file tools for these tasks.
2. A current successful native `exec` result for every exact check, after the last
   mutation. A later mutation or failed rerun invalidates that success.
3. When `baseline_failure` is true, a failed native run of that check before the
   first mutation. The last pre-mutation attempt must have failed. Native edits
   are blocked until this baseline is recorded; missing baseline checks remain
   allowed through observation-loop policies. If the check
   already passes, the caller must revise the task/check; fabricating a failure
   or silently removing the requirement cannot qualify the task.

A governor refusal, an approval request, an automatic test or a command printed
in output is not a baseline. Diagnostic failure does not establish test quality:
the caller must check that the intended assertion failed.

Requirements are derived from the original request and paired native history.
They survive resume and protected message pruning without another durable store.
Assistant plans and Observer suggestions cannot replace them; a new human task
creates a new scope. Invalid blocks fail closed. Fenced examples do not declare
obligations. Both roles receive the same pending list, and the native Web
completion endpoint uses the same gate.

This is an explicit contract, not automatic semantic understanding of arbitrary
prose. Editing a test file does not prove its quality; a green command does not
prove it covers the request. Retain an independent oracle, review generated tests
and docs, and report runtime completion separately from task success. Requests
without a block retain the existing gates.

# Small JSON edits

`patch_file` accepts mutually exclusive text (`path`, `search`, `replace`) or
JSON (`path`, `json_pointer`, `json_value`, optional `json_expected`) fields.
Values are JSON-encoded **strings**, keeping the wire schema flat.

```json
{
  "path": ".spiral-coder/tui_replay.json",
  "json_pointer": "/cases/-",
  "json_value": "{\"id\":\"new-case\",\"checks\":[{\"kind\":\"hint_queued\"}]}"
}
```

Supply all fields required by the replay schema in the new case. This example
illustrates the edit shape, not a complete replay case or a passing test.
To replace an existing member, supply its pointer, the new value and its current
expected value, e.g. `json_pointer="/enabled"`, `json_value="true"`,
`json_expected="false"`. Append omits `json_expected`.

The runtime parses the complete file, including content beyond `read_file`'s
display limit. Other values are preserved; formatting may change. Missing
pointers, invalid values, stale expectations and duplicate appended IDs fail
without writing. It does not create parent objects or replace the whole document.
Existing preservation checks still apply. Approval queues prepare the same
content and reject a changed baseline. Native mutation receipts and proof
invalidation remain the same as other `patch_file` edits.
