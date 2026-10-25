# Follow-up edits and recovery

Required documentation and replay work remain task obligations. Follow-up
coercion must not replace the work needed to diagnose an actual failure.

During `Diagnose`, the original native call reaches the existing recovery gate.
The gate still blocks premature mutations and verification, while diagnostic
reads can run. Native range reads keep their bounds in every phase. A model's
file edit to the required follow-up path keeps its arguments; existing file,
scope, approval and recovery checks still decide whether it executes.

Automatic whole-file follow-up patches require a complete correlated native
read. The runtime validates the path and header counts, strips its cache wrapper
and JSON guidance, and excludes partial ranges, truncation, pruned history and
empty bodies from whole-file anchors. These observations can still guide a
model-authored localized patch. Literal marker-like text inside an otherwise
complete file is ordinary file content.

Cache hits render the path used by the current read call. An absolute/relative
alias is rebound only after both paths resolve to the same safe file identity;
metadata and body bytes stay unchanged. Malformed or mismatched cached headers
fall back to a fresh read instead of manufacturing matching evidence.

A `done` call with nonempty `remaining_acceptance` reaches the existing
completion/report checks instead of becoming a mutation. This does not accept
incomplete work or manufacture verification. Follow-up hints, exact checks and
final-report requirements remain pending until independently met.

Regression tests replay native long-document reads, failed patches, diagnostic
range reads, session roundtrips, compressed history and incomplete closeout.
Ordinary synthesis from complete current reads remains covered.
