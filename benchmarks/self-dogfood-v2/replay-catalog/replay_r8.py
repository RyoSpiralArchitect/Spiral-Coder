"""Restore the archived R8 candidate and check its public native and packaged behavior without models."""
import argparse
import json
from pathlib import Path
import re
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[2]
sys.path.insert(0, str(REPO / "scripts"))
from coupled_bench.common import read_json, sha
from self_dogfood.contracts import execution_env, git_snapshot
from self_dogfood.runner import cache_lock

INDEX = REPO / "docs/examples/archives/replay-catalog-r8.json"
PATCH = REPO / "docs/evals/2026-11-02-cli-route/candidate.patch"


def replay(target_dir):
    index = read_json(INDEX)
    for path, digest in index["files"].items():
        if sha(REPO / path) != digest:
            raise ValueError(f"Archived input changed: {path}")
    with tempfile.TemporaryDirectory() as tmp:
        scratch = Path(tmp)
        root = scratch / "candidate"
        target = target_dir or (scratch / "target")
        git_snapshot(REPO, index["baseline_commit"], root)
        subprocess.run(["git", "apply", "--binary", str(PATCH)], cwd=root, check=True)
        env = execution_env(scratch, target)
        with cache_lock(target):
            if target_dir:
                subprocess.run(["cargo", "clean", "--locked", "-p", "spiral-coder"], cwd=root, env=env, check=True)
            subprocess.run(["cargo", "build", "--locked", "--bin", "spiral-coder"], cwd=root, env=env, check=True)
            binary = target / "debug/spiral-coder"
            argv = [binary, "tui-replay-list", "--filter", "PREFS", "--max-cases", "1"]
            output = json.loads(subprocess.check_output(argv, cwd=root, env=env, timeout=20))
            if output != index["filtered_output"]:
                raise ValueError(f"Unexpected native catalog: {output}")
            receipts = scratch / "replay-control"
            receipts.mkdir()
            checks = [
                ("visible", [HERE / "catalog_visible.py"], 0),
                ("held_out", [HERE / "catalog_oracle.py", binary], 1),
                ("packaged", [HERE / "packaged_spiral.py", binary], 1),
                ("ordinary_replay", [HERE / "regular_replay.py", binary, receipts], 0),
            ]
            outcomes = {}
            for name, args, expected in checks:
                run = subprocess.run([sys.executable, "-BS", *args], cwd=root, env=env,
                                     capture_output=True, text=True, timeout=120)
                log = run.stderr + run.stdout
                matches = run.returncode == expected
                if name == "visible":
                    matches = matches and "Ran 1 test" in log and "ok" in log and "OK" in log
                elif name == "held_out":
                    matches = (matches and "Ran 11 tests" in log and "FAILED (failures=1)" in log
                               and re.findall(r"^FAIL: (\w+)", log, re.M) == ["test_usage_is_documented"])
                elif name == "packaged":
                    matches = matches and "Ran 1 test" in log and "AssertionError: spiral routed replay listing to tui" in log
                if not matches:
                    raise RuntimeError(f"{name} differed from the archived outcome (exit {run.returncode})\n{log[-4000:]}")
                outcomes[name] = run.returncode
    return {"episode": index["episode"], "run_receipt_sha256": index["run_receipt_sha256"],
            "native_filtered_output": output, "check_exit_codes": outcomes,
            "matches_archive": True, "qualified_candidate": False, "provider_requests": 0}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target-dir", type=Path, help="dedicated disposable Cargo cache; rebuilds the spiral-coder package")
    opt = parser.parse_args()
    print(json.dumps(replay(opt.target_dir.resolve() if opt.target_dir else None), indent=2))
