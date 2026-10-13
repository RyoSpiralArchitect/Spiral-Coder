# PR #16 fresh review — 2026-10-14

Scope: the entire PR from base `0fe69a84c063d7edf4c9a4b6b8474f25cc1d8ef9`
to head `ef5e2ee98d9d7a452e5b7c10979227f442640515`.

The read-only reviewer was launched without parent conversation history.
Project-local context was temporarily hidden; the reviewer was instructed not
to read backups, user memories or prior conversations. This is best-effort
context isolation, not a claim that host-level instructions can be removed.

## Fresh reviewer output

The following preserves the review, with workspace links made repository-relative.

> Read-only review complete. **No proven merge-relevant regressions found** in
> the reviewed base-to-head changes (`0fe69a8` → `ef5e2ee`).
>
> Reviewed the full change inventory and substantive source/config changes,
> following recovery obligations, failed-edit history, reflection identity,
> pruning/resume, exact verification proof, protocol validation, Observer
> execution evidence, advisory repair/staleness, provider signatures and both
> streaming consumers. In particular, persisted recovery restoration and
> pruning protection are implemented at
> [recovery_focus.rs:253](../../src/tui/agent/recovery_focus.rs#L253) and
> [recovery_focus.rs:329](../../src/tui/agent/recovery_focus.rs#L329); task
> provenance is removed from provider payloads at
> [task_origin.rs:45](../../src/task_origin.rs#L45).
>
> Tests actually run, all passed:
>
> - `cargo test --locked --offline --all-targets recovery_focus -- --test-threads=1` — **15 passed**
> - `cargo test --locked --offline --all-targets observer -- --test-threads=1` — **52 passed**
> - `cargo test --locked --offline --all-targets native_and_web_streams -- --test-threads=1` — **1 passed**
>
> Each used the existing external Cargo target directory,
> `CARGO_INCREMENTAL=0`, `CARGO_PROFILE_DEV_DEBUG=0`, and
> `CARGO_PROFILE_TEST_DEBUG=0`. No live provider calls or tracked-file changes.
>
> Limitations, **not new PR findings**:
>
> - Browser end-to-end handling of tool calls accompanied by
>   `finish_reason:"stop"` remains unverified. The existing Web consumer
>   dispatches only `"tool_calls"` at [web/app.js:7934](../../web/app.js#L7934);
>   the server forwards the upstream reason. This restriction predates this
>   PR, so I have not reported it as an introduced regression.
> - I did not run the entire test suite, interactive TUI/browser sessions, or
>   independently reproduce the evaluation receipts. The large receipt datasets
>   were not audited record by record, and their reported outcomes were not used
>   as proof of correctness.
> - Passing targeted tests establishes deterministic coverage of these
>   boundaries, not cross-provider task-quality nonregression.

## Stateful operator annotation

The hidden context file was restored immediately after the reviewer returned;
its SHA-256 matched the pre-review value, and the working tree was clean.
There were no actionable new regressions to fix. The reviewed head also passed
both Linux and macOS model-free CI in
[run 38431104373](https://github.com/RyoSpiralArchitect/Spiral-Coder/actions/runs/38431104373).
Changes after the reviewed head record this review and clarify historical Draft
wording only; the runtime remains unchanged.

The collaborator authorized merging after review. The merge decision concerns
the reviewed runtime changes and deterministic checks. It does not change the
[bounded live evaluation](../evals/2026-10-14-advisory-recovery/README.md),
its failed criteria, or its qualification status. Coder remains 5/6 on each
fixed weak model in the final recorded panel. Initial Observer GPT results
remain 5/6; sampled corrections are a separate panel. TUI completion/provenance
failures and Web receipt parity remain open follow-up work. Final PR-head CI
must pass before merge.
