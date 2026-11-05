"""Continue the first sealed attempt without modifying its source or Observer note."""
import json
from pathlib import Path
import shutil
import subprocess
import sys

HERE=Path(__file__).resolve().parent
sys.path.insert(0,str(HERE.parents[3]/"scripts"))
from coupled_bench.common import inventory,read_json,sha,write_json


def prepare(parent, out):
    plan=read_json(HERE.parent/"plan.json")
    results=read_json(HERE.parent/"results.json")
    receipt=read_json(parent/"receipt.json")
    assert sha(parent/"receipt.json")==results["chain_receipt_sha256"] and receipt["plan_sha256"]==sha(HERE.parent/"plan.json")
    assert not receipt["redacted_files"] and sha(parent/"composite.patch")==receipt["patch_sha256"]
    assert inventory(parent/"run")==receipt["run_inventory"] and inventory(parent/"workspace",include_nested_git=True)==receipt["final_inventory"]
    manifest=read_json(parent/"run/iterations.json")
    assert manifest["stop_reason"]=="observer_unavailable" and manifest["task_requests"]==5
    if out.exists() or out.is_relative_to(parent): raise ValueError("use a fresh external directory")
    out.mkdir();root=out/"workspace";old=parent/"workspace"
    subprocess.run(["git","clone","-q","--no-local",old,root],check=True)
    subprocess.run(["git","remote","remove","origin"],cwd=root,check=True)
    subprocess.run(["git","apply","--binary",parent/"composite.patch"],cwd=root,check=True)
    subprocess.run(["git","add","-N","."],cwd=root,check=True)
    assert inventory(root,include_nested_git=True)==receipt["final_inventory"]
    write_json(out/"initial-inventory.json",receipt["final_inventory"])
    prev=parent/"run"/manifest["iterations"][-1]["directory"]/"session.json"
    # Keep all original call IDs, returned bytes, plan, and the retained R18 note.
    session=json.loads(prev.read_text().replace(str(old),str(root)))
    write_json(out/"seed.json",session)
    shutil.copyfile(parent/"task.txt",out/"task.txt")
    assert sha(out/"task.txt")==plan["prompt_sha256"]
    return dict(r18_declaration=plan["r18_declaration"],r18_seal=plan["r18_seal"],base=plan["base"],
        parent_receipt_sha256=sha(parent/"receipt.json"),prior_task_requests=5,parent_session_sha256=sha(prev),
        seed_sha256=sha(out/"seed.json"),inventory_sha256=sha(out/"initial-inventory.json"),prompt_sha256=sha(out/"task.txt"))


if __name__=="__main__":
    print(json.dumps(prepare(*(Path(arg).resolve() for arg in sys.argv[1:])),indent=2))
