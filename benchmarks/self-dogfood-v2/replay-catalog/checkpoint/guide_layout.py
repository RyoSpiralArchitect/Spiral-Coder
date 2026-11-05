"""Move the authored catalog block without changing its content or the original guide."""

import re

SECTION = re.compile(r"(?ms)^## Read-only replay catalog[ \t]*\n.*?(?=^## |\Z)")
DUPLICATE = "## Completion and errors\n"


def original_lines_preserved(original, document):
    sections = list(SECTION.finditer(document))
    if len(sections) != 1:
        return False
    outside = SECTION.sub("", document, count=1)
    return [line for line in outside.splitlines() if line.strip()] == [
        line for line in original.splitlines() if line.strip()
    ]


def place_at_end(original, document):
    matches = list(SECTION.finditer(document))
    if len(matches) != 1:
        raise ValueError("Expected exactly one catalog section")
    section = matches[0]
    suffix = document[section.end() :]
    if not suffix.startswith(DUPLICATE):
        raise ValueError("Expected the observed duplicate completion heading")
    outside = document[: section.start()] + suffix[len(DUPLICATE) :]
    if [line for line in outside.splitlines() if line.strip()] != [
        line for line in original.splitlines() if line.strip()
    ]:
        raise ValueError(
            "Other guide text differs; do not apply the bounded placement change"
        )
    relocated = (
        original
        + ("\n" if original.endswith("\n") else "\n\n")
        + section[0].rstrip("\n")
        + "\n"
    )
    assert relocated.startswith(original) and original_lines_preserved(
        original, relocated
    )
    return relocated
