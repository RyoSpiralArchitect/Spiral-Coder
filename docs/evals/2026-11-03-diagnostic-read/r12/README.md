# Returning to the original failed check after a repair

A successful automatic Cargo test does not discharge a different, declared
Python check. During a genuine verification phase the next Coder request now
offers the original pending command and its single matching check ID. Diagnostic
reads, reporting, approval and result validation keep their existing rules.

Two [before](menu-before.json)/[after](menu-after.json) loopback comparisons
used actual R11 data and no external model. In the isolated failure, read and
repair sequence, the old Coder request offered two IDs and an unrestricted
command; the new one offered only the original visible check. On a cold resume
of the **whole** compacted prefix, both executors restored Diagnose and left the
command unbound. A resumed prefix is not a reconstruction of the live slice.

The fresh pair used 23 Coder and 9 Observer requests in 339 seconds. Two
schema-constrained Observer answers were validated, accepted and followed by
the exact native reads. The pair reran the original visible check directly
after its step 11 and 18 source edits; both checks actually ran and failed.
This recovered useful feedback, not completion. Its final step 22 registered a
CLI variant before defining its argument type or match arm. The independent
build reported E0425 and E0004, so there was no executable for the held-out,
package or ordinary replay probes. The stock suite stopped at rustfmt. The
blocked-report attempt lacked evidence for every claimed completed acceptance
item; the session ended at the budget limit, with no accepted report and no
passing post-edit checks. The exact three-file candidate reapplied, but it
regressed relative to R11's compilable native listing. Neither was adopted.

[First outcome](results.json) · [Wire and read receipts](trace-handoffs.json) ·
[Independent patch replay](plain-replay.json) · [Patch](candidate.patch) ·
[Hashes](receipts.json) · [Predeclaration](plan.json).
