"""Run the predeclared native chain once and seal the resulting source and receipts."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parents[2] / "scripts"))
from coupled_bench.common import inventory, read_json, sha, write_json
from self_dogfood.contracts import execution_env
from self_dogfood.process import run_process
from self_dogfood.runner import cache_lock


def run(out, binary, target):
    plan = read_json(HERE / "plan.json")
    root = out / "workspace"
    assert sha(binary) == plan["executor_sha256"]
    assert sha(out / "seed.json") == plan["seed_sha256"] and sha(out / "task.txt") == plan["prompt_sha256"]
    assert sha(out / "initial-inventory.json") == plan["inventory_sha256"]
    assert inventory(root, include_nested_git=True) == read_json(out / "initial-inventory.json")
    assert not (out / "run").exists() and not (out / "started.json").exists()
    key = os.environ["OPENAI_API_KEY"].strip()
    assert key
    env = execution_env(out, target, binary)
    env["OPENAI_API_KEY"] = key
    spec = plan["limits"]
    args = [binary,"--provider","openai","--model",plan["model"],"--base-url","https://api.openai.com/v1",
            "--temperature","0","--max-tokens","32768","--timeout-seconds","90","coupled-run",
            "--tool-root",root,"--seed-session",out/"seed.json","--prompt-file",out/"task.txt","--out-dir",out/"run",
            "--iterations",str(spec["iterations"]),"--max-requests",str(spec["task_requests"]),
            "--coder-chunk-requests","8","--observer-reserved-requests","8","--transport-repair-requests","4",
            "--observer-provider","openai","--observer-model",plan["model"],"--observer-base-url","https://api.openai.com/v1",
            "--observer-api-key-env","OPENAI_API_KEY","--observer-max-tokens","8192"]
    with cache_lock(target):
        subprocess.run(["cargo","clean","--locked","--offline","-p","spiral-coder"], cwd=root, env=env, check=True, capture_output=True)
        with (out/"started.json").open("x") as f:
            json.dump(dict(plan_sha256=sha(HERE/"plan.json"), started_at=time.time(), argv=list(map(str,args))), f)
        process = run_process(list(map(str,args)), root, env, spec["wall_seconds"], out/"console.log", [key])
    redacted = []
    for filename in [p for p in (out/"run").rglob("*") if p.is_file() and not p.is_symlink()] + [
        root / p for p, info in inventory(root).items() if "sha256" in info]:
        data = filename.read_bytes()
        if key.encode() in data:
            filename.write_bytes(data.replace(key.encode(), b"[REDACTED]"))
            redacted.append(str(filename.relative_to(out)))
    subprocess.run(["git","add","-N","."], cwd=root, env=env, check=True, capture_output=True)
    patch = subprocess.check_output(["git","diff","--binary",plan["base"],"--"], cwd=root, env=env)
    (out/"composite.patch").write_bytes(patch)
    write_json(out/"receipt.json", dict(plan_sha256=sha(HERE/"plan.json"), process=process, redacted_files=redacted,
               run_inventory=inventory(out/"run"), final_inventory=inventory(root,include_nested_git=True),
               patch_sha256=sha(out/"composite.patch")))
    return dict(receipt_sha256=sha(out/"receipt.json"), process=process, redactions=len(redacted))


if __name__ == "__main__":
    p=argparse.ArgumentParser(description=__doc__)
    for name in ("out","binary","target"):
        p.add_argument(name,type=Path)
    args=p.parse_args()
    print(json.dumps(run(args.out.resolve(),args.binary.resolve(),args.target.resolve()), indent=2))
