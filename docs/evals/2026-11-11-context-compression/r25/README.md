# R25 completion and further prompt compression

The two `gpt-5.6-luna` roles completed issue #49 from the original
`55351155` repository in three bounded iterations. The executor was merged
main `737b395`. R25 started without a session seed or operator edits and retained
the R24 prompt, input inventory, protected paths and budgets.
[Frozen plan](plan.json)

R25 used **73 task requests, 80 wires and 781.906 seconds**. Iteration one
implemented and checked the native command. Iteration two delivered the npm
route and guide, corrected invented example IDs after the guide check failed,
and exhausted its budget before final verification. Iteration three performed
all five checks after the last edit and submitted accepted `done`.

All multi-round qualification gates and all six independent check groups passed,
including 11 held-out cases, the packaged launcher and preserved original
regressions. The measured guide example matched the packaged candidate. This
is a three-round qualification; it did not pass the original single-round cap.
The patch remains a historical candidate. Main already contains the previously
reviewed catalog implementation; this record does not replace it.
[Results](results.json) · [Guide](guide.json) · [Original patch](candidate.patch)

The first assessment used restrictive verifier file modes and failed patch
inventory equality. Reassessment of the same sealed candidate used normal
source modes. An initial guide invocation used the wrong executable basename.
Both operator errors and the correction are retained. No model calls or
candidate changes occurred during correction. The original raw receipts are
retained in the local R25 archive, identified by their hashes.
[Validation history](validation.json)

## Compression reviewed after R25

A fresh review identified an ambiguous human-task prefix, fixed overhead on
short tasks and duplicated advisory fields. The revised projection references
the original task only when its prefix identifies exactly one user-role message.
General Observer prompts retain bounded horizon information and reuse native
obligations for reproducible large command enums. Diagnostic and handoff
contexts retain their full horizons. Only recognized stale read headers replace
old source excerpts; ambiguous, cached or mismatched outputs keep diagnostics.

Three captures of the same sealed R25 states show Coder message reductions of
**7.98–11.51%** and Observer prompt reductions of **14.79–16.93%** against
merged main. Tool schemas, native Observer packets and all 50/106/122 retained
call/result messages are unchanged. OpenAI-shaped and Gemini-shaped advisory
controls accept the same authored report; the handoff prompt is identical.
These are model-free byte measurements with zero provider calls and zero tool
executions, not new live success or billed-token measurements. Protected Coder
context still exceeds the soft 64 KiB target.
[Capture results](compression.json)

`bash scripts/check-local.sh` passed the final runtime with 1,148 main Rust
tests (two ignored), six integration tests, 20 TUI replays, 97 Python tests,
80 Web tests, five npm tests, 14 repo-map cases and the Lite smoke. The fresh
review was rechecked after the main fixes; its final narrow header finding
was fixed and covered by regression tests before this full validation.
