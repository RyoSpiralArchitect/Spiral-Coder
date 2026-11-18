"""Check that inspection reproduces native counts and preserves archived run files."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile


def inventory(root):
    entries = {}
    for path in sorted(root.rglob("*")):
        name = path.relative_to(root).as_posix()
        if path.is_symlink():
            entries[name] = ["symlink", os.readlink(path)]
        elif path.is_file():
            entries[name] = ["file", hashlib.sha256(path.read_bytes()).hexdigest()]
        else:
            entries[name] = ["directory"]
    return entries


def inspect(binary, root):
    before = inventory(root)
    manifest = json.loads((root / "iterations.json").read_bytes())
    assert manifest["stop_reason"] and manifest["active_iteration"] is None
    assert manifest.get("interrupted_iteration") is None
    task = wire = 0
    for row in manifest["iterations"]:
        ledger = json.loads((root / row["directory"] / "state/tui-work.json").read_bytes())
        for request in ledger["requests"]:
            charge = request.get("charge", "task")
            assert charge in ("task", "transport")
            task += int(charge == "task")
            wire += 1
    assert [task, wire] == [manifest["task_requests"], manifest["wire_requests"]]
    with tempfile.TemporaryDirectory() as home:
        env = {"PATH": os.defpath, "HOME": home}
        if os.name == "nt":
            env["SystemRoot"] = os.environ["SystemRoot"]
        proc = subprocess.run([binary, "coupled-status", "--run-dir", root], capture_output=True,
                              text=True, env=env, timeout=20)
    try:
        data = json.loads(proc.stdout)
        count = [data["requests"][field] for field in ("task_used", "wire_used")]
        pairs = [(row["iteration"], row["task_id"]) for row in data["iterations"]]
        sensible = (all(type(n) is int for n in count) and count == [task, wire]
                    and pairs == [(row["iteration"], row["task_id"]) for row in manifest["iterations"]]
                    and data["status"] == ("completed" if manifest["completed"] else "stopped")
                    and data["stop_reason"] == manifest["stop_reason"] and data["pending"] is None)
    except (KeyError, TypeError, ValueError):
        sensible = False
    unchanged = inventory(root) == before
    return {"passed": proc.returncode == 0 and sensible and unchanged, "exit": proc.returncode,
            "inventory_unchanged": unchanged, "file_count": sum(v[0] == "file" for v in before.values()),
            "iterations": len(manifest["iterations"]), "task_requests": task, "wire_requests": wire,
            "manifest_sha256": before["iterations.json"][1],
            "stdout_sha256": hashlib.sha256(proc.stdout.encode()).hexdigest(), "stderr": proc.stderr.strip()}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("archives", type=Path, nargs="+")
    args = parser.parse_args()
    binary = args.binary.resolve()
    results = [inspect(binary, root.resolve()) for root in args.archives]
    print(json.dumps({"binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(), "archives": results,
                      "provider_calls": 0}, indent=2))
    raise SystemExit(int(not all(row["passed"] for row in results)))
