# Whole-file source replacements

`file_tools/overwrite_guard.rs` checks native `write_file` before creating its
temporary file. For an existing source file with at least 20 distinct nonblank
lines, replacing or removing more than half of those lines requires an explicit
`patch_file` or `apply_diff` instead. Comparison trims each line and ignores blank
lines, so indentation or line-ending changes alone do not trip this check.

The refusal reports the number of affected lines and leaves the file unchanged.
An intentional broad refactor or deletion remains possible through a patch with
old context. Existing approval, read, preservation and verification gates still
apply. The safeguard is shared by native TUI and Web file writes and introduces
no persistent baseline or new completion evidence.

This is a bounded editing heuristic, not a semantic preservation guarantee. It
covers the source extensions listed in the module (Python, Rust, JavaScript,
TypeScript, Go, C-family, JVM, Swift, Ruby, PHP, shells and Lua). Creation, small
files, documentation and other extensions retain their existing behavior. A
legitimate broad rewrite may require a patch; a small but harmful edit, an
explicit large patch or a shell write can still remove functionality. Tests and
independent artifact checks remain necessary. Thresholds and scope must not be
reported as proof that all existing behavior is preserved.
