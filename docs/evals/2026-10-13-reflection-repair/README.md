# Reflection and replay-input recovery — 2026-10-13

This continues the [four-cycle record](../2026-10-13-recovery-cycles/README.md).
The same three weak models use the same six-case spec, 33 fixture hashes,
acceptance checks and iteration caps. All three new candidates and all nine
attempted live suites are retained: 54 cases, with no suite timeout, run error,
reported transport error or provider retry.

## Repairs

- Reflection alignment compares complete execution-relevant arguments: patch
  search/replacement, write content, diff, full path, command and requested cwd.
  JSON formatting, ignored fields, call IDs and opaque provider signatures do
  not change operation identity. Identical retries still require a strategy
  change. Revised arguments grant no diagnosis, repair or completion proof.
- Reflection retry guidance obtains the accepted `last_outcome` enum from the
  shared contract. The prior free-text example was rejected by its own parser.
- Early replay input failures preserve the spec, case and original cause before
  a report exists. Workspace-relative input identity survives long absolute
  workspace prefixes. Empty selections report the filter and bounded existing
  IDs; no check runs and no report or verification result is fabricated.
- Replay schema feedback lists up to four independent invalid check positions
  together, one matching enum shape and the accepted kinds/value requirements.
  Additional positions are counted as omitted. Input identity precedes long
  parse errors; feedback fits the existing four-line automatic-test diagnostic
  budget. Invalid input remains invalid and assertions are never rewritten.

## Deterministic validation

The old reflection implementation fails two payload/command identity
counterexamples. A mock HTTP loop and local CLI probes distinguish an identical
retry from a revised patch, keep diagnosis mandatory, and preserve the absence
of verification. The initial HTTP fixture required separate corrections to its
evidence and protocol prerequisites; that initial failure is not counted as a
reproduction of the identity collision. The outcome-feedback test runs every
suggested enum through the actual parser.

Replay tests exercise real input/selection errors, automatic-test rendering,
history compaction and restored recovery. An unrelated successful `pwd` does
not discharge the pending input repair. The existing diagnostic-retention test
caught an intermediate extra-line regression; input identity and cause now
share a line instead of displacing target-selection guidance. Multiple-invalid-
check tests retain their positions, shape and omitted count through compaction
without changing the source or granting a pass.

Eight regressions were added in these three candidates. Final
`bash scripts/check-local.sh` passes: **667 Rust tests (1 ignored), 1 integration,
TUI replay 3/3, Python 12, Web 53, repo-map 14/14, syntax checks and Lite smoke**.
The passing log for each candidate and the intermediate failed check-log hashes
are retained separately in the receipts.

## Live results

| Fixed model | Round 1 | Round 2 | Round 3 |
|---|---:|---:|---:|
| `mistral-small-2603` | 6/6 | 5/6 | 5/6 |
| `gpt-4.1-mini` | 5/6 | 5/6 | 5/6 |
| `gemini-3.5-flash-lite` | 3/6 | 3/6 | 3/6 |

The preceding record ended at 6/6, 5/6 and 4/6 respectively. These new runs do
not show uniform improvement: failure locations change, and Gemini's total is
lower. GPT completes the TUI construction case in round 3, but loses a different
criterion elsewhere. All candidates differ; there is no unchanged final-
candidate repeat, fixed random seed, causal comparison or nonregression claim.

| Round | Evaluated source commit | Binary SHA-256 |
|---|---|---|
| 1 | `493447a2a6ebd62ef5555d86acbf863de3624650` | `cd7f6d764555157bc75e2e4437422223bd86fcd66927c981d8aa5a4804ef63b4` |
| 2 | `c63a10e0cccd5915d2b2fe2903654cfa2f098b78` | `2f6931b690a085ea22faac7a0a5660d58986faccbc888360eadfd0735848a834` |
| 3 | `a1c797964f5110a36ef567a89cfbe856c7b90fe6` | `91393e8ccc6e73c75df295c5499c0e953c78737ba7542add8309e1012c1f9cb7` |

Following commits only publish documentation and receipts. The unchanged spec
SHA-256 is `2edcc57bb2291ed10d0130b9a0997e018e52b0593d0fa576e59bc064a66c5db0`;
the unchanged local runner SHA-256 is
`72d1ba53a412720a4295f5119dfe4cb1dda314a0b93628a9a9ec399edf3ac3ac`.
Each case receives a fresh copied workspace and its own Cargo target. Conditions
remain temperature 0, maximum output 2048 tokens, request timeout 90 seconds,
outer timeout 900 seconds per suite, and case caps 14/14/10/8/12/18. Tool choice
remains `required` for Mistral/OpenAI and `auto` for Gemini. Provider calls can
overlap within a round; candidates are evaluated sequentially.

## Retained failures

- **Round 1:** GPT's TUI edits progress from bad anchors and invalid JSON to a
  case without a failure-like assistant target. Its early target-selection
  error loses the input spec, prompting the round 2 repair. Gemini's damaged
  resume follows generic diagnosis into `git status` in a non-Git fixture; its
  TUI run selects a case before adding it, then searches nonexistent source
  paths. Its self-fix run alternates protocol rejection and discovery without
  editing. No required final handoff is produced in those failed cases.
- **Round 2:** Mistral completes and verifies TUI work using `write_file`; only
  the frozen required `patch_file` criterion fails. GPT repairs unknown
  `file_includes` and `replay_filter` kinds in separate turns, then runs out of
  budget at the missing target; this motivates the round 3 aggregated schema
  diagnostics. Gemini's damaged resume passes, but the proof-resume case
  extends an exact required command with `&& cat verification_receipt.txt` and
  does not complete. Its TUI run creates a source file instead of editing the
  replay input. Its self-fix run obtains an automatic-test pass but omits the
  required final report before the cap.
- **Round 3, Mistral:** TUI passes. Self-fix stalls after four observation tool
  executions and no successful mutation or current verification. The refuted
  wording “The file `src/tui/agent/session_bridge.rs` exists in the project.”
  falls outside the bounded existence parser; the overlap fallback blocks edits
  to a separate registry file. This is a new counterexample to the earlier
  bounded repair. A final planning message contains the requested path/command
  strings, so those literal checks pass; this is not a completed handoff.
- **Round 3, GPT:** TUI passes with the original case preserved. Damaged resume
  completes using `write_file`, and only the required `patch_file` criterion
  fails. The criterion is retained without calling the artifact broken or
  coercing a fixture-specific tool choice.
- **Round 3, Gemini:** Proof resume repeats the extended-command failure; a
  subsequent first check does not supply the missing exact second check or
  final report. TUI adds a case with no completed failure-like assistant target,
  receives the corrected input/target diagnostic, and reaches its cap with the
  attempted repair still blocked by diagnosis. Self-fix patches Rust, but its
  automatic test fails because follow-up work is unfinished. Generic diagnosis
  and progress pressure then conflict over reading/editing the documentation;
  there is no current automatic-test pass or final report in this run.

All retained reflection/action-mismatch counts are zero in these live runs.
This does not establish that every provider exercised the repaired boundary;
the deterministic loop supplies that evidence. Gate counts describe retained
messages, not every transition. Seeded history is not counted as new execution
or fresh proof.

## Artifact audit

All nine generated TUI specs parse as JSON. Round 1 and round 2 Gemini preserve
only the original case: preservation alone does not mean the requested case
was built. The other seven specs contain two cases.

The strict original-object audit is true in eight runs. Round 3 Mistral adds
explicit `null` values for the existing case's `lang`, `reason_hint`, `selector`
and `tool_root`, so strict JSON-object equality is false. No other field in that
case changes. These four fields are `#[serde(default)] Option<String>` in
`TuiReplayCase`: omitted and null both deserialize to `None`. The original
prompt, messages, Observer response and checks remain equal. Record the strict
false result and this interpretation separately; it is not evidence of an
existing behavioral regression. No audit result changes a frozen case score.

**Draft qualification remains red.** Deterministic repairs and small fixed-suite
results do not establish general reliability or a model ranking. Historical
failures, required tool-use checks and authored final reporting remain part of
the record.

[Receipts](receipts.json) retain every check and source/binary/spec/fixture
identity. The [failure ledger](failure-modes.json) links the prior observations
to repairs and remaining counterexamples. Raw transcripts, credentials, opaque
signature values and machine-local paths are not published.
