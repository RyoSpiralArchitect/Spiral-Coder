"""Run the published pilot-cli fixture through one native multi-iteration command."""
import argparse
import os
from pathlib import Path
import shutil
import sys

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[3]
sys.path.insert(0, str(REPO / "scripts"))
from coupled_bench.common import clean_env, inventory, read_json, sha, write_json
from self_dogfood.process import run_process


def run(binary, dest):
    plan = read_json(HERE / "plan.json")
    case = REPO / plan["fixture"]
    assert sha(binary) == plan["executor_sha256"]
    assert sha(__file__) == plan["runner_sha256"]
    assert sha(case / "task.txt") == plan["prompt_sha256"]
    assert inventory(case / "workspace") == plan["baseline_inventory"]
    key = os.environ["OPENAI_API_KEY"].strip()
    assert key
    dest.mkdir()
    root = dest / "workspace"
    shutil.copytree(case / "workspace", root)
    shutil.copyfile(case / "task.txt", dest / "task.txt")
    assert inventory(root) == plan["baseline_inventory"]
    limits = plan["limits"]
    args = [binary, "--provider", "openai", "--model", plan["model"],
            "--base-url", "https://api.openai.com/v1", "--temperature", "0",
            "--max-tokens", "32768", "--timeout-seconds", "90", "coupled-run",
            "--tool-root", root, "--prompt-file", dest / "task.txt", "--out-dir", dest / "run",
            "--max-requests", str(limits["requests"]), "--iterations", str(limits["iterations"]),
            "--coder-chunk-requests", str(limits["chunk"]),
            "--observer-reserved-requests", str(limits["observer"]),
            "--transport-repair-requests", str(limits["transport"]),
            "--observer-provider", "openai", "--observer-model", plan["model"],
            "--observer-base-url", "https://api.openai.com/v1",
            "--observer-api-key-env", "OPENAI_API_KEY", "--observer-max-tokens", "8192"]
    write_json(dest / "started.json", {"plan_sha256": sha(HERE / "plan.json"), "argv": list(map(str, args))})
    env = clean_env(binary)
    env.update(OPENAI_API_KEY=key, PYTHONDONTWRITEBYTECODE="1",
               PYTHONPYCACHEPREFIX=str(dest / "cache"))
    process = run_process(list(map(str, args)), root, env, limits["wall_seconds"], dest / "console.log", [key])
    for directory in (dest / "run", root):
        for path in directory.rglob("*"):
            if path.is_file() and not path.is_symlink() and key.encode() in path.read_bytes():
                raise RuntimeError("Inspect the private run before publishing any output")
    write_json(dest / "receipt.json", {"plan_sha256": sha(HERE / "plan.json"), "process": process,
               "run_inventory": inventory(dest / "run"), "final_inventory": inventory(root)})


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("binary", type=Path)
    p.add_argument("dest", type=Path)
    args = p.parse_args()
    run(args.binary.resolve(), args.dest.resolve())
