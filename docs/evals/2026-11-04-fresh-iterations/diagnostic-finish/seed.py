"""Verify the predecessor, keep its exact candidate bytes, and bind one new human request."""
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parents[3] / "scripts"))
from coupled_bench.common import inventory, read_json, sha, write_json


def seed(prior, out):
    previous = HERE.parent / "diagnostic"
    old = read_json(previous / "plan.json")
    result = read_json(previous / "results.json")
    receipt = read_json(prior / "receipt.json")
    assert sha(prior / "receipt.json") == result["chain_receipt_sha256"]
    assert sha(previous / "plan.json") == receipt["plan_sha256"]
    assert inventory(prior / "run") == receipt["run_inventory"] and not receipt["redacted_files"]
    assert inventory(prior / "workspace", include_nested_git=True) == receipt["final_inventory"]
    assert sha(prior / "composite.patch") == receipt["patch_sha256"] and not receipt["process"]["log_redactions"]
    out.mkdir()
    root = out / "workspace"
    subprocess.run(["git", "clone", "-q", "--no-local", prior / "workspace", root], check=True)
    for args in (["remote", "remove", "origin"], ["checkout", "-q", "--detach", old["base"]],
                 ["apply", "--binary", prior / "composite.patch"], ["add", "-N", "."]):
        subprocess.run(["git", *args], cwd=root, check=True)
    assert inventory(root, include_nested_git=True) == receipt["final_inventory"]
    write_json(out / "initial-inventory.json", receipt["final_inventory"])
    last = read_json(prior / "run/iterations.json")["iterations"][-1]["directory"]
    session = prior / "run" / last / "session.json"
    write_json(out / "seed.json", json.loads(session.read_text().replace(str(prior / "workspace"), str(root))))
    shutil.copyfile(HERE / "task.txt", out / "task.txt")
    original = (root / "tests/tui_replay_catalog.rs").read_bytes()
    assert original.count(b'"--output-dir"') == 1
    formatted = subprocess.check_output(["rustfmt", "--edition", "2021", "--emit", "stdout"],
                                        input=original.replace(b'"--output-dir"', b'"--out-dir"'))
    return dict(base=old["base"], r18_declaration=old["r18_declaration"], r18_seal=old["r18_seal"],
                parent_receipt_sha256=sha(prior / "receipt.json"), parent_session_sha256=sha(session),
                inventory_sha256=sha(out / "initial-inventory.json"), seed_sha256=sha(out / "seed.json"),
                prompt_sha256=sha(out / "task.txt"), preserved_regression_sha256=hashlib.sha256(formatted).hexdigest())


if __name__ == "__main__":
    print(json.dumps(seed(*(Path(x).resolve() for x in sys.argv[1:])), indent=2))
