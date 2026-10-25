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

Both roles also receive [work progress](observer-work-progress.md), which keeps
the original request separate from explicit proof requirements and records
actual editing/verification activity. This advisory projection and observation
pressure do not change acceptance conditions or independent candidate tests.
