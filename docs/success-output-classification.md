# Successful-command output classification

Exit code zero is necessary for native command success. The shared classifier in
`src/tui/agent/success_output.rs` still detects error records from commands that
return zero, including non-terminating PowerShell errors and shell chains. Explicit exec, goal-check verification, failure restoration and resumed proof
use this same classification.

The classifier ignores only a complete libtest passing record with the shape
`test identifier::path ... ok`. Each identifier is an ASCII letter or underscore
followed by ASCII letters, digits or underscores; a Rust `r#` prefix is allowed.
ANSI SGR styling is removed for this recognition. Arbitrary prose, doctest file
paths, ignored or failed statuses, partial rows and trailing diagnostics do not
qualify. Every other line in stdout and stderr remains subject to the existing
error-marker checks. A passing summary does not exempt a stream or turn a real
error into a success. The reported diagnostic is the matching line itself.

Recognition does not require a complete suite summary: successful native output
is intentionally shortened before persistence. After full raw output has been
classified, preview truncation omits a partial recognized passing row, and the
history digest summarizes an overlong recognized row instead of dropping its
`ok` suffix. This prevents the retained portion of a passing name such as
`initial_baseline_exception` from becoming a failure after handoff. Unrecognized
error text is never removed by these two rendering rules.

These rules classify output. They do not provide behavioral acceptance, prove
test counts or replace independent evaluation. The self-dogfood oracle still
checks complete expected test summaries on separately retained full logs.
