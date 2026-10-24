# Gemini Argon unindexed stream protocol

A frozen same-model Coder + Observer panel on the preceding runtime stopped at
its first request in each of three Argon tasks. The error was `streamed tool-call
id changed at index 0`. Each episode spent one Coder request, retained in the
workspace work ledger, but the older driver exited before writing report.json.
This is a transport failure; it does not measure Argon coding ability.

One separate minimal provider request asked for two read_file calls. The
[sanitized protocol receipt](protocol-probe.json) records two one-call SSE deltas,
both without an index, each with a distinct explicit ID and complete arguments.
The original SSE is retained locally by SHA-256; opaque continuation data is not
published. This probe is one additional request, separate from task episodes.

The decoder now uses explicit IDs for an ID-only stream and retains first-seen
call order. Indexed conflicts, duplicate IDs across indices, ambiguous anonymous
parallel fragments and mixed multi-call encodings still fail before any merged
call can execute. Thought signatures remain bound to their originating call.
The coupled driver now writes the incomplete report, session and consumed budget
before returning an orchestration error.

Validation: 809 Rust tests passed, one ignored; one integration test passed;
TUI replay 4/4. Regression tests reproduce two unindexed calls, attributed partial
arguments, ambiguous continuations, signature ownership, duplicate-index rejection
and an error on the first stream that must retain its request receipt.
A new matched Gemini panel is required before drawing any task-quality conclusion.
