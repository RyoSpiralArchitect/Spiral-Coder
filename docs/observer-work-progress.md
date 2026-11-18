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

When a native failure requires an exact diagnostic read, the next-action prompt
asks for a single response contract and one read or no-tool clarification. It
quotes the failed check and its source path. On the hosted OpenAI native route,
the request uses a strict schema for these two alternatives; the Observer still
authors the path and confidence. Ordinary assistance, including terminal
diagnosis, keeps its existing format. Unsupported request options use the
existing metered compatibility fallback. Malformed or duplicate contracts,
invalid paths and insufficient confidence do not acquire forwarding authority.

In declared repairs both roles are prompted to check a proposed import in its
owner and inspect a caller. A file-scoped Rust identifier search can show
same-line lexical declaration and call locations separately from literal match
counts. These leads do not resolve symbols: inline or generated implementations
can still be valid, and code should be read before an edit.

After an edit addresses a diagnosed failure, the next request exposes the
original failed command as `required_recheck` only when it is still a declared,
pending exact check. During verification the Coder's exec menu offers that
command (or its own check ID on compatible routes). Read tools and truthful
blocked reports remain available. This is a request-time hint and schema:
the original approval, working-directory, validation and proof checks still
decide what can actually execute and what counts as passing.

All intervention modes also receive a whole-task route. It groups declared
artifacts by path and requirement, keeps dedicated test cases alongside shared
behavior rules, and shows the exact checks a further edit would refresh. Edit
receipts and current checks describe native activity; semantic coverage still
requires examining the implementation, controls and assertions.

The route estimates the requests left after reserving a full check sweep and
one report. The estimate is an optimistic lower bound, excluding unknown
diagnosis, reads, retries and test gaps. The Observer uses it to choose a useful
milestone, compare a feasible alternative and explain what must continue in a
later iteration. Mandatory diagnosis, rechecks and report-only turns keep their
existing precedence.

The review also asks where each identity or counter is owned and reset, using
the declared contract and observed producer or schema. Negative rejection
tests are paired with a question about valid composition across those
boundaries. This can expose an overbroad validator that rejects normal combined
inputs. It remains source-grounded advisory reasoning, not an inferred defect,
an additional mandatory test or a new acceptance criterion.

The existing `why_this_first` and `fallback` fields now reach the Coder with the
validated primary suggestion. Each is bounded to 700 Unicode characters and
marked when truncated. They describe the route and an observation that would
justify changing it. They remain quoted advice in the existing soft hint, with
the same confidence threshold and no additional scheduled action. The native
transcript saves the current system hint and retains it through context
compression. Starting the next continuation refreshes the system context;
historical Observer replies remain in the run's display record.
