# Shared work progress for Coder and Observer

`agent/work_progress.rs` derives `task_obligations.work_progress` from the current
human task and correlated native execution. The same projection reaches the
Coder obligation prompt, TUI Observer and browser Observer. Browser-supplied
progress fields are replaced by native projection. The Coder's existing
obligation prompt also opens for an ordinary repair task once native
observations repeat the same arguments, even without a typed proof contract. This uses the existing
repair/read-only lane classification; empty tasks, idle repairs, a single read
and read-only inspection do not gain an extra obligation prompt from this rule.
No new persistent state, requirement, completion authority or automatic action
is introduced.

The projection keeps these surfaces distinct:

- `original_human_task` preserves up to 8,192 Unicode characters from the current
  root request, with its total character count and an explicit truncation flag.
  Runtime continuation does not replace that root. New human steering starts a
  new task, so prior edits, reads and completion claims cannot satisfy it.
- `task_requirements` remains the declared machine-checkable proof contract.
  It is not necessarily a complete semantic description of the task. A required
  documentation edit plus test does not erase a source repair requested in prose.
- `native_file_edits` counts successful, failed, blocked and unconfirmed native
  file-tool attempts, and lists up to 32 successful paths with an omission count.
  A successful write with a later failed automatic test still counts as a write.
  Unmatched results and assistant assertions do not count. These are write
  receipts, not a current filesystem diff or proof that a repair is correct;
  shell execution can also change files.
- `observations_since_last_edit_or_exec` summarizes returned native read/search
  actions, groups identical tool/argument pairs and retains up to eight repeated
  groups with an omission count. Any executed process or successful file edit
  starts a new observation window. A failed or refused file edit does not.
  Returned reads can themselves fail; their count proves neither diagnosis nor
  complete coverage. Current native results retain the content and freshness.
- `last_done_claims` preserves acceptance lists from the last accepted `done`
  action as Coder-authored claims. An accepted incomplete report does not mean
  the task was completed. Pending exact checks remain separate native facts.

Before recommending documentation, a check or a final report, both roles compare
these facts with the original task and its acceptance criteria. If source work
has no recorded edit, the Observer must not silently collapse the task into its
explicit documentation proof obligation. Repeated observation should identify a
specific missing fact and a bounded next action. A local patch still requires
adequate current content; uncertainty permits clarification, not an invented
edit or forced adoption of a suggestion.

This view intentionally does not infer semantic completion from a filename,
amount of activity, passing old tests, an Observer claim or a `done` receipt.
Independent behavioral checks and the existing native completion gates retain
their authority.
