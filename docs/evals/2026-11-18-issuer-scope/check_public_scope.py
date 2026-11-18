"""Run a paired public CLI control against a supplied binary and frozen native fixture."""
import argparse
from copy import deepcopy
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile


def check(binary, fixture_file):
    module = importlib.util.spec_from_file_location("status_fixture", fixture_file)
    fixture = importlib.util.module_from_spec(module)
    module.loader.exec_module(fixture)

    def invoke(root, expected):
        process = subprocess.run(
            [binary, "coupled-status", "--run-dir", root],
            capture_output=True, text=True, timeout=20,
        )
        if expected is None:
            passed = process.returncode != 0 and process.stdout == "" and bool(process.stderr.strip())
        else:
            try:
                output = json.loads(process.stdout)
            except ValueError:
                output = None
            passed = process.returncode == 0 and output == expected
        return {"passed": passed, "exit": process.returncode,
                "stdout": process.stdout.strip(), "stderr": process.stderr.strip()}

    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        manifest = fixture.make(root)
        expected = fixture.expected()
        one = invoke(root, expected)

        first = manifest["iterations"][0]
        second = dict(deepcopy(first), iteration=2, directory="iteration-02",
                      task_id="t2", parent_task_id=first["task_id"])
        first.update(completed=False, termination="blocked")
        manifest["iterations"].append(second)
        manifest.update(task_requests=4, wire_requests=6)
        fixture.put(root, "iterations.json", manifest)
        ledger_path = "iteration-02/state/tui-work.json"
        ledger = fixture.work(second["task_id"])
        fixture.put(root, ledger_path, ledger)
        first_ids = [r["id"] for r in json.loads((root / "iteration-01/state/tui-work.json").read_text())["requests"]]
        ids = [r["id"] for r in ledger["requests"]]
        assert ids == first_ids == [0, 1, 2]

        first_row = expected["iterations"][0]
        expected["iterations"].append(dict(deepcopy(first_row), iteration=2, task_id=second["task_id"]))
        first_row["termination"] = "blocked"
        expected["requests"].update(task_used=4, wire_used=6, task_remaining=188, wire_remaining=210)
        two = invoke(root, expected)

        ledger["requests"][1]["id"] = ids[0]
        fixture.put(root, ledger_path, ledger)
        duplicate = invoke(root, None)

    return {
        "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
        "fixture_sha256": hashlib.sha256(fixture_file.read_bytes()).hexdigest(),
        "input_ids": {"first": first_ids, "second_positive": ids,
                      "second_negative": [r["id"] for r in ledger["requests"]]},
        "one_finalized": one, "two_finalized": two, "duplicate_within_second": duplicate,
        "passed": all(row["passed"] for row in (one, two, duplicate)),
    }


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("fixture", type=Path)
    args = parser.parse_args()
    print(json.dumps(check(args.binary.resolve(), args.fixture.resolve()), indent=2))
