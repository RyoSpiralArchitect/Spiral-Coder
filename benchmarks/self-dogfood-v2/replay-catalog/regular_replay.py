"""Hold the pre-existing replay selection, failure, and diagnostics to their contract."""
import json
from pathlib import Path
import subprocess
import sys

binary, receipts = map(Path, sys.argv[1:])
stock = json.loads(Path(".spiral-coder/tui_replay.json").read_text())
options = ["--filter", "ObSeRvEr", "--max-cases", "2"]
expected = [r["id"] for r in stock["cases"]
            if "observer" in r["id"].lower() or any("observer" in t.lower() for t in r["tags"])][:2]
assert len(expected) == 2
passed = receipts / "regular-pass"
subprocess.run([binary, "tui-replay", *options, "--out-dir", passed], check=True)
report = json.loads((passed / "report.json").read_text())
assert report["summary"]["passed_ids"] == expected and report["summary"]["failed_ids"] == []
assert [r["id"] for r in report["cases"]] == expected
failure = dict(stock["cases"][0], id="stock-check-failure",
               checks=[{"kind": "observer_contains", "value": "ABSENT_SENTINEL_49"}])
spec = receipts / "failing.json"
spec.write_text(json.dumps({"cases": [failure]}))
failed = receipts / "regular-fail"
bad = subprocess.run([binary, "tui-replay", "--spec", spec, "--out-dir", failed], capture_output=True, text=True)
assert bad.returncode and "stock-check-failure" in bad.stderr and "ABSENT_SENTINEL_49" in bad.stderr, bad.stderr
assert json.loads((failed / "report.json").read_text())["summary"]["failed_ids"] == ["stock-check-failure"]
empty = subprocess.run([binary, "tui-replay", "--filter", "no-such-case", "--out-dir", receipts / "regular-empty"],
                       capture_output=True, text=True)
assert empty.returncode and "0 cases" in empty.stderr and "available_case_ids" in empty.stderr, empty.stderr
print("Verified existing replay order, pass and fail reports, and the empty-filter diagnostic.")
