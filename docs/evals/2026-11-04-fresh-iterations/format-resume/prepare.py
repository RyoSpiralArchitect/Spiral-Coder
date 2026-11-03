"""Restore the sealed source and native session for a same-task formatting continuation."""
import json
from pathlib import Path
import shutil
import subprocess
import sys

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parents[3] / "scripts"))
from coupled_bench.common import inventory, read_json, sha, write_json


def restore(previous, dest):
    record = HERE.parent / "diagnostic-finish"
    old, audit = read_json(record / "plan.json"), read_json(record / "audit.json")
    seal = read_json(previous / "receipt.json")
    assert sha(previous / "receipt.json") == audit["receipt_sha256"] and sha(record / "plan.json") == seal["plan_sha256"]
    assert not seal["redacted_files"] and seal["process"]["log_redactions"] == 0
    assert inventory(previous / "run") == seal["run_inventory"]
    assert inventory(previous / "workspace", include_nested_git=True) == seal["final_inventory"]
    assert sha(previous / "composite.patch") == seal["patch_sha256"]
    dest.mkdir()
    root = dest / "workspace"
    subprocess.run(["git", "clone", "-q", "--no-local", previous / "workspace", root], check=True)
    for args in (["remote","remove","origin"], ["checkout","--detach","-q",old["base"]],
                 ["apply","--binary",previous / "composite.patch"], ["add","-N","."]):
        subprocess.run(["git",*args], cwd=root, check=True)
    assert inventory(root, include_nested_git=True) == seal["final_inventory"]
    write_json(dest / "initial-inventory.json", seal["final_inventory"])
    final = read_json(previous / "run/iterations.json")["iterations"][-1]["directory"]
    session = previous / "run" / final / "session.json"
    write_json(dest / "seed.json", json.loads(session.read_text().replace(str(previous / "workspace"), str(root))))
    shutil.copy2(previous / "task.txt", dest / "task.txt")
    assert sha(dest / "task.txt") == old["prompt_sha256"]
    return dict(parent_receipt_sha256=audit["receipt_sha256"], parent_session_sha256=sha(session),
                seed_sha256=sha(dest/"seed.json"), prompt_sha256=sha(dest/"task.txt"),
                inventory_sha256=sha(dest/"initial-inventory.json"), base=old["base"],
                r18_declaration=old["r18_declaration"], r18_seal=old["r18_seal"],
                preserved_regression_sha256=old["preserved_regression_sha256"])


if __name__ == "__main__":
    print(json.dumps(restore(*(Path(x).resolve() for x in sys.argv[1:])),indent=2))
