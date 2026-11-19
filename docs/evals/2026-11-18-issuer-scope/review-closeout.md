# Independent review of the issuer-scope trial

The blind, read-only review compared PR #104 at `d52dc0d` to `609344c` and
inspected the focused candidate as a separate patch. The original measurements
remain 24/24 on the saved candidate and 17/26 qualification gates on the fresh
run. The reviewed candidate is an archived experiment.

## Fresh reviewer findings

- **P2, harness.** A second range read of the same test file can send the Coder
  back to the Observer for an already diagnosed failure. Changing the rendered
  read resets the comparison count and the first-read flag.
- **P1, candidate.** Before the first finalized iteration, a genuine
  interruption with a missing ledger has null aggregate counts. A readable
  ledger may already have spent requests. The candidate requires zero for both.
- **P2, candidate.** Null interrupted counts can contradict a readable ledger.
  A missing ledger can retain a numeric interrupted wire count. Unknown pending
  usage can mask a known finalized wire-cap violation, and readable pending
  usage can exceed its per-iteration task cap. Unknown charge values can be
  silently counted as transport.
- **P2/P3, candidate tests.** Unix symlink helpers appear in unconditional
  tests. The two-iteration test checks output substrings, which depend on
  whitespace and can also match iteration numbers 10 and 20.

## Operator verification

Two different native read windows after one clipped failure reproduced the
extra handoff. The runtime now refreshes its source observation while yielding
only on the first eligible read. The new regression and all 12 neighboring
diagnostic tests passed. The full local suite and TUI replay passed.

The saved candidate correctly read three genuine archives with credentials
absent. Their independent request totals were 128/143, 51/59 and 152/166. Each
complete archive inventory matched before and after inspection. A new set of
27 writer-state cases exposed 11 failures in the saved candidate. The
independent reference passed all 27. This extends the evidence beyond the
original 24 cases. It also gives the successor task positive and negative
controls for the first interruption and partial usage. [Measurement receipts](review-evidence.json)

The research issue is being closed at the collaborators' request with its
original fresh qualification unmet. Its original baseline, first terminal,
thresholds and candidate snapshots remain available for a fresh run.
