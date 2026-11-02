# Declared routing on the shared-order task

The fixed executor preserved the two verification commands when replaying the retained r2 prefixes. The third live pair remained blocked. Its candidate registered a module and wrote the listing implementation, which still referred to a CLI argument type that had not been created. No candidate was adopted. [Runtime PR #51](https://github.com/RyoSpiralArchitect/Spiral-Coder/pull/51) · [Frozen protocol](protocol.md) · [Plan](plan.json) · [Preflight](preflight.json)

| Observation | r2, prior executor | r3, fixed executor |
| --- | --- | --- |
| Shared task, transport and roles | 24 Coder, 8 Observer, 0 transport repairs | 24 Coder, 8 Observer, 0 transport repairs |
| First changed edit | Created catalog file | Registered catalog module, then created the file |
| Changed files | `src/tui_replay/catalog.rs`, `src/main.rs` | `src/tui_replay.rs`, `src/tui_replay/catalog.rs` |
| Recorded heuristic tool rewrites | 3, concerning 2 proposed verification calls | 0 |
| Recorded handoffs | 7 accepted (6 reads and 1 verification) | 7 accepted reads, 1 overridden read; all 8 suggestions had a matching executed read |
| Observer-linked changed edits | 0 | 0 |
| Native terminal call | An explicit blocked report | An explicit blocked report |
| Independent result | Failed to build | Failed to build |

The r3 executor was built from `1f691ebaba9768ea66c23159a8d3929d6f3b63c3`. The candidate started at the same original Git commit as r2. The shared prompt, assembled candidate baseline, reference, probes, driver, budgets and boolean gate rules were identical by hash. Both mandatory exact checks remained pending. R3's only explicit executed shell check was the failing visible reproduction, so it did not present a matching `cargo test 2>&1` opportunity in its own native receipt. The direct runtime evidence comes from the saved-prefix replay.

## Replay of the two verification calls

The offline probe takes the prefixes ending immediately before the two associated call IDs in the sealed r2 session and proposes the original verification at the follow-up and module-discovery stages. The original executor rewrote both to reading `docs/evaluation.md`; the fixed executor kept `cargo test 2>&1` in both cases. One live redirect continued to `src/agent_session.rs`. That second hop did not reproduce from the pruned saved prefix. Normal execution and proof gates were exercised separately by the regression and full test suite. [Probe](routing_probe.rs) · [Reproduction script](replay_routes.py) · [Results](prefix-replay.json)

## Where the candidate stopped

The first edit applied three of five hunks. Only the module declaration remained from that edit in the final diff. The new module initially imported an unavailable selector. Further attempts to share the selector failed, and a later pair of edits added and removed code in the parent. The final candidate used a local selector and still referenced an undefined `crate::TuiReplayListArgs`. The independent build reproduced E0425. The visible test encountered the build error, and stock checks stopped at formatting. With no binary, three binary-dependent checks did not start. The declared historical driver flags these as timeouts with no exit code or log. Its original verdict is preserved. [Native and independent results](results.json) · [Partial patch](candidate.patch) · [Plain-source replay](plain-replay.json) · [Raw digests](receipts.json)

The Coder's blocked report correctly identified both changed paths, the missing type, absent launcher and guide edits, and missing final verification. The pair did not produce a qualified feature.

## Reproduction of the rejected diffs

In a disposable directory, the frozen native file tool reconstructed the parent source by replaying the first partial edit. Each of the next two rejected diffs ended in a bare `*** End Patch` line. Both original requests failed to match. Removing only that terminal line let each diff apply one hunk independently to the same pre-edit bytes. Both resulting files defined `resolve_path` twice. This establishes the formatting cause of those two rejections and a separate implementation error. It does not qualify either edit. A focused next intervention can point out the unsupported trailer and the already-defined helper before asking for another broad read. [Reproduction script](replay_hunks.py) · [Results](hunk-replay.json)

The raw episode is under `~/.cache/spiral-coder/replay-catalog-native-r3-routing-20261101`. All three live results remain independent observations.
