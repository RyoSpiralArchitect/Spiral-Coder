# R16 stopped with a repair and a dispatch line still unexecuted

Starting from the same original commit and prompt, the Luna Coder and Observer
used 24 and eight task requests respectively. The pair reproduced the missing
command before editing and changed three Rust files. At step 21 it inserted a
type alias between an existing derive attribute and its struct. The automatic
test failed. At step 22 it proposed moving the alias above the attribute,
but the new failure still required diagnosis. The correction did not execute.
The native session accepted its incomplete report and ended blocked after
265 seconds. The independent build failed, neither required native check had
post-edit proof, and the binary-dependent hidden, npm and ordinary replay
probes did not run. The patch reproduced byte-for-byte on the starting tree.
[Original run](results.json) · [Patch replay](plain-replay.json) ·
[Pair handoffs](trace-handoffs.json).

## What the pair's two proposals would have changed

A local control rebuilt the sealed candidate, then made two changes to
that disposable source tree. First it applied step 22's exact search and
replacement. The derive and argument-type errors disappeared. The compiler
still reported E0004 for the missing dispatch arm. Second, the control placed
the pair's one dispatch line from step 15 next to the actual sibling arm.
An operator chose that anchor. The full original step 15 patch had been
refused because its other anchors did not match the source.

The resulting native binary built, listed the expected case and passed nine
of eleven hidden tests. Empty-selection wording and the guide still failed,
and the packaged launcher still sent the command to TUI. This derivative
has no role in the original episode's qualification. The next testable
handoff is whether the Coder can recover its own proposed edit after reading
the source required by a new diagnosis, while checking that the target
still matches. [Two original proposals](proposals.json) ·
[Builds and executable results](route-control.json).

Run the control without model credentials from a Git checkout containing the
pinned source, with Rust, Python and Node available. An optional disposable
Cargo directory outside the checkout can reuse dependencies.

```sh
python3 -BIS docs/evals/2026-11-03-catalog-route/r16/route_control.py
```
