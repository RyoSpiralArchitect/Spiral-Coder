"""Check that inspection reproduces native counts and preserves archived run files."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
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
    def usage(ledger):
        charges = [r.get("charge", "task") for r in ledger["requests"]]
        assert all(charge in ("task", "transport") for charge in charges)
        return charges.count("task"), len(charges)

    task = wire = 0
    for row in manifest["iterations"]:
        ledger = json.loads((root / row["directory"] / "state/tui-work.json").read_bytes())
        tasks, wires = usage(ledger)
        task += tasks
        wire += wires
    pending = None
    interrupted = manifest.get("interrupted_iteration")
    if interrupted is not None:
        directory = interrupted["directory"]
        assert directory == f"iteration-{len(manifest['iterations']) + 1:02d}"
        file = root / directory / "state/tui-work.json"
        tasks = wires = task_id = in_flight = None
        if file.exists():
            ledger = json.loads(file.read_bytes())
            tasks, wires = usage(ledger)
            task_id = ledger["task_id"]
            in_flight = sum(r["status"] == "in_flight_or_interrupted" for r in ledger["requests"])
            task += tasks
            wire += wires
        else:
            task = wire = None
        assert [tasks, wires] == [interrupted["task_requests"], interrupted["wire_requests"]]
        pending = dict(directory=directory, task_id=task_id, task_requests=tasks,
                       wire_requests=wires, in_flight_requests=in_flight)
    assert [task, wire] == [manifest["task_requests"], manifest["wire_requests"]]
    with tempfile.TemporaryDirectory() as home:
        copy = Path(home) / "run"
        shutil.copytree(root, copy, symlinks=True)
        if inventory(copy) != before:
            raise ValueError("Archive copy differs from the supplied native records")
        env = {"PATH": os.defpath, "HOME": home}
        if os.name == "nt":
            env["SystemRoot"] = os.environ["SystemRoot"]
        proc = subprocess.run([binary, "coupled-status", "--run-dir", copy], capture_output=True,
                              text=True, env=env, timeout=20)
        copy_unchanged = inventory(copy) == before
    try:
        data = json.loads(proc.stdout)
        used = [data["requests"][field] for field in ("task_used", "wire_used")]
        left = [data["requests"][field] for field in ("task_remaining", "wire_remaining")]
        limits = [data["requests"][field] for field in ("task_limit", "wire_limit")]
        expected_limits = [manifest["max_task_requests"], manifest["max_wire_requests"]]
        remaining = [None if n is None else cap - n for n, cap in zip((task, wire), expected_limits)]
        pairs = [(row["iteration"], row["task_id"]) for row in data["iterations"]]
        sensible = (all(n is None or type(n) is int for n in used + left + limits)
                    and used == [task, wire] and left == remaining and limits == expected_limits
                    and pairs == [(row["iteration"], row["task_id"]) for row in manifest["iterations"]]
                    and data["status"] == ("completed" if manifest["completed"] else "stopped")
                    and data["stop_reason"] == manifest["stop_reason"] and data["pending"] == pending)
    except (KeyError, TypeError, ValueError):
        sensible = False
    unchanged = copy_unchanged and inventory(root) == before
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
