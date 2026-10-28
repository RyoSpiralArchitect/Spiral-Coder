"""Count event levels in a JSON Lines export."""
import json


def summarize(lines):
    counts = {"info": 0, "warning": 0, "error": 0}
    for number, line in enumerate(lines, start=1):
        if not line.strip():
            continue
        try:
            event = json.loads(line)
        except json.JSONDecodeError as error:
            raise ValueError(f"Invalid event at line {number}") from error
        if not isinstance(event, dict) or not isinstance(event.get("level"), str) or event["level"] not in counts:
            raise ValueError(f"Invalid event at line {number}")
        counts[event["level"]] += 1
    return counts
