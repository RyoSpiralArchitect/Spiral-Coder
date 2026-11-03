# Coordinating a CLI variant, argument type and handler

The R12 patch fails with E0425 for the missing argument type and E0004 for the
missing match arm when applied to its original Git tree. Restoring just
`src/main.rs` makes the binary compile and ordinary replay available, but
the new command disappears from help. [Compiler control](r12-reproduction.json)

For R13, both roles received a pointer to the existing sibling's three route
locations. The pair applied three hunks together in the final edit to
`src/main.rs`. The variant, argument type and exhaustive match were present,
but dispatch referenced a missing handler. The independent build failed with
E0425 for `tui_replay::list`. Both final native checks lacked a passing result,
and the binary-dependent held-out, packaged and ordinary probes could not run.

The pair stopped in 153 seconds with 15 Coder and 6 Observer requests. Two
successive general Observer replies recommended lines 150 through 350 of the
correct file, one line over the inclusive limit. Both were refused. R13
ended `observer_unavailable` with remaining budget and an exact, unqualified
one-file patch. [Original protocol](protocol.md) · [Results](results.json) ·
[Independent patch replay](plain-replay.json) · [Handoffs](trace-handoffs.json).

A shared native range error now gives the first legal window and where to
resume. The recorded invalid proposals stay rejected. The [one-request
Observer comparison](observer/README.md) received the corrected window and
passed native validation. The [handoff attempt](handoff/README.md) retained its
on-disk partial result after a host restart.

In the already qualified search repair H3, the Before field followed another
sentence on the same line. A replay of the actual done call now rejects its
layout after all declared evidence is ready. Changing that one space to a
newline passes. Neither replay changed the source or called an external model.
[Old executor](report-before.json) · [New executor](report-after.json) ·
[Loopback replay](replay_report.py). The earlier H3 result remains sealed.
