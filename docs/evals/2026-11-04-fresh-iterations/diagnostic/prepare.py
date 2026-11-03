"""Seed an explicit human follow-up from the previous sealed production candidate."""
import json
from pathlib import Path
import shutil
import subprocess
import sys

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[3]
sys.path.insert(0, str(REPO / "scripts"))
from coupled_bench.common import inventory, read_json, sha, write_json


def prepare(prior, out):
    old = HERE.parents[1] / "2026-11-03-coupled-iterations/acceptance"
    plan, result = read_json(old / "plan.json"), read_json(old / "results.json")
    receipt = read_json(prior / "receipt.json")
    assert sha(prior / "receipt.json") == result["chain_receipt_sha256"]
    assert sha(old / "plan.json") == receipt["plan_sha256"]
    assert not receipt["redacted_files"] and not receipt["process"]["log_redactions"]
    assert inventory(prior / "run") == receipt["run_inventory"]
    assert inventory(prior / "workspace", include_nested_git=True) == receipt["final_inventory"]
    assert sha(prior / "composite.patch") == receipt["patch_sha256"]
    out.mkdir()
    root = out / "workspace"
    subprocess.run(["git", "clone", "--no-local", "-q", str(prior / "workspace"), str(root)], check=True)
    for args in (["remote", "remove", "origin"], ["checkout", "-q", "--detach", plan["base"]],
                 ["apply", "--binary", str(prior / "composite.patch")], ["add", "-N", "."]):
        subprocess.run(["git", *args], cwd=root, check=True)
    assert inventory(root, include_nested_git=True) == receipt["final_inventory"]
    write_json(out / "initial-inventory.json", receipt["final_inventory"])
    manifest = read_json(prior / "run/iterations.json")
    session = prior / "run" / manifest["iterations"][-1]["directory"] / "session.json"
    write_json(out / "seed.json", json.loads(session.read_text().replace(str(prior / "workspace"), str(root))))
    shutil.copyfile(HERE / "task.txt", out / "task.txt")
    return dict(r18_declaration=plan["r18_declaration"], r18_seal=plan["r18_seal"], base=plan["base"],
                parent_receipt_sha256=sha(prior / "receipt.json"), parent_session_sha256=sha(session),
                seed_sha256=sha(out / "seed.json"), inventory_sha256=sha(out / "initial-inventory.json"),
                prompt_sha256=sha(out / "task.txt"))


if __name__ == "__main__":
    print(json.dumps(prepare(*(Path(v).resolve() for v in sys.argv[1:])), indent=2))
