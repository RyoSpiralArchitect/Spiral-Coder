# File edit diagnostics

`patch_file` and `apply_diff` report rejected anchors with numbered excerpts from
the text they inspected. Missing anchors show the closest informative line context
and the file tail when it is outside that context. Punctuation-only anchors show
the tail, so an incorrect closing block does not produce an unrelated file prefix.
An empty file is reported explicitly. Ambiguous exact anchors show up to three
occurrences with surrounding lines so the caller can choose unique context.

These excerpts are diagnostic evidence only. Similarity does not authorize an
edit, choose an ambiguous occurrence, or repair the requested replacement.
`patch_file` retains its existing exact-match and unique whitespace-only multiline
fallback rules. A rejected patch leaves the file unchanged. `apply_diff` retains
its existing per-hunk behavior: accepted hunks can be written while rejected
hunks are skipped. Its diagnostic excerpts are labeled as the input immediately
before that hunk, including any earlier accepted hunks.

The diagnostic helper limits each context window to six lines, each displayed
line to 140 Unicode characters, and ambiguous matches to three three-line
windows. `apply_diff` includes these contexts for at most two rejected hunks.
Truncated lines are marked. The diagnostic explicitly tells callers that line
numbers and truncation markers are display-only and must not be copied into search
text. Read the actual file before constructing a replacement anchor.
