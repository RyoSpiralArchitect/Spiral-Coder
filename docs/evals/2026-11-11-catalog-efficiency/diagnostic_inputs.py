"""Prepare a prospective guide diagnostic without changing its predicates."""
import json
from pathlib import Path
import runpy
import sys

HERE = Path(__file__).resolve().parent


def guide_command():
    commands = json.loads((HERE / "plan.json").read_bytes())["additional_native_checks"]
    (command,) = [c for c in commands if "assert text.count(marker)==1" in c]
    return command


def explain_failure(command):
    for old, new in [
        (
            "assert text.count(marker)==1;",
            'assert text.count(marker)==1, f"docs/evaluation.md: expected one guide command marker, found {text.count(marker)}. Add or correct the measured guide example using candidate output above.";',
        ),
        (
            'assert shown==actual and actual["total"]>0 and actual["total"]==len(actual["cases"])',
            'assert shown==actual, {"error":"docs/evaluation.md example differs from measured candidate output","guide_example":shown,"candidate_output":actual}; assert actual["total"]>0 and actual["total"]==len(actual["cases"]), "candidate output has an invalid case count"',
        ),
    ]:
        if command.count(old) != 1:
            raise ValueError("The pinned guide assertion changed")
        command = command.replace(old, new, 1)
    return command


def materialize(out):
    previous = HERE.parent / "2026-11-11-catalog-on-main/next_inputs.py"
    runpy.run_path(str(previous))["materialize"](out)
    prompt = out / "prompt.txt"
    task = prompt.read_text()
    original = guide_command()
    updated = explain_failure(original)
    literal = json.dumps(original)
    if task.count(literal) != 1:
        raise ValueError("Expected exactly one pinned guide command")
    prompt.write_text(task.replace(literal, json.dumps(updated), 1))
    return updated


if __name__ == "__main__":
    print(materialize(Path(sys.argv[1]).resolve()))
