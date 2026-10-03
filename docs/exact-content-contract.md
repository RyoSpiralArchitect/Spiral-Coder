# Exact-content creation contract

The runtime recognizes one bounded request form at the beginning of the original
task: ``Create `path` containing exactly `literal`.`` The keywords are
case-insensitive. The path is limited to 4096 bytes and the literal to 64 KiB.
An empty literal is supported. Literal UTF-8 bytes, whitespace, and backslashes
are preserved; no escape decoding or implicit trailing newline is applied.
Optional following sentences are limited to `Verify it`, `Verify it before you
finish`, and `Final answer must include ...`. Other trailing instructions are
not silently ignored, since they may change the requested contents.

Before accepting `done` or automatically closing a verified action, the runtime
reads the current file and compares its bytes with that literal. It uses the existing tool-root path
resolver and rejects targets that resolve outside the root, including symlinks.
A missing, unreadable, non-file, or mismatched target blocks completion and
provides a correction hint. The check never creates or edits the artifact.
No particular writing tool is required: an exact file produced by `exec` can
satisfy the same contract as one produced by `write_file`.

This check supplements normal verification. A line match such as `grep -Fx`
does not establish byte equality because it can accept an added final newline.
A previous success, a model-authored acceptance claim, or the requested path in
the final answer does not substitute for the current bytes.

The recognizer deliberately declines compound or multiple matching creation
requests, additional same-sentence modifiers, malformed quoting, and prompts
outside this leading syntax. Those tasks retain their existing acceptance
checks; this feature does not claim general natural-language interpretation.
The validator does not inspect evaluation specs, case IDs, or expected scores.
