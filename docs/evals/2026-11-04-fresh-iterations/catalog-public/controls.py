"""Check both additional commands on separate copies before the live condition."""
import hashlib
import json
from pathlib import Path
import shlex
import shutil
import subprocess
import sys
import tempfile

from prepare import GUIDE_COMMAND, PUBLIC_COMMANDS, REPO
sys.path.insert(0, str(REPO / "scripts"))
from coupled_bench.common import inventory, read_json, sha
from self_dogfood.build_cache import reset_package_cache
from self_dogfood.contracts import apply_overlays, execution_env
from self_dogfood.runner import cache_lock


def controls(prepared, r19, target):
    previous = read_json(r19 / "run-receipt.json")
    assert inventory(r19 / "episode") == previous["episode_inventory"]
    spec = read_json(prepared / "inputs/spec.json")
    rows = []
    with tempfile.TemporaryDirectory() as tmp, cache_lock(target):
        area = Path(tmp)
        env = execution_env(area, target)
        for label in ("r19", "reference"):
            workspace = area / label
            shutil.copytree(r19 / "episode/workspace" if label == "r19" else prepared / "baseline",
                            workspace, ignore=shutil.ignore_patterns(".git"))
            if label == "reference":
                apply_overlays(workspace, prepared / "inputs", spec["reference"])
            reset_package_cache(area, workspace, target, area / f"{label}-reset.json")
            if label == "reference":
                subprocess.run(["cargo", "build", "--locked", "--quiet", "--bin", "spiral-coder"], cwd=workspace, env=env, check=True)
                binary = target / "debug/spiral-coder"
                measured = subprocess.check_output([binary, "--provider", "openai", *GUIDE_COMMAND.split()[1:]],
                                                    cwd=workspace, env=env, text=True).strip()
                with (workspace / "docs/evaluation.md").open("a") as file:
                    file.write(f"\n\x60\x60\x60console\n$ {GUIDE_COMMAND}\n{measured}\n\x60\x60\x60\n")
            checks = []
            for command in PUBLIC_COMMANDS:
                proc = subprocess.run(shlex.split(command), cwd=workspace, env=env, capture_output=True, timeout=120)
                checks.append(dict(exit=proc.returncode, stdout_sha256=hashlib.sha256(proc.stdout).hexdigest(),
                                   stderr_sha256=hashlib.sha256(proc.stderr).hexdigest()))
            assert all((x["exit"] == 0) == (label == "reference") for x in checks), (label, checks)
            rows.append(dict(condition=label, results=checks))
    return dict(r19_receipt_sha256=sha(r19 / "run-receipt.json"), checks=rows, external_calls=0,
                reference_guide_example="Measured on the unchanged reference binary and appended solely to its disposable positive control.")


if __name__ == "__main__":
    print(json.dumps(controls(*(Path(x).resolve() for x in sys.argv[1:])), indent=2))
