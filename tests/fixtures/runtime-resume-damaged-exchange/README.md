# Interrupted-session repair fixture

The Rust test starts with a one-character greeting bug. `interrupted_session.json` ends with an assistant tool call and a tool result carrying the wrong call ID. Resume must discard the complete broken exchange, preserve the valid prefix, and finish the smallest tested source change.
