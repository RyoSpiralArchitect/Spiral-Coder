# Harness continuation and edit recovery — 2026-10-08

This change repairs two deterministic mechanisms: a runtime continuation must not
replace the human task contract, and successful diagnostic reads must not erase
unresolved edit failures. It does **not** establish improved live completion or
qualify the branch for runtime promotion. The live suite remains red.

## Frozen comparison

The six cases and their checks are unchanged from
[the restart spec](../2026-10-03-restart/runtime_eval.json). Historical attempts
01–08 remain unchanged. [receipts.json](receipts.json) records identities, fixture
hashes, every passing/failing check, selected metrics, and raw artifact hashes.

| Run | Source commit | Binary SHA-256 |
|---|---|---|
| Baseline | `62a7ef3eca745a8a0710737dd2c68ac34d2d5aee` | `f54a7aafc73658801b82456756d551c5a8ef54767471e2436fc8a8a5d8f7a6fc` |
| Candidate | `37d36ca0d1f916f7ae4c68fd919e3210dbf4926b` | `b556b8810f4ceb38713e21ad184eed9865d3cbf438780e98b09ab11c932a8754` |

Baseline source and merged main `0fe69a84c063d7edf4c9a4b6b8474f25cc1d8ef9`
have the identical Git tree `eb6051ad3cec2906026c97a15eaecf027d9d1a69`.
The candidate was built and frozen before these evidence documents were added.
Spec SHA-256 is `2edcc57bb2291ed10d0130b9a0997e018e52b0593d0fa576e59bc064a66c5db0`.
All 33 fixture file hashes match across the comparison.

Conditions: `mistral-small-2603` and `gpt-4.1-mini`; temperature 0; 2,048 output
tokens; 90-second request timeout; 900-second outer timeout per six-case suite;
case iteration caps 14/14/10/8/12/18. Each copied workspace has its own Cargo
target. For each provider, baseline finished before candidate began; different
providers could overlap. No run timed out. Local credentials were reused without
including values in receipts. Raw local transcripts are represented by hashes,
not published with machine paths.

## Results and their limits

| Provider | Baseline | Candidate |
|---|---:|---:|
| Mistral Small | 4/6 | 4/6 |
| GPT-4.1 mini | 5/6 | 5/6 |

Both versions pass Rust repair, damaged-session repair, benchmark execution-proof
resume, and the final-report follow-up case with both providers. GPT-4.1 mini also
passes exact file creation. Both fail the TUI replay case.

Equal totals do not establish nonregression: the failed checks differ. Baseline
Mistral file creation completes with correct bytes but misses the required
`write_file` call; candidate uses that tool and satisfies the artifact checks but
reaches the 14-iteration cap without completion. Baseline Mistral replay retains
execution/test proof but fails completion and the required `patch_file` call;
candidate also lacks execution/test proof. Those requirements remain enforced.
This single comparison per provider cannot separate sampling variation from a
runtime regression and is not a quality, speed, or promotion claim.

GPT-4.1 mini replay exposes a different failure shape from historical attempt 08:

- Baseline edits succeed, but first corrupt JSON and later leave empty Observer
  suggestions. The last artifact still fails replay semantics.
- Candidate has one failed `apply_diff`, then a successful read and patch.
  Its counter is **1 → 1 → 0** at trace lines 17/23/30: the read preserves the
  failure and the edit clears it, even though the automatic test then fails.
- The candidate patch produces invalid JSON. The automatic test identifies a
  concrete syntax error, but a successful `pwd` advances recovery to Fix and the
  model drifts into unrelated searches. The run reaches its 12-iteration cap.

The new two-failure strategy hint is covered by deterministic regressions, but
was not exercised in this live replay case. Counter correctness is observed;
replay completion is still unresolved. The next grounded recovery investigation
is retaining the file and cause identified by automatic-test diagnostics, so an
unrelated successful command cannot substitute for inspecting that failure.

## Deterministic validation

`bash scripts/check-local.sh` passed with reduced debug information and incremental
compilation disabled: 611 Rust unit tests passed (1 ignored), 1 integration test,
TUI replay 3/3, Python 12 tests, Web 53 tests, JavaScript/shell syntax, repo-map
14/14, and Lite asset/status/git smoke. After reverting an unsafe auto-review
provenance change, all Rust targets and the candidate build passed again.

The 16 additional tests cover session round trips, explicit human steering,
read-only and exact-byte contracts, required final-answer literals, benchmark
verification through pruning, and actual OpenAI-compatible/Mistral HTTP payloads.
Real missing-anchor edits exercise failed edit/read/retry behavior; success,
automatic-test failure, rejected operations, diagnostic/verification commands,
action-command repair, resume, and pruning each retain their distinct meaning.
No success check, iteration cap, or required tool check was weakened.

## Scope still open

- CLI auto-review and TUI automatic handoffs need a task boundary that preserves
  constraints while resetting completion evidence for newly requested fixes.
  Auto-review retains its previous behavior in this patch; tagging it as a mere
  continuation could close the next review round using previous completion proof.
- Read-only completion paths and Web/native final-handoff parity remain separate.
- Legacy user messages without provenance retain their human meaning. The runtime
  does not guess whether old prose was generated internally.
- Live completion failures above are unresolved; these receipts are not an
  approval to merge or promote a runtime policy.
