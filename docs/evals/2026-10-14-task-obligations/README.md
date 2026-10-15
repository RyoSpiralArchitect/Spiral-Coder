# Shared task obligations and Observer recovery — 2026-10-14

This continues [rounds 1–4](../2026-10-14-coupled-loop/README.md). The unit remains
one original task completed by Coder and Observer together, including artifacts,
current native proof, authored reporting and the shared wire cap.

## Repairs

- Explicit approved `target_files` now determines artifact scope. Source paths in
  the objective remain referenced subjects. Coder, TUI Observer and the Rust Web
  endpoint derive the same remaining exact checks and report literals from the
  original request and native exchanges. This bounded parser does not interpret
  arbitrary prose as an approved artifact plan.
- Runtime-admitted plans, including compatibility rescues absent from assistant
  text, survive native session save/load and pruning. They are revalidated on
  resume and stripped from the provider wire schema. A new human request resets
  the plan boundary.
- Rounds 5–6 exposed a second resume defect: a later rejected plan revision could
  hide an earlier valid plan. Resume now searches for the last plan that passes
  the current task contract, matching the uninterrupted loop's retained plan.
- Observer repair now retains concrete JSON/enum parser diagnostics. Previously,
  an invalid `kind: patch_file` reached repair as only a generic parse failure.
  Both UI paths still allow only one repair and never infer or execute an action
  from a malformed answer.
- Historical governor blocks remain execution history; only actual user-rejected
  results populate approval rejections. Observer had cited an old plan-gate
  failure as though it were an outstanding human rejection.

## Fixed repetition and evidence

Before running round 5, both rounds 5 and 6 were declared against commit
`0ebbe4c905797d41eb469f8245152952882eaadf`, binary
`ca881b7b53fd356a24028c7a2d168b8b7056bff56c193acfaf4743252639b649`.
The complete panel ran twice without a candidate or scoring change: **9/18
strict passes**, 130 wire requests, no episode timeout. Runtime completion was
11/18; it does not replace strict qualification.

| Both roles use | Round 5 | Round 6 |
|---|---:|---:|
| `mistral-small-2603` | 0/3 | 1/3 |
| `gpt-4.1-mini` | 2/3 | 2/3 |
| `gemini-3.5-flash-lite` | 2/3 | 2/3 |
| Total | **4/9** | **5/9** |

All models ran the same three cases: damaged-exchange resume, prepared artifact
verification/final reporting, and replay regression addition. The spec hash
remains `3c3c24ddec901a769158b6cab126797bdb2427243387b0b9b315e5e3182e4752`.
Caps remain 10/8/12 shared HTTP attempts, including Observer repair and provider
retries. Temperature is 0, output cap 2048, request timeout 90 seconds and episode
wall cap 600 seconds. Required `patch_file`, exact command and case-sensitive
report assertions remain unchanged. Each replay fixture includes the same
external preservation baseline with extension and nested provenance fields.

Both rounds preserved every old replay JSON value (6/6). Only one of the six
replay artifacts passed both old and new cases: round-6 Mistral. It still failed
strictly because it omitted the required `patch_file`. Round-5 Mistral's source
repair also completed and passed the independent tests but omitted that tool.
GPT and Gemini passed both resume and exact-proof/report tasks in both rounds;
all replay tasks failed strict qualification. Patch anchors/JSON structure,
protocol recovery and reporting still consume the available budget.

These are small, adaptive development fixtures. The repeated pair establishes
what happened under a fixed candidate and conditions; it does not establish
model rankings, general nonregression, or Observer's causal contribution. There
is no Observer-disabled ablation. Historical failed receipts are unchanged.

## Final candidate and checks

After diagnosing the fixed pair, commit
`16bb83c780948956e53817cfc6437d7203e86651` received one further complete panel,
round 7, with binary
`91c171ad5853b53ace4a55b1d50c85179d30a537af3f3661cbb9c5c97ba1fd40`.
This is an adaptive follow-up, separate from the unchanged rounds 5–6 pair.

| Both roles use | Round 7 strict |
|---|---:|
| `mistral-small-2603` | 1/3 |
| `gpt-4.1-mini` | 2/3 |
| `gemini-3.5-flash-lite` | 2/3 |
| Total | **5/9** |

Round 7 uses 59 wire requests, with no timeout or cap violation. All three
exact-proof/final-report tasks pass. All three source-repair artifacts pass the
independent tests; Mistral still omits the fixed `patch_file` requirement. Every
old replay JSON value is preserved (3/3). GPT and Gemini add the requested case,
but neither passes the complete replay; Mistral creates no new case. Thus no
replay task qualifies. Detailed Observer parser errors are now retained in the
live receipts, but Mistral's repair still returns an invalid action kind. Correct
error feedback does not guarantee model compliance or task completion.

The final candidate passes `bash scripts/check-local.sh`: **721 Rust tests
(1 ignored), 1 integration, TUI replay 3/3, Python 15, Web 61, repo-map 14/14**, JS
and shell syntax checks, 11 Lite assets and the Lite end-to-end smoke. Actual
HTTP regressions cover Observer continuation with a runtime-accepted plan and a
newer rejected plan revision, without requiring the next response to replan.
TUI/Web tests cover concrete repair diagnostics and user-rejection classification.
See [verification.json](verification.json) for the log and binary identities.

All **27 episodes and 189 wire requests** from rounds 5–7 are retained, in
addition to the earlier four rounds. The fixed pair is 9/18 strict passes; the
final adaptive panel is 5/9. Neither result establishes a reliability improvement.
The current failure ledger remains [failure-modes.json](failure-modes.json).

## Audit trail

[receipts.json](receipts.json) retains fixed check outcomes, independent oracles,
per-wire usage/duration, role counts, assist receipts and structured action
summaries. Missing provider usage is null. [raw-manifest.json](raw-manifest.json)
hashes local frozen binaries, declarations, sessions, traces, outputs and oracles.
`LOCAL_RAW` resolves to `~/.cache/spiral-coder` on the evaluation host. Raw native
sessions stay local; provider continuation metadata is not published.
