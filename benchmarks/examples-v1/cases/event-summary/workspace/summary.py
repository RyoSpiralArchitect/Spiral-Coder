"""Count event levels in a JSON Lines export."""
import json


def summarize(lines):
    counts = {"info": 0, "warning": 0, "error": 0}
    for line in lines:
        event = json.loads(line)
        counts[event["level"]] += 1
    return counts
