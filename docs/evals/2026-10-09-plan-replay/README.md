# Plan fields, replay repair diagnostics, and Gemini continuation — 2026-10-09

The native plan parser now recognizes the explicit `Acceptance Criteria:` alias.
Replay syntax errors retain bounded source excerpts; target-ID failures expose
both expected and selected IDs. Recovery keeps supporting diagnostic lines so a
failure count does not displace the actionable cause. Assertions, completion
checks, and iteration caps are unchanged.

Gemini is now exercised alongside the existing weak models. Native and Web
streaming retain Google thought signatures through tool-result history; native
blocked calls, save/load, and retained resume exchanges preserve the original
call. Signed calls are not rewritten. Google requests use `tool_choice: auto`
so explicit protocol text can accompany calls. Google-only synthetic tool rescue
is disabled; ordinary verified text completion remains available, and unfinished
text turns receive a request for a native call. No signature is fabricated.

The final frozen suite remains unqualified for promotion. Mistral's final TUI
run completes its artifact and verification work but fails the required
`patch_file` usage check. GPT and Gemini still have substantive failed tasks.

## Frozen conditions and identities

The six-case [restart spec](../2026-10-03-restart/runtime_eval.json), its checks,
and all 33 fixture hashes are unchanged. Spec SHA-256:
`2edcc57bb2291ed10d0130b9a0997e018e52b0593d0fa576e59bc064a66c5db0`.
The baseline executable is the preceding implementation; intervening commit
`3d385c2` added documentation only. Earlier evidence remains unchanged in
[recovery-focus](../2026-10-09-recovery-focus/README.md).

| Phase | Source commit | Binary SHA-256 |
|---|---|---|
| Baseline | `bed7e03bf7cc1da17784f89e229aa4cdb1c5207c` | `523aff3b26f133df69357a79842435e2c1ddba82e79c820816763b59fbfb3c5c` |
| First candidate | `48598d3f4d6fc2b7ad2212ecba8293f86c18e6de` | `f3f7ceb16f22c9fa3ef69ce2741f1269ac968cc719cb03163beae4ec07d146e9` |
| Second candidate | `50b29a7252f58b0b1d58b6ef3aab1b7d467501cb` | `9cc94209cb740993f73752ba7b8e41756926546f45fc061e97ef3ce137c26db7` |
| Final candidate | `78938559db780d7c83a8b847aee42eea58009f4e` | `6526325be286d305f6f47065e7461193b990ae5ff1dc104ef6c405e088d23fa7` |

Every candidate was frozen before its live run. Conditions: temperature 0,
2,048 output tokens, 90-second request timeout, 900-second outer suite timeout,
case iteration caps 14/14/10/8/12/18, per-copied-workspace Cargo targets. The
baseline precedes each candidate for each existing provider; different providers
can overlap. No suite timed out and no provider retry occurred. The runner hash
matches within every provider. Gemini's separate runner skips incomplete literal
credential assignments without sourcing shell startup files.

## All results, including superseded candidates

| Model | Baseline | First candidate | Second candidate | Final candidate |
|---|---:|---:|---:|---:|
| `mistral-small-2603` | 5/6 | 6/6 | 5/6 | 5/6 |
| `gpt-4.1-mini` | 4/6 | 5/6 | 5/6 | 5/6 |
| `gemini-3.5-flash-lite` | No quality baseline | 4/6 | 2/6 | 3/6 |

[First receipts](receipts-first-candidate.json),
[second receipts](receipts-second-candidate.json), and
[final receipts](receipts.json) retain identities, every check, selected metrics,
raw artifact hashes, diagnostic-state counts, and retained signed-call counts.
Final receipts link prior receipts by hash. Counts of retained metadata are not
independent evidence of repair or completion.

The first Mistral candidate repairs malformed replay JSON, passes the original
automatic test and both explicit commands, and produces the required final
report. The original fixture case remains unchanged. This is a real completed
trajectory, not a reliability guarantee: the second candidate stalls on a
missing failure-like assistant target and unmatched edit anchors. The final
candidate uses `apply_diff`/`write_file`, successfully runs the automatic check
and both explicit commands, and completes the final report. Its only failed
check is `tool_call_seen:patch_file`; that criterion remains enforced.

GPT's baseline damaged-session case omits the required `patch_file` despite
repairing the file. All three candidates pass that case. TUI replay remains
incomplete: the first candidate confuses the selected message ID with a source
path; the second reaches schema/target errors; the final repairs JSON syntax but
ends with a missing `value` field. It also changes the original fixture's
`observer_response` during its failed full-file rewrite. The
[post-run artifact audit](tui-artifact-audit.json) records structural preservation
separately from frozen checks; it does not introduce a new pass criterion.

These are small, stochastic trajectories across source revisions, not matched
random seeds or repeated qualification of one immutable candidate. Aggregate
scores do not establish improvement, nonregression, equivalence, or speed.

## Gemini access, transport, and the two corrections

A bounded preflight for `gemini-2.5-flash-lite` returned HTTP 404 for this
credential. The provider suggested `gemini-3.5-flash-lite`, which was available.
The first 3.5 probe called a tool but rejected the following request when its
thought signature was dropped. Preserving the opaque field completed that
round trip. Separate two-request probes found that `required` omitted the
requested plan prelude, while `auto` returned the prelude and the same tool call.
Sanitized preflight receipts are included in every receipt document. Provider
reference: [Google's OpenAI compatibility guide](https://ai.google.dev/gemini-api/docs/openai).

The first native candidate still failed benchmark resume with HTTP 400: a
textual `<default_api:exec>` response triggered the existing runtime's synthetic
unsigned `exec`. The first correction intercepted every Google text-only turn
before synthesis, which removed the HTTP error but also skipped legitimate
no-tool completion handling. This overbroad guard is preserved as the second
candidate, including its 2/6 result.

The final correction disables only synthetic actions and places native-call
feedback after ordinary completion gates. All final trace error counts are zero;
Gemini's benchmark resume now passes every frozen check, including fresh exact
execution proof and final reporting. The model passes file creation and Rust
repair too. It still fails damaged-session repair after protocol/inspection
stalling, TUI artifact repair, and the self-fix final handoff. The latter run
leaves a failing automatic check after an extra follow-up edit. No live failure
is converted to success by raising caps, substituting models, or dropping checks.

## Deterministic verification and remaining scope

`bash scripts/check-local.sh` passes: 641 Rust unit tests (1 ignored), 1
integration test, TUI replay 3/3, Python 12, Web 53, repo-map 14/14,
JavaScript/shell syntax, and Lite asset/status/git smoke. Ten new regressions
cover the real spaced plan, bounded source/target diagnostics, retained repair
context, indexed signatures, malformed metadata, native/Web SSE forwarding,
blocked session round trips, provider switches, and human task preservation
when requesting a native tool call.

Next work remains reproducible replay construction and edit-anchor recovery,
Gemini protocol compliance and final-handoff closure, and recovery classification
when a preliminary replay command is treated as an action before the intended
edit. Native/Web plan-parser parity, CLI auto-review/TUI handoff boundaries,
and read-only/Web final-handoff parity are not established here.

Custom proxy hosts do not receive Google continuation metadata. Legacy unsigned
sessions are not retroactively certified. Published evidence excludes raw
transcripts, signature values, credentials, and machine-local paths; local
resumable session files necessarily retain provider continuation data.
