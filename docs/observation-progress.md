# Observation progress

`agent/observation_stall.rs` derives repeated observation from correlated native
calls/results in the current human task. For existing-file repair/edit lanes,
the progress gate refuses a third `read_file` or `search_files` with exactly the
same JSON arguments after two matching native outputs, even when there is no
active verification failure. This closes the gap where a task could remain in
inspection forever before making its first edit.

Different ranges, targets and queries remain available. The Fix-stage gate also
permits a range not previously observed. Required failed-edit diagnosis and the
Diagnose stage take precedence. Read-only tasks are unchanged. The refusal asks
for a specific missing fact, a new targeted observation, a behavioral check, an
evidence-supported edit, or an honest incomplete report. It does not synthesize
an edit or accept an Observer proposal on the model's behalf.

Cache wrappers are normalized; native range headers must agree with the call.
Unknown, pruned or failed observations cannot establish duplicate contents.
A changed result resets its counter. An executed command or actual edit attempt
resets all counters because it can change files or supply new diagnostic
information, even on failure. Blocked/rejected actions cannot reset pressure.
A new human task resets history; runtime Continue and Observer handoffs do not.
The comparison describes recorded outputs, not the current filesystem or proof
of diagnosis. Different spellings/default arguments are conservatively distinct.
There is no total-read cutoff and the shared request budget remains authoritative.

## Unchanged rereads

A second rule compares reads with the file as it is now, so it applies to every
task lane and is not reset by intervening edits or commands. A `read_file` is an
unchanged reread when reading the same arguments now would return exactly the
latest native result for those arguments. The rule counts how many consecutive
identical results those arguments already returned in the current task.

From the third identical result on:

- The Coder's call is refused with a `[Progress Gate]`. The refusal lists the
  failing cases of the latest executed command when it failed.
- An Observer suggestion of the same read gets an advice issue. The Observer
  uses its one repair attempt to propose an action that uses the content.

A required diagnostic read and the target read of a failed edit are never
refused. Each identical check failure still requires its location to be read,
and the diagnostic follow-up still compares the failures once per unchanged
chain. A different range, another file and any change to the file on disk
remain new evidence.

The rule was measured on GPT's fresh #105 trials B–R, using their recorded
histories and their native recovery snapshots. It would have refused 17 of
716 reads across 13 runs, between 0 and 4 per run. The eight rereads of
the path check in R were all required reads, so none of them would be refused.

Both roles also receive [work progress](observer-work-progress.md), which keeps
the original request separate from explicit proof requirements and records
actual editing/verification activity. This advisory projection and observation
pressure do not change acceptance conditions or independent candidate tests.
