"""Prepare a public acceptance request on the preceding sealed workspace."""
import json
from pathlib import Path
import shutil
import subprocess
import sys

HERE=Path(__file__).resolve().parent
sys.path.insert(0,str(HERE.parents[3]/"scripts"))
from coupled_bench.common import inventory,read_json,sha,write_json


def prepare(prior,out):
    old_plan=read_json(HERE.parent/"range-return/plan.json")
    result=read_json(HERE.parent/"range-return/results.json")
    receipt=read_json(prior/"receipt.json")
    assert sha(prior/"receipt.json")==result["chain_receipt_sha256"] and receipt["plan_sha256"]==sha(HERE.parent/"range-return/plan.json")
    assert not receipt["redacted_files"] and inventory(prior/"run")==receipt["run_inventory"]
    assert sha(prior/"composite.patch")==receipt["patch_sha256"] and inventory(prior/"workspace",include_nested_git=True)==receipt["final_inventory"]
    manifest=read_json(prior/"run/iterations.json")
    assert manifest["stop_reason"]=="completed" and manifest["task_requests"]==10
    if out.exists() or out.is_relative_to(prior): raise ValueError("use a fresh external directory")
    out.mkdir();root=out/"workspace";old=prior/"workspace"
    subprocess.run(["git","clone","-q","--no-local",old,root],check=True)
    subprocess.run(["git","remote","remove","origin"],cwd=root,check=True)
    subprocess.run(["git","checkout","-q","--detach",old_plan["base"]],cwd=root,check=True)
    subprocess.run(["git","apply","--binary",prior/"composite.patch"],cwd=root,check=True)
    subprocess.run(["git","add","-N","."],cwd=root,check=True)
    assert inventory(root,include_nested_git=True)==receipt["final_inventory"]
    write_json(out/"initial-inventory.json",receipt["final_inventory"])
    prev=prior/"run"/manifest["iterations"][-1]["directory"]/"session.json"
    write_json(out/"seed.json",json.loads(prev.read_text().replace(str(old),str(root))))
    shutil.copyfile(HERE/"task.txt",out/"task.txt")
    return dict(r18_declaration=old_plan["r18_declaration"],r18_seal=old_plan["r18_seal"],base=old_plan["base"],
        parent_receipt_sha256=sha(prior/"receipt.json"),parent_session_sha256=sha(prev),seed_sha256=sha(out/"seed.json"),
        inventory_sha256=sha(out/"initial-inventory.json"),prompt_sha256=sha(out/"task.txt"))


if __name__=="__main__":
    print(json.dumps(prepare(*(Path(arg).resolve() for arg in sys.argv[1:])),indent=2))
