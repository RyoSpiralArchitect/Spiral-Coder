"""Build a published cumulative patch and exercise its documented npm commands without a model."""

import argparse
import json
from pathlib import Path
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[3]
EVAL = REPO / "docs/evals/2026-11-03-catalog-route"
sys.path.insert(0, str(REPO / "scripts"))
from coupled_bench.common import read_json, sha
from self_dogfood.contracts import execution_env, git_snapshot
from self_dogfood.runner import cache_lock

sys.path.insert(0, str(HERE))
from guide_visible import discrepancies, measured


def replay(stage, cache):
    record = EVAL / ("checkpoint" if stage == "delivery" else "guide/intent")
    proof = read_json(record / "combined-verification.json")
    patch = record / "combined.patch"
    if sha(patch) != proof["patch_sha256"] or not proof["inventory_matches"]:
        raise ValueError("combined patch does not match its original-tree replay")
    if cache and cache.is_relative_to(REPO):
        raise ValueError("choose a disposable Cargo cache outside the checkout")
    with tempfile.TemporaryDirectory() as tmp:
        scratch = Path(tmp).resolve()
        root = scratch / "candidate"
        target = cache or (scratch / "target")
        git_snapshot(REPO, proof["original_baseline_commit"], root)
        subprocess.run(
            ["git", "apply", "--binary", "--whitespace=nowarn", str(patch)],
            cwd=root,
            check=True,
        )
        env = execution_env(scratch, target)
        with cache_lock(target):
            if cache:
                subprocess.run(
                    ["cargo", "clean", "--locked", "-p", "spiral-coder"],
                    cwd=root,
                    env=env,
                    check=True,
                )
            subprocess.run(
                ["cargo", "build", "--locked", "--bin", "spiral-coder"],
                cwd=root,
                env=env,
                check=True,
            )
            observed = measured(target / "debug/spiral-coder", root)
            issues = discrepancies((root / "docs/evaluation.md").read_text(), observed)
    return {
        "stage": stage,
        "patch_sha256": sha(patch),
        "package_json_output": json.loads(observed["listed"]["stdout"]),
        "no_match_exit": observed["empty"]["exit"],
        "no_match_stdout": observed["empty"]["stdout"],
        "no_match_stderr": observed["empty"]["stderr"].splitlines()[0],
        "guide_discrepancies": issues,
        "provider_requests": 0,
    }


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--stage", choices=["delivery", "guide"], default="delivery")
    p.add_argument(
        "--target-dir",
        type=Path,
        help="optional dedicated Cargo cache; rebuilds the root crate",
    )
    args = p.parse_args()
    print(
        json.dumps(
            replay(args.stage, args.target_dir.resolve() if args.target_dir else None),
            indent=2,
        )
    )
