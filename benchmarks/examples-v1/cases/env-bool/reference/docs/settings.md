# Boolean settings

`parse_bool(value, default=False)` accepts `true`, `1`, `yes`, `on` and
`false`, `0`, `no`, `off`, ignoring case and surrounding whitespace.
None, empty and whitespace-only strings return the supplied default.
Any other string raises ValueError.

Examples: `parse_bool("OFF") -> False` and `parse_bool("", default=True) -> True`.

Check: `python3 -B -S -m unittest discover -s tests -v`.
