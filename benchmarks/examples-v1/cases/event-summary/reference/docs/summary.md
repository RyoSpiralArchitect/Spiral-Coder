# Event summary

`summarize(lines)` returns counts for `info`, `warning`, `error`, including zero
counts. It accepts an iterable of strings, skips blank lines and ignores extra
object fields. Levels must use these exact lowercase names.

An empty export returns `{"info": 0, "warning": 0, "error": 0}`.
Invalid JSON, a non-object record or an unknown/missing/non-string level raises
ValueError with the physical line number (including blank lines), such as `line 3`.
Errors are never silently skipped; input strings remain unchanged.

Check: `python3 -B -S -m unittest discover -s tests -v`.
