# Bounded file reads

Reflection compares the requested path and effective line bounds. A corrected
range (including narrowing an oversized request) is a different action. Omitted
defaults normalize to the same bounds, and call IDs or ignored fields cannot
make an identical range look new. This affects retry identity only; it does not
grant whole-file overwrite permission or verification credit.

Coder and Observer share `read_file` arguments in the native tool catalog. Web
forwards the same arguments to `/api/read_file`:

```json
{"path":"src/example.rs","start_line":890,"end_line":930}
```

Lines are one-based and the end is inclusive. When a range is requested, omitted
`start_line` means 1 and omitted `end_line` means `start_line + 199`. Each call
allows at most 200 lines and returns at most 8,000 content characters. The end
clips to EOF; zero, reversed, oversized, non-integer and beyond-EOF starts fail
explicitly. Original newline bytes, including CRLF, are preserved. Large single
lines can still exceed the character cap; use an exact JSON patch or a focused
shell inspection in that case.

Calls without either field retain the original full-read format and truncation
limit. Their truncation hint now directs both roles to ranges. Range headers
identify the actual bounds and mark the result as a partial file. Native evidence
retains both bounds, so distinct reads have distinct action signatures.

A correlated range header can satisfy the existing path-level diagnostic read
obligation, including after pruning/resume. Its bounds must match the original
call; it supplies observation, not verification or whole-file read authority.

Ranges bypass the whole-file cache and read current disk contents. A partial
range or truncated prefix cannot create whole-file overwrite authority, including
after Observer continuation. Use exact `patch_file`/`apply_diff` edits instead.
The Web response's `complete_read` flag controls the corresponding local guard.
This flag is response metadata, not test or completion evidence. No new persistent
state or verification receipt is created by the range mechanism.

Coverage includes Unicode/CRLF/EOF boundaries, native HTTP validation, Observer
suggestion replay, rejection of whole-file overwrite after continuation, and the
Web request/overwrite path. Runtime state and Python cache files are a separate
workspace-cleanliness issue; this change does not filter them from historical
qualification receipts or claim to resolve their ownership.
