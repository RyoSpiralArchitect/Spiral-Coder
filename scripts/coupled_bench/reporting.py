"""Bounded assertions in authored prose, not keyword mentions or inferred proof."""
import re


def _ends_field(text):
    return (not text or text.startswith(("\n", "\r"))
            or (text[0] in ".!?;," and (len(text) == 1 or text[1].isspace())))


def _starts_field(text):
    prefix = text.rsplit("\n", 1)[-1].strip()
    if prefix in ("", "-", "*"):
        return True
    if text and text[-1].isspace() and text.rstrip().endswith((".", "!", "?", ";", ",")):
        return True
    for label in ("Recorded outcomes:", "Verification results:"):
        if text.rstrip().endswith(label):
            earlier = text.rstrip()[:-len(label)]
            if not earlier.strip() or earlier.endswith("\n") or (
                    earlier[-1:].isspace() and earlier.rstrip().endswith((".", "!", "?", ";"))):
                return True
    return False


def states_outcome(summary, label, status):
    """Keep canonical labels; reject e.g. 'After: PASSED is NOT claimed'.

    Native proof and exact-command checks remain separate mandatory gates.
    This deliberately does not grade arbitrary natural-language equivalence.
    """
    pattern = rf"\b{re.escape(label)}[ \t]*:[ \t]*{re.escape(status)}\b"
    for match in re.finditer(pattern, summary, re.I):
        if not _starts_field(summary[:match.start()]):
            continue
        tail = summary[match.end():].lstrip(" \t")
        if tail.startswith("—"):
            tail = tail[1:].lstrip(" \t")
            if not tail.startswith("`") or "`" not in tail[1:]:
                continue
            _, tail = tail[1:].split("`", 1)
            tail = tail.lstrip(" \t")
        if _ends_field(tail):
            return True
    return False
