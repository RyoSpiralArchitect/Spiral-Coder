"""Reconstruct the R16 source and truncated native session for a new continuation."""

import argparse
import json
from pathlib import Path
import shutil
import subprocess
import sys

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[3]
ARCHIVE = HERE.parents[1] / "2026-11-03-catalog-route/r16"
sys.path.insert(0, str(REPO / "scripts"))
from coupled_bench.common import inventory, read_json, sha
from self_dogfood.evidence import bound_run


def prepare(raw, out):
    if out.is_relative_to(REPO) or out.is_relative_to(raw):
        raise ValueError(
            "Choose a fresh directory outside the checkout and sealed episode"
        )
    baseline = read_json(ARCHIVE / "plan.json")["declaration_sha256"]
    seal = read_json(ARCHIVE / "results.json")["run_receipt_sha256"]
    assert bound_run(raw, baseline, seal) and sha(raw / "predeclared.json") == baseline
    run = raw / "episode/run"
    report = read_json(run / "report.json")
    source = read_json(run / "session.json")
    held = next(a for a in report["execution"]["actions"] if a["result_step"] == 22)
    assert held["tool"] == "patch_file" and not held["executed"]
    assert report["execution"]["mutation_step"] == 21
    end = next(
        i
        for i, m in enumerate(source["messages"])
        if m.get("tool_call_id") == held["call_id"]
    )
    source["messages"] = source["messages"][: end + 1]
    out.mkdir()
    root = out / "workspace"
    shutil.copytree(raw / "baseline", root)
    assert inventory(root) == read_json(raw / "predeclared.json")["baseline_inventory"]
    subprocess.run(["git", "init", "-q"], cwd=root, check=True)
    subprocess.run(["git", "add", "-f", "."], cwd=root, check=True)

    def commit(label):
        subprocess.run(
            [
                "git",
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@localhost",
                "-c",
                "commit.gpgSign=false",
                "commit",
                "-qm",
                label,
            ],
            cwd=root,
            check=True,
        )

    commit("Original fixture")
    patch = ARCHIVE / "candidate.patch"
    assert sha(patch) == read_json(ARCHIVE / "plain-replay.json")["patch_sha256"]
    subprocess.run(["git", "apply", "--binary", patch], cwd=root, check=True)
    files = inventory(root, include_nested_git=True)
    assert files == read_json(raw / "episode/candidate-inventory.json")
    subprocess.run(["git", "add", "-f", "."], cwd=root, check=True)
    commit("Exact R16 seed")
    seed_commit = subprocess.check_output(
        ["git", "rev-parse", "HEAD"], cwd=root, text=True
    ).strip()
    with (out / "initial-inventory.json").open("w") as f:
        json.dump(files, f, indent=2)
    (out / "seed.json").write_text(
        json.dumps(source, ensure_ascii=False).replace(
            str(raw / "episode/workspace"), str(root)
        )
    )
    shutil.copy2(raw / "episode/task.txt", out / "task.txt")
    return {
        "r16_declaration_sha256": baseline,
        "r16_run_sha256": seal,
        "r16_patch_sha256": sha(patch),
        "held_call_id": held["call_id"],
        "source_step": 22,
        "seed_commit": seed_commit,
        "seed_session_sha256": sha(out / "seed.json"),
        "seed_inventory_sha256": sha(out / "initial-inventory.json"),
        "task_sha256": sha(out / "task.txt"),
    }


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("r16", type=Path)
    p.add_argument("out", type=Path)
    args = p.parse_args()
    print(json.dumps(prepare(args.r16.resolve(), args.out.resolve()), indent=2))
