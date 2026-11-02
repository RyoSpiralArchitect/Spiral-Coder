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

## A trailing patch marker

Some model replies append a bare `*** End Patch` to an otherwise complete
`apply_diff` call. The native tool and approval preview can discard this one
terminal line for a single file. The body must start with `@@` and contain only
hunk headers and explicitly prefixed lines. Every hunk must match unique,
nonblank, whole-line context in sequence. If one fails, the tool reports the
reason and discards the entire preview. A changed edit records the format
repair in its receipt. Approval still checks that the file matches its preview.

A path-bearing patch envelope, multiple trailers, a file header inside the
repaired body, or a bare trailer that could be a literal line in the target
requires an explicit retry. Pass the target in `path` and its hunks in `diff`.
A space, `+`, or `-` before `*** End Patch` marks ordinary file content.
The repair interprets only framing. The caller chooses the implementation and
verifies the resulting code.

## Finding the code behind a CLI name

A literal search for a public name such as `list-runs` can find an inventory
entry while the parser and dispatch mention `ListRuns`. When `search_files`
targets one Rust file, it can show up to four line numbers where that whole
identifier occurs in the same source. The `[navigation]` line sits below the
ordinary search header. Both the Coder's tool result and the Observer's action
excerpt can carry it. The header and file rows still count only literal matches,
including when there are zero.

The lookup covers a lowercase ASCII hyphenated name of at most 64 bytes in an
explicit `.rs` file of at most 512 KiB. It treats text in strings and comments
like other lexical occurrences. Read the surrounding source to identify the
argument declaration and handler, then run the public command with the requested
arguments and inspect its output.
