"""Change only the pending directory number of an otherwise valid interrupted run."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile


def check(binary, fixture_file):
    spec = importlib.util.spec_from_file_location("status_fixture", fixture_file)
    fixture = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(fixture)
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp) / "run"
        manifest = fixture.make(root, "interrupted")
        def probe():
            r = subprocess.run([binary, "coupled-status", "--run-dir", root],
                               env={"PATH": os.defpath, "HOME": tmp}, capture_output=True, text=True, timeout=15)
            return {"exit": r.returncode, "stdout": r.stdout.strip(), "stderr": r.stderr.strip()}
        valid = probe()
        assert manifest["interrupted_iteration"]["directory"] == "iteration-02" and len(manifest["iterations"]) == 1
        (root / "iteration-02").rename(root / "iteration-03")
        manifest["interrupted_iteration"]["directory"] = "iteration-03"
        fixture.put(root, "iterations.json", manifest)
        gap = probe()
    return {"binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
            "valid": valid, "skipped_directory": gap,
            "passed": valid["exit"] == 0 and gap["exit"] != 0 and not gap["stdout"] and bool(gap["stderr"])}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("fixture", type=Path)
    args = parser.parse_args()
    result = check(args.binary.resolve(), args.fixture.resolve())
    print(json.dumps(result, indent=2))
    raise SystemExit(int(not result["passed"]))
