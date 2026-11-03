# Unattended second iteration on the stock pilot

This comparison starts from the unmodified [pilot-cli fixture](../../../../benchmarks/coding-v1/cases/pilot-cli/task.txt).
The [frozen trial](plan.json) allowed two iterations of eleven task requests
each, three reserved Observer turns per iteration, and one transport repair
per iteration. Both roles used `gpt-5.6-luna`. One native command owned
the workspace, both ledgers, the original task and final stop. The operator
did not send a continuation prompt.

In the [original live run](results.json), the Coder first reproduced the
failure and changed `scripts/run_coupled_pilot.py`. It reached its limit
and submitted a truthful blocked report. The Observer recorded the missing
guide edit and final exact check in its handoff. A second iteration started
with the same task and a fresh, parent-linked budget. The Coder read and
changed `docs/pilot.md` and passed the exact check. The original independent
oracle accepted the repaired source, documentation and preserved tests.
These files are retained in the [candidate patch](candidate.patch).

The last Observer proposal identified the report as the only remaining
action but supplied invalid `done` arguments. Its one repair repeated a
JSON field. The original scheduler stopped with seven task requests still
available and no successful authored final report. The two rounds used
15 task requests and 16 wire sends.

[Matched replay](closeout.py) reconstructs this two-iteration native run
from the unchanged stock fixture. Tool IDs, model prose and the five raw
Observer replies are retained. For replay, native check selections are
reduced to their original ID, workspace paths are remapped, and the
compacted first Coder message is recovered from its displayed plan and
thought. The Coder executes the tools and the independent comparison
requires an exact match to the sealed final workspace. The checks for the
report-only wire require `gpt-5.6-luna`, a one-tool `done` schema and the
native report-only instruction after the 16 recorded replies.

The original binary stopped after the sixteenth wire. A first change
handled a recorded transport rejection but still stopped at the same
point because the original malformed arguments had hidden the explicit
report intent. On the revised binary, the seventeenth wire advertised
only `done`. Each local control reproduced the sealed candidate and made
zero external calls. [Wire comparison](wire-controls.json).

A [separately pinned single live call](tail-plan.json) then delivered just
that final wire to the Luna Coder. It reported the changed source and
guide, the failed baseline and seven passing tests after the edit. Native
validation accepted the report at step 12 of the task, during iteration
two. Both rounds used 16 task requests and 17 wires in the reconstructed
invocation. Sixteen responses came from the earlier run and one was newly
generated. The independent stock oracle passed again. The original fully
live trial ended without an accepted report.
[Final native report and independent assessment](tail-results.json).
